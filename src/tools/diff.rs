use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::service::{diff, parser};
use crate::tools::FileCache;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetDiffParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
}

/// Compare cached file with the current on-disk version.
/// Returns added keys, removed keys, and keys whose source language text
/// changed. Does not track translation changes in non-source locales.
pub(crate) async fn handle_get_diff(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    params: GetDiffParams,
) -> Result<serde_json::Value, XcStringsError> {
    let guard = cache.lock().await;
    let path = if let Some(ref fp) = params.file_path {
        let p = PathBuf::from(fp);
        match p.extension().and_then(|e| e.to_str()) {
            Some("xcstrings") => {}
            _ => return Err(XcStringsError::NotXcStrings { path: p }),
        }
        p
    } else {
        guard
            .active_path()
            .cloned()
            .ok_or(XcStringsError::NoActiveFile)?
    };

    let cached = guard.get(&path).ok_or(XcStringsError::NoActiveFile)?;
    let old_content = cached.content.clone();
    drop(guard);

    // Read fresh from disk
    let raw = store.read(&path)?;
    let new_content = parser::parse(&raw)?;

    let report = diff::compute_diff(&old_content, &new_content);
    Ok(serde_json::to_value(report)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::FileCache;
    use crate::tools::test_helpers::{MemoryStore, SIMPLE_FIXTURE};

    #[tokio::test]
    async fn test_handle_get_diff_no_changes() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());

        // Parse to populate cache
        let params = crate::tools::parse::ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        crate::tools::parse::handle_parse(&store, &cache, params)
            .await
            .unwrap();

        // Diff with no on-disk changes
        let diff_params = GetDiffParams { file_path: None };
        let result = handle_get_diff(&store, &cache, diff_params).await.unwrap();

        assert!(result["added"].as_array().unwrap().is_empty());
        assert!(result["removed"].as_array().unwrap().is_empty());
        assert!(result["modified"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_handle_get_diff_with_changes() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());

        // Parse to populate cache
        let params = crate::tools::parse::ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        crate::tools::parse::handle_parse(&store, &cache, params)
            .await
            .unwrap();

        // Modify the file on disk: add a new key, remove "welcome_message"
        let modified_fixture = r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "greeting" : {
      "extractionState" : "manual",
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "Hello"
          }
        },
        "uk" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "Привіт"
          }
        }
      }
    },
    "new_key" : {
      "extractionState" : "manual",
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "New string"
          }
        }
      }
    }
  },
  "version" : "1.0"
}"#;
        store.update_file("/test/file.xcstrings", modified_fixture);

        let diff_params = GetDiffParams { file_path: None };
        let result = handle_get_diff(&store, &cache, diff_params).await.unwrap();

        let added: Vec<String> = serde_json::from_value(result["added"].clone()).unwrap();
        let removed: Vec<String> = serde_json::from_value(result["removed"].clone()).unwrap();

        assert!(added.contains(&"new_key".to_string()));
        assert!(removed.contains(&"welcome_message".to_string()));
    }

    #[tokio::test]
    async fn test_handle_get_diff_rejects_non_xcstrings_path() {
        let store = MemoryStore::new();
        let cache = Mutex::new(FileCache::new());

        let diff_params = GetDiffParams {
            file_path: Some("/test/file.json".to_string()),
        };
        let result = handle_get_diff(&store, &cache, diff_params).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, XcStringsError::NotXcStrings { .. }),
            "expected NotXcStrings, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_handle_get_diff_no_cache() {
        let store = MemoryStore::new();
        let cache = Mutex::new(FileCache::new());

        let diff_params = GetDiffParams { file_path: None };
        let result = handle_get_diff(&store, &cache, diff_params).await;
        assert!(result.is_err());
    }
}
