use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::model::translation::TranslationUnit;
use crate::service::extractor;
use crate::tools::parse::CachedFile;
use crate::tools::resolve_file;

fn default_batch_size() -> usize {
    30
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetUntranslatedParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Target locale code (e.g., "uk", "de")
    pub locale: String,
    /// Number of strings per batch (1-100, default 30)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Offset for pagination (default 0)
    #[serde(default)]
    pub offset: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct GetUntranslatedResult {
    pub units: Vec<TranslationUnit>,
    pub total: usize,
    pub offset: usize,
    pub batch_size: usize,
    pub has_more: bool,
}

/// Extract untranslated strings for a locale with batching.
pub(crate) async fn handle_get_untranslated(
    store: &dyn FileStore,
    cache: &Mutex<Option<CachedFile>>,
    params: GetUntranslatedParams,
) -> Result<serde_json::Value, XcStringsError> {
    let (_path, file) = resolve_file(store, cache, params.file_path.as_deref()).await?;

    let (units, total) =
        extractor::get_untranslated(&file, &params.locale, params.batch_size, params.offset)?;

    let has_more = params.offset + units.len() < total;

    let result = GetUntranslatedResult {
        units,
        total,
        offset: params.offset,
        batch_size: params.batch_size,
        has_more,
    };

    Ok(serde_json::to_value(result)?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetStaleParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Target locale code (e.g., "uk", "de")
    pub locale: String,
    /// Number of strings per batch (1-100, default 30)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Offset for pagination (default 0)
    #[serde(default)]
    pub offset: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct GetStaleResult {
    pub units: Vec<TranslationUnit>,
    pub total: usize,
    pub offset: usize,
    pub batch_size: usize,
    pub has_more: bool,
}

/// Extract stale strings for a locale with batching.
pub(crate) async fn handle_get_stale(
    store: &dyn FileStore,
    cache: &Mutex<Option<CachedFile>>,
    params: GetStaleParams,
) -> Result<serde_json::Value, XcStringsError> {
    let (_path, file) = resolve_file(store, cache, params.file_path.as_deref()).await?;

    let (units, total) =
        extractor::get_stale(&file, &params.locale, params.batch_size, params.offset)?;

    let has_more = params.offset + units.len() < total;

    let result = GetStaleResult {
        units,
        total,
        offset: params.offset,
        batch_size: params.batch_size,
        has_more,
    };

    Ok(serde_json::to_value(result)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::parse::{ParseParams, handle_parse};
    use crate::tools::test_helpers::{MemoryStore, SIMPLE_FIXTURE};

    #[tokio::test]
    async fn test_get_untranslated_with_file_path() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(None);

        let params = GetUntranslatedParams {
            file_path: Some("/test/file.xcstrings".to_string()),
            locale: "de".to_string(),
            batch_size: 30,
            offset: 0,
        };
        let result = handle_get_untranslated(&store, &cache, params)
            .await
            .unwrap();
        assert_eq!(result["total"], 2);
        assert_eq!(result["has_more"], false);
    }

    #[tokio::test]
    async fn test_get_untranslated_from_cache() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(None);

        let parse_params = ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, parse_params).await.unwrap();

        let params = GetUntranslatedParams {
            file_path: None,
            locale: "uk".to_string(),
            batch_size: 30,
            offset: 0,
        };
        let result = handle_get_untranslated(&store, &cache, params)
            .await
            .unwrap();
        assert_eq!(result["total"], 1);
    }

    #[tokio::test]
    async fn test_get_untranslated_no_cache_no_path() {
        let store = MemoryStore::new();
        let cache = Mutex::new(None);

        let params = GetUntranslatedParams {
            file_path: None,
            locale: "de".to_string(),
            batch_size: 30,
            offset: 0,
        };
        let result = handle_get_untranslated(&store, &cache, params).await;
        assert!(result.is_err());
    }

    const STALE_FIXTURE: &str = include_str!("../../tests/fixtures/with_stale.xcstrings");

    #[tokio::test]
    async fn test_get_stale_returns_stale_keys() {
        let store = MemoryStore::new();
        store.add_file("/test/stale.xcstrings", STALE_FIXTURE);
        let cache = Mutex::new(None);

        let params = GetStaleParams {
            file_path: Some("/test/stale.xcstrings".to_string()),
            locale: "uk".to_string(),
            batch_size: 30,
            offset: 0,
        };
        let result = handle_get_stale(&store, &cache, params).await.unwrap();

        // with_stale.xcstrings has 2 stale translatable keys: removed_feature, renamed_key
        assert_eq!(result["total"], 2);
        assert_eq!(result["has_more"], false);
    }

    #[tokio::test]
    async fn test_get_stale_no_stale_keys() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(None);

        let params = GetStaleParams {
            file_path: Some("/test/file.xcstrings".to_string()),
            locale: "uk".to_string(),
            batch_size: 30,
            offset: 0,
        };
        let result = handle_get_stale(&store, &cache, params).await.unwrap();

        assert_eq!(result["total"], 0);
        assert!(result["units"].as_array().unwrap().is_empty());
    }
}
