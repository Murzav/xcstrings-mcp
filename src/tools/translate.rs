use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::model::translation::{CompletedTranslation, RejectedTranslation, SubmitResult};
use crate::service::{formatter, merger, parser, validator};
use crate::tools::parse::CachedFile;
use crate::tools::resolve_file;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SubmitTranslationsParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Translations to submit
    pub translations: Vec<CompletedTranslation>,
    /// If true, validate without writing to disk
    #[serde(default)]
    pub dry_run: bool,
}

/// Submit translations: validate, merge, and write back.
pub(crate) async fn handle_submit_translations(
    store: &dyn FileStore,
    cache: &Mutex<Option<CachedFile>>,
    write_lock: &Mutex<()>,
    params: SubmitTranslationsParams,
) -> Result<serde_json::Value, XcStringsError> {
    let (path, file) = resolve_file(store, cache, params.file_path.as_deref()).await?;

    // Validate all translations against the file
    let rejected = validator::validate_translations(&file, &params.translations);

    // Build set of rejected keys to filter them out
    let rejected_keys: std::collections::HashSet<&str> =
        rejected.iter().map(|r| r.key.as_str()).collect();

    let accepted_translations: Vec<&CompletedTranslation> = params
        .translations
        .iter()
        .filter(|t| !rejected_keys.contains(t.key.as_str()))
        .collect();

    let accepted_count = accepted_translations.len();

    if params.dry_run {
        let result = SubmitResult {
            accepted: accepted_count,
            rejected: rejected
                .into_iter()
                .map(|r| RejectedTranslation {
                    key: r.key,
                    reason: r.reason,
                })
                .collect(),
            dry_run: true,
        };
        return Ok(serde_json::to_value(result)?);
    }

    if accepted_count == 0 {
        let result = SubmitResult {
            accepted: 0,
            rejected,
            dry_run: false,
        };
        return Ok(serde_json::to_value(result)?);
    }

    // Acquire write lock for safe concurrent access
    let _write_guard = write_lock.lock().await;

    // Re-read from disk to get latest state and re-validate against fresh file
    let raw = store.read(&path)?;
    let mut fresh_file = parser::parse(&raw)?;

    // Re-validate against fresh file (it may have changed since initial validation)
    let fresh_rejected = validator::validate_translations(&fresh_file, &params.translations);
    let fresh_rejected_keys: std::collections::HashSet<&str> =
        fresh_rejected.iter().map(|r| r.key.as_str()).collect();

    let owned: Vec<CompletedTranslation> = accepted_translations
        .into_iter()
        .filter(|t| !fresh_rejected_keys.contains(t.key.as_str()))
        .cloned()
        .collect();

    if owned.is_empty() {
        let mut all_rejected = rejected;
        all_rejected.extend(fresh_rejected);
        let result = SubmitResult {
            accepted: 0,
            rejected: all_rejected,
            dry_run: false,
        };
        return Ok(serde_json::to_value(result)?);
    }

    let merge_result = merger::merge_translations(&mut fresh_file, &owned);

    // Format and write
    let formatted = formatter::format_xcstrings(&fresh_file)?;
    store.write(&path, &formatted)?;

    // Update cache
    let mtime = store.modified_time(&path)?;
    let mut guard = cache.lock().await;
    *guard = Some(CachedFile {
        path,
        content: fresh_file,
        modified: mtime,
    });

    // Combine all rejections (initial validation + fresh re-validation + merge)
    let mut all_rejected = rejected;
    all_rejected.extend(fresh_rejected);
    all_rejected.extend(merge_result.rejected);

    let result = SubmitResult {
        accepted: merge_result.accepted,
        rejected: all_rejected,
        dry_run: false,
    };

    Ok(serde_json::to_value(result)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::translation::CompletedTranslation;
    use crate::tools::parse::{ParseParams, handle_parse};
    use crate::tools::test_helpers::{MemoryStore, SIMPLE_FIXTURE};
    use std::path::Path;

    #[tokio::test]
    async fn test_submit_dry_run() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(None);
        let write_lock = Mutex::new(());

        let parse_params = ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, parse_params).await.unwrap();

        let params = SubmitTranslationsParams {
            file_path: None,
            translations: vec![CompletedTranslation {
                key: "welcome_message".to_string(),
                locale: "de".to_string(),
                value: "Willkommen in der App".to_string(),
                plural_forms: None,
                substitution_name: None,
            }],
            dry_run: true,
        };

        let result = handle_submit_translations(&store, &cache, &write_lock, params)
            .await
            .unwrap();
        assert_eq!(result["dry_run"], true);
        assert_eq!(result["accepted"], 1);

        let content = store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap();
        assert!(!content.contains("Willkommen"));
    }

    #[tokio::test]
    async fn test_submit_writes_file() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(None);
        let write_lock = Mutex::new(());

        let parse_params = ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, parse_params).await.unwrap();

        let params = SubmitTranslationsParams {
            file_path: None,
            translations: vec![CompletedTranslation {
                key: "welcome_message".to_string(),
                locale: "de".to_string(),
                value: "Willkommen in der App".to_string(),
                plural_forms: None,
                substitution_name: None,
            }],
            dry_run: false,
        };

        let result = handle_submit_translations(&store, &cache, &write_lock, params)
            .await
            .unwrap();
        assert_eq!(result["accepted"], 1);
        assert_eq!(result["dry_run"], false);

        let content = store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap();
        assert!(content.contains("Willkommen"));
    }

    #[tokio::test]
    async fn test_submit_rejects_invalid_specifier() {
        let fixture = r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "greeting" : {
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "Hello %@"
          }
        }
      }
    }
  },
  "version" : "1.0"
}"#;
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", fixture);
        let cache = Mutex::new(None);
        let write_lock = Mutex::new(());

        let parse_params = ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, parse_params).await.unwrap();

        let params = SubmitTranslationsParams {
            file_path: None,
            translations: vec![CompletedTranslation {
                key: "greeting".to_string(),
                locale: "de".to_string(),
                value: "Hallo".to_string(),
                plural_forms: None,
                substitution_name: None,
            }],
            dry_run: false,
        };

        let result = handle_submit_translations(&store, &cache, &write_lock, params)
            .await
            .unwrap();
        assert_eq!(result["accepted"], 0);
        assert!(!result["rejected"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_submit_no_active_file() {
        let store = MemoryStore::new();
        let cache = Mutex::new(None);
        let write_lock = Mutex::new(());

        let params = SubmitTranslationsParams {
            file_path: None,
            translations: vec![],
            dry_run: false,
        };
        let result = handle_submit_translations(&store, &cache, &write_lock, params).await;
        assert!(result.is_err());
    }
}
