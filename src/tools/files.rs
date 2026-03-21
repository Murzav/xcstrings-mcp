use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::tools::FileCache;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ListFilesParams {}

/// List all cached .xcstrings files with summary info.
pub(crate) async fn handle_list_files(
    cache: &Mutex<FileCache>,
) -> Result<serde_json::Value, XcStringsError> {
    let guard = cache.lock().await;
    let entries = guard.list();
    Ok(serde_json::to_value(entries)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::parse::{ParseParams, handle_parse};
    use crate::tools::test_helpers::{MemoryStore, SIMPLE_FIXTURE};

    #[tokio::test]
    async fn test_list_files_empty_cache() {
        let cache = Mutex::new(FileCache::new());
        let result = handle_list_files(&cache).await.unwrap();
        let arr = result.as_array().unwrap();
        assert!(arr.is_empty());
    }

    #[tokio::test]
    async fn test_list_files_after_parsing_two_files() {
        let store = MemoryStore::new();
        store.add_file("/test/a.xcstrings", SIMPLE_FIXTURE);
        store.add_file("/test/b.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());

        let params_a = ParseParams {
            file_path: "/test/a.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, params_a).await.unwrap();

        let params_b = ParseParams {
            file_path: "/test/b.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, params_b).await.unwrap();

        let result = handle_list_files(&cache).await.unwrap();
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);

        // Last parsed (b) should be active
        let active_count = arr
            .iter()
            .filter(|e| e["is_active"].as_bool() == Some(true))
            .count();
        assert_eq!(active_count, 1);

        let active = arr
            .iter()
            .find(|e| e["is_active"].as_bool() == Some(true))
            .unwrap();
        assert!(
            active["path"]
                .as_str()
                .unwrap()
                .contains("/test/b.xcstrings")
        );

        // Entries must be sorted by path for deterministic output
        let paths: Vec<&str> = arr.iter().map(|e| e["path"].as_str().unwrap()).collect();
        let mut sorted_paths = paths.clone();
        sorted_paths.sort();
        assert_eq!(
            paths, sorted_paths,
            "list_files output must be sorted by path"
        );
    }
}
