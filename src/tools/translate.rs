use rmcp::RoleServer;
use rmcp::model::LoggingLevel;
use rmcp::service::Peer;
use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::model::translation::{CompletedTranslation, RejectedTranslation, SubmitResult};
use crate::service::{formatter, merger, parser, validator};
use crate::tools::parse::CachedFile;
use crate::tools::resolve_file;
use crate::tools::{FileCache, mcp_log};

fn default_true() -> bool {
    true
}

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
    peer: Option<&Peer<RoleServer>>,
) -> Result<serde_json::Value, XcStringsError> {
    let (path, file) = resolve_file(store, cache, params.file_path.as_deref()).await?;

    mcp_log(
        peer,
        LoggingLevel::Info,
        &format!("Validating {} translations...", params.translations.len()),
    )
    .await;

    // Validate all translations against the file
    let rejected = validator::validate_translations(&file, &params.translations);

    // If continue_on_error=false and any rejected, return ALL as rejected without writing
    if !params.continue_on_error && !rejected.is_empty() {
        let all_rejected: Vec<RejectedTranslation> = params
            .translations
            .iter()
            .map(|t| {
                // Find the specific rejection reason, or mark as "batch rejected"
                let reason = rejected
                    .iter()
                    .find(|r| r.key == t.key)
                    .map(|r| r.reason.clone())
                    .unwrap_or_else(|| "batch rejected due to other failures".into());
                RejectedTranslation {
                    key: t.key.clone(),
                    reason,
                }
            })
            .collect();
        let result = SubmitResult {
            accepted: 0,
            rejected: all_rejected,
            dry_run: params.dry_run,
            accepted_keys: Vec::new(),
        };
        return Ok(serde_json::to_value(result)?);
    }

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
        let accepted_key_list: Vec<String> = accepted_translations
            .iter()
            .map(|t| t.key.clone())
            .collect();
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
            accepted_keys: accepted_key_list,
        };
        return Ok(serde_json::to_value(result)?);
    }

    if accepted_count == 0 {
        let result = SubmitResult {
            accepted: 0,
            rejected,
            dry_run: false,
            accepted_keys: Vec::new(),
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

    // If continue_on_error=false and fresh re-validation rejects anything, abort
    if !params.continue_on_error && !fresh_rejected.is_empty() {
        let mut all_rejected = rejected;
        all_rejected.extend(fresh_rejected);
        let all_keys: Vec<String> = params.translations.iter().map(|t| t.key.clone()).collect();
        let all_rejected_out: Vec<RejectedTranslation> = all_keys
            .into_iter()
            .map(|key| {
                let reason = all_rejected
                    .iter()
                    .find(|r| r.key == key)
                    .map(|r| r.reason.clone())
                    .unwrap_or_else(|| "batch rejected due to other failures".into());
                RejectedTranslation { key, reason }
            })
            .collect();
        let result = SubmitResult {
            accepted: 0,
            rejected: all_rejected_out,
            dry_run: false,
            accepted_keys: Vec::new(),
        };
        return Ok(serde_json::to_value(result)?);
    }

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
            accepted_keys: Vec::new(),
        };
        return Ok(serde_json::to_value(result)?);
    }

    let merge_result = merger::merge_translations(&mut fresh_file, &owned);

    mcp_log(
        peer,
        LoggingLevel::Info,
        &format!(
            "{} accepted, {} rejected",
            merge_result.accepted,
            rejected.len() + fresh_rejected.len() + merge_result.rejected.len()
        ),
    )
    .await;

    // Format and write
    let formatted = formatter::format_xcstrings(&fresh_file)?;
    store.write(&path, &formatted)?;

    // Update cache
    let mtime = store.modified_time(&path)?;
    let mut guard = cache.lock().await;
    guard.insert(
        path.clone(),
        CachedFile {
            path,
            content: fresh_file,
            modified: mtime,
        },
    );

    // Combine all rejections (initial validation + fresh re-validation + merge)
    let mut all_rejected = rejected;
    all_rejected.extend(fresh_rejected);
    all_rejected.extend(merge_result.rejected);

    let result = SubmitResult {
        accepted: merge_result.accepted,
        rejected: all_rejected,
        dry_run: false,
        accepted_keys: merge_result.accepted_keys,
    };

    Ok(serde_json::to_value(result)?)
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
        handle_parse(&store, &cache, parse_params, None)
            .await
            .unwrap();

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
            continue_on_error: true,
        };

        let result = handle_submit_translations(&store, &cache, &write_lock, params, None)
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
        handle_parse(&store, &cache, parse_params, None)
            .await
            .unwrap();

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
            continue_on_error: true,
        };

        let result = handle_submit_translations(&store, &cache, &write_lock, params, None)
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
        handle_parse(&store, &cache, parse_params, None)
            .await
            .unwrap();

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
            continue_on_error: true,
        };

        let result = handle_submit_translations(&store, &cache, &write_lock, params, None)
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
        let result = handle_submit_translations(&store, &cache, &write_lock, params, None).await;
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
        handle_parse(&store, &cache, parse_params, None)
            .await
            .unwrap();

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
                },
                CompletedTranslation {
                    key: "farewell".to_string(),
                    locale: "de".to_string(),
                    value: "Tschuess".to_string(),
                    plural_forms: None,
                    substitution_name: None,
                },
            ],
            dry_run: false,
            continue_on_error: false,
        };

        let result = handle_submit_translations(&store, &cache, &write_lock, params, None)
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
        handle_parse(&store, &cache, parse_params, None)
            .await
            .unwrap();

        let params = SubmitTranslationsParams {
            file_path: None,
            translations: vec![
                CompletedTranslation {
                    key: "greeting".to_string(),
                    locale: "de".to_string(),
                    value: "Hallo".to_string(),
                    plural_forms: None,
                    substitution_name: None,
                },
                CompletedTranslation {
                    key: "farewell".to_string(),
                    locale: "de".to_string(),
                    value: "Tschuess".to_string(),
                    plural_forms: None,
                    substitution_name: None,
                },
            ],
            dry_run: false,
            continue_on_error: true,
        };

        let result = handle_submit_translations(&store, &cache, &write_lock, params, None)
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
        handle_parse(&store, &cache, parse_params, None)
            .await
            .unwrap();

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
            continue_on_error: true,
        };

        let result = handle_submit_translations(&store, &cache, &write_lock, params, None)
            .await
            .unwrap();
        let accepted_keys = result["accepted_keys"].as_array().unwrap();
        assert_eq!(accepted_keys.len(), 1);
        assert_eq!(accepted_keys[0], "welcome_message");
    }
}
