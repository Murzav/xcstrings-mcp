use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::service::{formatter, locale, parser};
use crate::tools::parse::CachedFile;
use crate::tools::resolve_file;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ListLocalesParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
}

pub(crate) async fn handle_list_locales(
    store: &dyn FileStore,
    cache: &Mutex<Option<CachedFile>>,
    params: ListLocalesParams,
) -> Result<serde_json::Value, XcStringsError> {
    let (_path, file) = resolve_file(store, cache, params.file_path.as_deref()).await?;
    let locales = locale::list_locales(&file);
    Ok(serde_json::to_value(locales)?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct AddLocaleParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Locale code to add (e.g., "ko", "ja")
    pub locale: String,
}

#[derive(Debug, Serialize)]
struct AddLocaleResult {
    added: usize,
    locale: String,
}

pub(crate) async fn handle_add_locale(
    store: &dyn FileStore,
    cache: &Mutex<Option<CachedFile>>,
    write_lock: &Mutex<()>,
    params: AddLocaleParams,
) -> Result<serde_json::Value, XcStringsError> {
    let (path, _file) = resolve_file(store, cache, params.file_path.as_deref()).await?;

    // Acquire write lock (same pattern as submit_translations)
    let _write_guard = write_lock.lock().await;

    // Re-read fresh from disk
    let raw = store.read(&path)?;
    let mut fresh_file = parser::parse(&raw)?;

    let added = locale::add_locale(&mut fresh_file, &params.locale)?;

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

    let result = AddLocaleResult {
        added,
        locale: params.locale,
    };
    Ok(serde_json::to_value(result)?)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::tools::parse::{ParseParams, handle_parse};
    use crate::tools::test_helpers::{MemoryStore, SIMPLE_FIXTURE};

    #[tokio::test]
    async fn test_list_locales_returns_locales() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(None);

        let params = ListLocalesParams {
            file_path: Some("/test/file.xcstrings".to_string()),
        };
        let result = handle_list_locales(&store, &cache, params).await.unwrap();

        let locales = result.as_array().unwrap();
        assert!(!locales.is_empty());
        assert!(locales.iter().any(|l| l["locale"] == "en"));
        assert!(locales.iter().any(|l| l["locale"] == "uk"));
    }

    #[tokio::test]
    async fn test_add_locale_success() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(None);
        let write_lock = Mutex::new(());

        let parse_params = ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, parse_params).await.unwrap();

        let params = AddLocaleParams {
            file_path: None,
            locale: "fr".to_string(),
        };
        let result = handle_add_locale(&store, &cache, &write_lock, params)
            .await
            .unwrap();

        assert_eq!(result["locale"], "fr");
        assert!(result["added"].as_u64().unwrap() > 0);

        // Verify file was written with the new locale
        let content = store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap();
        assert!(content.contains("\"fr\""));
    }

    #[tokio::test]
    async fn test_add_locale_duplicate_error() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(None);
        let write_lock = Mutex::new(());

        let parse_params = ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, parse_params).await.unwrap();

        let params = AddLocaleParams {
            file_path: None,
            locale: "uk".to_string(),
        };
        let result = handle_add_locale(&store, &cache, &write_lock, params).await;
        assert!(result.is_err());
    }
}
