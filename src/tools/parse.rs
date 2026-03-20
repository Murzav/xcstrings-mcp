use std::path::PathBuf;
use std::time::SystemTime;

use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::model::xcstrings::XcStringsFile;
use crate::service::{formatter, parser};

/// Cached parsed file, shared across tool invocations.
#[allow(
    dead_code,
    reason = "modified field reserved for future mtime-based cache invalidation"
)]
pub(crate) struct CachedFile {
    pub path: PathBuf,
    pub content: XcStringsFile,
    pub modified: SystemTime,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ParseParams {
    /// Absolute path to the .xcstrings file
    pub file_path: String,
}

/// Parse an .xcstrings file and return a summary.
/// Caches the parsed file for subsequent tool calls.
pub(crate) async fn handle_parse(
    store: &dyn FileStore,
    cache: &Mutex<Option<CachedFile>>,
    params: ParseParams,
) -> Result<serde_json::Value, XcStringsError> {
    let path = PathBuf::from(&params.file_path);

    match path.extension().and_then(|e| e.to_str()) {
        Some("xcstrings") => {}
        _ => return Err(XcStringsError::NotXcStrings { path }),
    }

    let raw = store.read(&path)?;
    let file = parser::parse(&raw)?;
    let summary = parser::summarize(&file);
    let mtime = store.modified_time(&path)?;

    // Verify we can format (catches issues early)
    let _ = formatter::format_xcstrings(&file)?;

    let mut guard = cache.lock().await;
    *guard = Some(CachedFile {
        path,
        content: file,
        modified: mtime,
    });

    Ok(serde_json::to_value(summary)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::test_helpers::{MemoryStore, SIMPLE_FIXTURE};

    #[tokio::test]
    async fn test_handle_parse_success() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(None);

        let params = ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        let result = handle_parse(&store, &cache, params).await.unwrap();

        assert_eq!(result["source_language"], "en");
        assert_eq!(result["total_keys"], 2);
        assert_eq!(result["translatable_keys"], 2);

        let guard = cache.lock().await;
        assert!(guard.is_some());
        assert_eq!(
            guard.as_ref().unwrap().path,
            PathBuf::from("/test/file.xcstrings")
        );
    }

    #[tokio::test]
    async fn test_handle_parse_not_xcstrings() {
        let store = MemoryStore::new();
        store.add_file("/test/file.json", "{}");
        let cache = Mutex::new(None);

        let params = ParseParams {
            file_path: "/test/file.json".to_string(),
        };
        let result = handle_parse(&store, &cache, params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_parse_file_not_found() {
        let store = MemoryStore::new();
        let cache = Mutex::new(None);

        let params = ParseParams {
            file_path: "/nonexistent.xcstrings".to_string(),
        };
        let result = handle_parse(&store, &cache, params).await;
        assert!(result.is_err());
    }
}
