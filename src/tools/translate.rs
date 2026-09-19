use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::model::translation::{CompletedTranslation, SubmitResult};
use crate::service::{formatter, merger, parser, validator};
use crate::tools::parse::CachedFile;
use crate::tools::submit_response;
use crate::tools::{FileCache, mcp_log};
use std::path::PathBuf;

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SubmitTranslationsParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Translations to submit. Each entry needs key, locale, and either value (simple strings) or plural_forms (plural keys). Definite format argument components must match; ambiguous percent-in-prose differences are accepted with warnings[].
    pub translations: Vec<CompletedTranslation>,
    /// If true, validate without writing to disk
    #[serde(default)]
    pub dry_run: bool,
    /// If true (default), write accepted translations even when some are rejected.
    /// If false, reject ALL translations when any single one fails validation.
    #[serde(default = "default_true")]
    pub continue_on_error: bool,
}

/// Submit translations: validate, merge, and write back.
pub(crate) async fn handle_submit_translations(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    write_lock: &Mutex<()>,
    params: SubmitTranslationsParams,
) -> Result<serde_json::Value, XcStringsError> {
    let path = match &params.file_path {
        Some(path) => PathBuf::from(path),
        None => cache
            .lock()
            .await
            .active_path()
            .cloned()
            .ok_or(XcStringsError::NoActiveFile)?,
    };
    if path.extension().and_then(|extension| extension.to_str()) != Some("xcstrings") {
        return Err(XcStringsError::NotXcStrings { path });
    }
    let identity = store.file_identity(&path)?;
    let _write_guard = write_lock.lock().await;
    let expected = store.read_bytes(&identity)?;
    let raw = std::str::from_utf8(&expected)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let mut file = parser::parse(raw.strip_prefix('\u{feff}').unwrap_or(raw))?;
    let validation = validator::validate_translations_detailed(&file, &params.translations);
    if !params.continue_on_error && !validation.rejected.is_empty() {
        let rejected = params
            .translations
            .iter()
            .enumerate()
            .map(|(index, request)| {
                validation
                    .rejected_indices
                    .iter()
                    .position(|rejected| *rejected == index)
                    .and_then(|position| validation.rejected.get(position))
                    .cloned()
                    .unwrap_or_else(|| {
                        crate::service::submission::reject(
                            request,
                            "batch_rejected",
                            "batch rejected due to other failures",
                        )
                    })
            })
            .collect();
        return submit_response::to_value(
            SubmitResult {
                rejected,
                dry_run: params.dry_run,
                ..Default::default()
            },
            validation.warnings,
        );
    }
    let accepted: Vec<_> = params
        .translations
        .iter()
        .enumerate()
        .filter(|(index, _)| !validation.rejected_indices.contains(index))
        .map(|(_, request)| request.clone())
        .collect();
    let mut result = merger::merge_translations(&mut file, &accepted);
    result.rejected.extend(validation.rejected);
    result.dry_run = params.dry_run;
    if !params.continue_on_error && !result.rejected.is_empty() {
        let rejected = params
            .translations
            .iter()
            .map(|request| {
                result.rejected.iter().find(|rejected| {
                rejected.key == request.key
                    && rejected.locale.as_deref() == Some(request.locale.as_str())
                    && rejected.path == request.path
            }).cloned().unwrap_or_else(|| crate::service::submission::reject(
                request,
                "batch_rejected",
                "batch rejected because combined translations violate catalog constraints",
            ))
            })
            .collect();
        return submit_response::to_value(
            SubmitResult {
                rejected,
                dry_run: params.dry_run,
                ..Default::default()
            },
            validation.warnings,
        );
    }
    if !params.dry_run && result.accepted > 0 {
        let formatted = formatter::format_xcstrings(&file)?;
        store.write_if_matches(&identity, Some(&expected), &formatted)?;
        match store.modified_time(&identity) {
            Ok(modified) => cache.lock().await.insert(
                identity,
                CachedFile {
                    path,
                    content: file,
                    modified,
                },
            ),
            Err(error) => {
                // The data write committed; discard stale cache state without reporting a false failure.
                cache.lock().await.files.remove(&identity);
                mcp_log(&format!(
                    "Translations saved; cache invalidated because metadata could not be read: {error}"
                ));
            }
        }
    }
    mcp_log(&format!(
        "{} accepted, {} rejected",
        result.accepted,
        result.rejected.len()
    ));
    submit_response::to_value(result, validation.warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::translation::CompletedTranslation;
    use crate::tools::parse::{ParseParams, handle_parse};
    use crate::tools::test_helpers::{MIXED_SPECIFIER_FIXTURE, MemoryStore, SIMPLE_FIXTURE};
    use std::path::Path;

    #[tokio::test]
    async fn test_submit_dry_run() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());
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
                ..Default::default()
            }],
            dry_run: true,
            continue_on_error: true,
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
        let cache = Mutex::new(FileCache::new());
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
                ..Default::default()
            }],
            dry_run: false,
            continue_on_error: true,
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
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", MIXED_SPECIFIER_FIXTURE);
        let cache = Mutex::new(FileCache::new());
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
                ..Default::default()
            }],
            dry_run: false,
            continue_on_error: true,
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
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let params = SubmitTranslationsParams {
            file_path: None,
            translations: vec![],
            dry_run: false,
            continue_on_error: true,
        };
        let result = handle_submit_translations(&store, &cache, &write_lock, params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_continue_on_error_false_rejects_all() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", MIXED_SPECIFIER_FIXTURE);
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let parse_params = ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, parse_params).await.unwrap();

        let params = SubmitTranslationsParams {
            file_path: None,
            translations: vec![
                CompletedTranslation {
                    key: "greeting".to_string(),
                    locale: "de".to_string(),
                    // Missing %@ — should be rejected
                    value: "Hallo".to_string(),
                    plural_forms: None,
                    substitution_name: None,
                    ..Default::default()
                },
                CompletedTranslation {
                    key: "farewell".to_string(),
                    locale: "de".to_string(),
                    value: "Tschuess".to_string(),
                    plural_forms: None,
                    substitution_name: None,
                    ..Default::default()
                },
            ],
            dry_run: false,
            continue_on_error: false,
        };

        let result = handle_submit_translations(&store, &cache, &write_lock, params)
            .await
            .unwrap();
        assert_eq!(result["accepted"], 0);
        // All should be rejected (both greeting and farewell)
        assert_eq!(result["rejected"].as_array().unwrap().len(), 2);

        // File should NOT have been written
        let content = store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap();
        assert!(!content.contains("Tschuess"));
    }

    #[tokio::test]
    async fn test_continue_on_error_true_writes_valid() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", MIXED_SPECIFIER_FIXTURE);
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let parse_params = ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, parse_params).await.unwrap();

        let params = SubmitTranslationsParams {
            file_path: None,
            translations: vec![
                CompletedTranslation {
                    key: "greeting".to_string(),
                    locale: "de".to_string(),
                    value: "Hallo".to_string(),
                    plural_forms: None,
                    substitution_name: None,
                    ..Default::default()
                },
                CompletedTranslation {
                    key: "farewell".to_string(),
                    locale: "de".to_string(),
                    value: "Tschuess".to_string(),
                    plural_forms: None,
                    substitution_name: None,
                    ..Default::default()
                },
            ],
            dry_run: false,
            continue_on_error: true,
        };

        let result = handle_submit_translations(&store, &cache, &write_lock, params)
            .await
            .unwrap();
        // "farewell" accepted, "greeting" rejected (missing %@)
        assert_eq!(result["accepted"], 1);
        assert!(!result["rejected"].as_array().unwrap().is_empty());

        // File should have farewell written
        let content = store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap();
        assert!(content.contains("Tschuess"));
    }

    #[tokio::test]
    async fn test_continue_on_error_default_is_true() {
        // Test that deserialization defaults to true
        let json = r#"{
            "translations": [],
            "dry_run": true
        }"#;
        let params: SubmitTranslationsParams = serde_json::from_str(json).unwrap();
        assert!(params.continue_on_error);
    }

    #[tokio::test]
    async fn test_accepted_keys_returned() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());
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
                ..Default::default()
            }],
            dry_run: false,
            continue_on_error: true,
        };

        let result = handle_submit_translations(&store, &cache, &write_lock, params)
            .await
            .unwrap();
        let accepted_keys = result["accepted_keys"].as_array().unwrap();
        assert_eq!(accepted_keys.len(), 1);
        assert_eq!(accepted_keys[0], "welcome_message");
    }
}

#[cfg(test)]
mod cas_tests;
