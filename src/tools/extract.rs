use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::model::translation::TranslationUnit;
use crate::model::xcstrings::TranslationState;
use crate::service::extractor;
use crate::tools::FileCache;
use crate::tools::resolve_file;

fn default_batch_size() -> usize {
    30
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetUntranslatedParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Target locale code (e.g., "uk", "de"). Used if `locales` is not set.
    pub locale: String,
    /// Multiple target locales. A key is untranslated if missing in ANY of these locales. Overrides `locale` if set.
    #[serde(default)]
    pub locales: Option<Vec<String>>,
    /// Number of strings per batch (1-100, default 30)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Offset for pagination (default 0). Set to previous offset + batch_size to get the next page.
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

/// Extract untranslated strings for one or more locales with batching.
pub(crate) async fn handle_get_untranslated(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    params: GetUntranslatedParams,
) -> Result<serde_json::Value, XcStringsError> {
    let (_path, file) = resolve_file(store, cache, params.file_path.as_deref()).await?;

    // Determine which locales to check
    let locales: Vec<String> = params
        .locales
        .unwrap_or_else(|| vec![params.locale.clone()]);
    let locale_refs: Vec<&str> = locales.iter().map(|s| s.as_str()).collect();

    let (units, total) =
        extractor::get_untranslated_multi(&file, &locale_refs, params.batch_size, params.offset)?;

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
    /// Target locale code (e.g., "uk", "de"). Returns keys with extractionState=stale.
    pub locale: String,
    /// Number of strings per batch (1-100, default 30)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Offset for pagination (default 0). Set to previous offset + batch_size to get the next page.
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
    cache: &Mutex<FileCache>,
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

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SearchKeysParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Substring to search for (case-insensitive, matches key name and source text). Empty string returns all translatable keys.
    pub pattern: String,
    /// Target locale for translation context
    pub locale: String,
    /// Maximum number of results per batch (1-100, default 30)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Offset for pagination (default 0). Set to previous offset + batch_size to get the next page.
    #[serde(default)]
    pub offset: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct SearchKeysResult {
    pub units: Vec<TranslationUnit>,
    pub total: usize,
    pub offset: usize,
    pub batch_size: usize,
    pub has_more: bool,
}

/// Search keys by substring pattern with batching.
pub(crate) async fn handle_search_keys(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    params: SearchKeysParams,
) -> Result<serde_json::Value, XcStringsError> {
    let (_path, file) = resolve_file(store, cache, params.file_path.as_deref()).await?;

    let (units, total) = extractor::search_keys(
        &file,
        &params.pattern,
        &params.locale,
        params.batch_size,
        params.offset,
    )?;

    let has_more = params.offset + units.len() < total;

    let result = SearchKeysResult {
        units,
        total,
        offset: params.offset,
        batch_size: params.batch_size,
        has_more,
    };

    Ok(serde_json::to_value(result)?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetKeyParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// The localization key to look up
    pub key: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct GetKeyResult {
    key: String,
    source_language: String,
    source_text: String,
    comment: Option<String>,
    should_translate: bool,
    translations: Vec<KeyTranslation>,
}

#[derive(Debug, Serialize)]
pub(crate) struct KeyTranslation {
    locale: String,
    value: Option<String>,
    state: Option<TranslationState>,
    has_plurals: bool,
    has_device_variants: bool,
}

/// Get all translations for a specific key across all locales.
pub(crate) async fn handle_get_key(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    params: GetKeyParams,
) -> Result<serde_json::Value, XcStringsError> {
    let (_path, file) = resolve_file(store, cache, params.file_path.as_deref()).await?;

    let entry = file
        .strings
        .get(&params.key)
        .ok_or_else(|| XcStringsError::KeyNotFound(params.key.clone()))?;

    let source_text = entry
        .localizations
        .as_ref()
        .and_then(|locs| locs.get(&file.source_language))
        .and_then(|loc| loc.string_unit.as_ref())
        .map(|su| su.value.clone())
        .unwrap_or_else(|| params.key.clone());

    let mut translations = Vec::new();
    if let Some(locs) = &entry.localizations {
        for (locale, loc) in locs {
            let value = loc.string_unit.as_ref().map(|su| su.value.clone());
            let state = loc.string_unit.as_ref().map(|su| su.state.clone());
            let has_plurals = loc.variations.as_ref().is_some_and(|v| v.plural.is_some());
            let has_device_variants = loc.variations.as_ref().is_some_and(|v| v.device.is_some());

            translations.push(KeyTranslation {
                locale: locale.clone(),
                value,
                state,
                has_plurals,
                has_device_variants,
            });
        }
    }

    let result = GetKeyResult {
        key: params.key,
        source_language: file.source_language.clone(),
        source_text,
        comment: entry.comment.clone(),
        should_translate: entry.should_translate,
        translations,
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
        let cache = Mutex::new(FileCache::new());

        let params = GetUntranslatedParams {
            file_path: Some("/test/file.xcstrings".to_string()),
            locale: "de".to_string(),
            locales: None,
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
        let cache = Mutex::new(FileCache::new());

        let parse_params = ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, parse_params).await.unwrap();

        let params = GetUntranslatedParams {
            file_path: None,
            locale: "uk".to_string(),
            locales: None,
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
        let cache = Mutex::new(FileCache::new());

        let params = GetUntranslatedParams {
            file_path: None,
            locale: "de".to_string(),
            locales: None,
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
        let cache = Mutex::new(FileCache::new());

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
        let cache = Mutex::new(FileCache::new());

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

    #[tokio::test]
    async fn test_search_keys_handler() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());

        let params = SearchKeysParams {
            file_path: Some("/test/file.xcstrings".to_string()),
            pattern: "greeting".to_string(),
            locale: "de".to_string(),
            batch_size: 30,
            offset: 0,
        };
        let result = handle_search_keys(&store, &cache, params).await.unwrap();

        // simple.xcstrings has a "greeting" key
        assert!(result["total"].as_u64().unwrap() >= 1);
        assert_eq!(result["has_more"], false);

        let units = result["units"].as_array().unwrap();
        assert!(units.iter().any(|u| u["key"] == "greeting"));
    }

    #[tokio::test]
    async fn test_get_key_success() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());

        let params = GetKeyParams {
            file_path: Some("/test/file.xcstrings".to_string()),
            key: "greeting".to_string(),
        };
        let result = handle_get_key(&store, &cache, params).await.unwrap();

        assert_eq!(result["key"], "greeting");
        assert_eq!(result["source_language"], "en");
        assert!(result["should_translate"].as_bool().unwrap());
        assert!(!result["translations"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_get_key_not_found() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());

        let params = GetKeyParams {
            file_path: Some("/test/file.xcstrings".to_string()),
            key: "nonexistent".to_string(),
        };
        let result = handle_get_key(&store, &cache, params).await;
        assert!(result.is_err());
    }
}
