use rmcp::RoleServer;
use rmcp::model::LoggingLevel;
use rmcp::service::Peer;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::service::{formatter, locale, parser};
use crate::tools::parse::CachedFile;
use crate::tools::resolve_file;
use crate::tools::{FileCache, mcp_log};

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ListLocalesParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
}

pub(crate) async fn handle_list_locales(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
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
    cache: &Mutex<FileCache>,
    write_lock: &Mutex<()>,
    params: AddLocaleParams,
    peer: Option<&Peer<RoleServer>>,
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
    guard.insert(
        path.clone(),
        CachedFile {
            path,
            content: fresh_file,
            modified: mtime,
        },
    );

    mcp_log(
        peer,
        LoggingLevel::Info,
        &format!(
            "Added locale '{}': {} keys initialized",
            params.locale, added
        ),
    )
    .await;

    let result = AddLocaleResult {
        added,
        locale: params.locale,
    };
    Ok(serde_json::to_value(result)?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct RemoveLocaleParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Locale code to remove (e.g., "ko", "ja")
    pub locale: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct RemoveLocaleResult {
    removed: usize,
    locale: String,
}

pub(crate) async fn handle_remove_locale(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    write_lock: &Mutex<()>,
    params: RemoveLocaleParams,
    peer: Option<&Peer<RoleServer>>,
) -> Result<serde_json::Value, XcStringsError> {
    let (path, file) = resolve_file(store, cache, params.file_path.as_deref()).await?;
    let source_language = file.source_language.clone();

    // Acquire write lock
    let _write_guard = write_lock.lock().await;

    // Re-read fresh from disk
    let raw = store.read(&path)?;
    let mut fresh_file = parser::parse(&raw)?;

    let removed = locale::remove_locale(&mut fresh_file, &params.locale, &source_language)?;

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

    mcp_log(
        peer,
        LoggingLevel::Info,
        &format!(
            "Removed locale '{}': {} entries affected",
            params.locale, removed
        ),
    )
    .await;

    let result = RemoveLocaleResult {
        removed,
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
        let cache = Mutex::new(FileCache::new());

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
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let parse_params = ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, parse_params, None)
            .await
            .unwrap();

        let params = AddLocaleParams {
            file_path: None,
            locale: "fr".to_string(),
        };
        let result = handle_add_locale(&store, &cache, &write_lock, params, None)
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
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let parse_params = ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, parse_params, None)
            .await
            .unwrap();

        let params = AddLocaleParams {
            file_path: None,
            locale: "uk".to_string(),
        };
        let result = handle_add_locale(&store, &cache, &write_lock, params, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_remove_locale_success() {
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

        let params = RemoveLocaleParams {
            file_path: None,
            locale: "uk".to_string(),
        };
        let result = handle_remove_locale(&store, &cache, &write_lock, params, None)
            .await
            .unwrap();

        assert_eq!(result["locale"], "uk");
        assert!(result["removed"].as_u64().unwrap() > 0);

        // Verify the locale was removed from written file
        let content = store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        for (_key, entry) in parsed["strings"].as_object().unwrap() {
            if let Some(locs) = entry.get("localizations") {
                assert!(
                    locs.get("uk").is_none(),
                    "uk locale should have been removed"
                );
            }
        }
    }

    #[tokio::test]
    async fn test_remove_source_locale_error() {
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

        let params = RemoveLocaleParams {
            file_path: None,
            locale: "en".to_string(),
        };
        let result = handle_remove_locale(&store, &cache, &write_lock, params, None).await;
        assert!(result.is_err());
    }
}
