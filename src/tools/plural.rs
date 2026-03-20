use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::model::translation::{ContextKey, PluralUnit};
use crate::service::{context, plural_extractor};
use crate::tools::parse::CachedFile;
use crate::tools::resolve_file;

fn default_plural_batch_size() -> usize {
    20
}

fn default_context_count() -> usize {
    5
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetPluralsParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Target locale code (e.g., "uk", "de")
    pub locale: String,
    /// Number of plural keys per batch (1-100, default 20)
    #[serde(default = "default_plural_batch_size")]
    pub batch_size: usize,
    /// Offset for pagination (default 0)
    #[serde(default)]
    pub offset: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct GetPluralsResult {
    pub units: Vec<PluralUnit>,
    pub total: usize,
    pub offset: usize,
    pub batch_size: usize,
    pub has_more: bool,
}

/// Extract plural/device keys needing translation for a locale.
pub(crate) async fn handle_get_plurals(
    store: &dyn FileStore,
    cache: &Mutex<Option<CachedFile>>,
    params: GetPluralsParams,
) -> Result<serde_json::Value, XcStringsError> {
    let (_path, file) = resolve_file(store, cache, params.file_path.as_deref()).await?;

    let (units, total) = plural_extractor::get_untranslated_plurals(
        &file,
        &params.locale,
        params.batch_size,
        params.offset,
    )?;

    let has_more = params.offset + units.len() < total;

    let result = GetPluralsResult {
        units,
        total,
        offset: params.offset,
        batch_size: params.batch_size,
        has_more,
    };

    Ok(serde_json::to_value(result)?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetContextParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// The key to find context for
    pub key: String,
    /// Target locale code (e.g., "uk", "de")
    pub locale: String,
    /// Number of context keys to return (default 5)
    #[serde(default = "default_context_count")]
    pub count: usize,
}

/// Get nearby context keys for a specific key.
pub(crate) async fn handle_get_context(
    store: &dyn FileStore,
    cache: &Mutex<Option<CachedFile>>,
    params: GetContextParams,
) -> Result<serde_json::Value, XcStringsError> {
    let (_path, file) = resolve_file(store, cache, params.file_path.as_deref()).await?;

    let context_keys: Vec<ContextKey> =
        context::get_context(&file, &params.key, &params.locale, params.count);

    Ok(serde_json::to_value(context_keys)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::test_helpers::MemoryStore;

    const PLURALS_FIXTURE: &str = include_str!("../../tests/fixtures/with_plurals.xcstrings");
    const SIMPLE_FIXTURE: &str = include_str!("../../tests/fixtures/simple.xcstrings");

    #[tokio::test]
    async fn test_get_plurals_success() {
        let store = MemoryStore::new();
        store.add_file("/test/plurals.xcstrings", PLURALS_FIXTURE);
        let cache = Mutex::new(None);

        let params = GetPluralsParams {
            file_path: Some("/test/plurals.xcstrings".to_string()),
            locale: "de".to_string(),
            batch_size: 20,
            offset: 0,
        };
        let result = handle_get_plurals(&store, &cache, params).await.unwrap();
        assert!(result["total"].as_u64().unwrap() > 0);
    }

    #[tokio::test]
    async fn test_get_plurals_empty() {
        let store = MemoryStore::new();
        store.add_file("/test/simple.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(None);

        let params = GetPluralsParams {
            file_path: Some("/test/simple.xcstrings".to_string()),
            locale: "de".to_string(),
            batch_size: 20,
            offset: 0,
        };
        let result = handle_get_plurals(&store, &cache, params).await.unwrap();
        assert_eq!(result["total"], 0);
    }

    #[tokio::test]
    async fn test_get_context_success() {
        let store = MemoryStore::new();
        store.add_file("/test/simple.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(None);

        let params = GetContextParams {
            file_path: Some("/test/simple.xcstrings".to_string()),
            key: "greeting".to_string(),
            locale: "uk".to_string(),
            count: 5,
        };
        let result = handle_get_context(&store, &cache, params).await.unwrap();
        let arr = result.as_array().unwrap();
        assert!(!arr.is_empty());
    }

    #[tokio::test]
    async fn test_get_context_missing_key() {
        let store = MemoryStore::new();
        store.add_file("/test/simple.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(None);

        let params = GetContextParams {
            file_path: Some("/test/simple.xcstrings".to_string()),
            key: "nonexistent".to_string(),
            locale: "uk".to_string(),
            count: 5,
        };
        let result = handle_get_context(&store, &cache, params).await.unwrap();
        let arr = result.as_array().unwrap();
        assert!(arr.is_empty());
    }
}
