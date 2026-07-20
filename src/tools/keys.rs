use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::service::{formatter, keys, parser};
use crate::tools::parse::CachedFile;
use crate::tools::resolve_file;
use crate::tools::{FileCache, mcp_log};

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct DeleteKeysParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Keys to permanently remove along with all their translations across every locale.
    pub keys: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DeleteKeysResult {
    deleted: Vec<String>,
    not_found: Vec<String>,
}

pub(crate) async fn handle_delete_keys(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    write_lock: &Mutex<()>,
    params: DeleteKeysParams,
) -> Result<serde_json::Value, XcStringsError> {
    let (path, _file) = resolve_file(store, cache, params.file_path.as_deref()).await?;

    let _write_guard = write_lock.lock().await;

    let raw = store.read(&path)?;
    let mut fresh_file = parser::parse(&raw)?;

    let svc_result = keys::delete_keys(&mut fresh_file, &params.keys);

    if !svc_result.deleted.is_empty() {
        let formatted = formatter::format_xcstrings(&fresh_file)?;
        store.write(&path, &formatted)?;

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
    }

    mcp_log(&format!(
        "Deleted {} keys ({} not found)",
        svc_result.deleted.len(),
        svc_result.not_found.len()
    ));

    let result = DeleteKeysResult {
        deleted: svc_result.deleted,
        not_found: svc_result.not_found,
    };
    Ok(serde_json::to_value(result)?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct RenameKeyParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Current key name
    pub old_key: String,
    /// New key name
    pub new_key: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct RenameKeyResult {
    old_key: String,
    new_key: String,
}

pub(crate) async fn handle_rename_key(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    write_lock: &Mutex<()>,
    params: RenameKeyParams,
) -> Result<serde_json::Value, XcStringsError> {
    let (path, _file) = resolve_file(store, cache, params.file_path.as_deref()).await?;

    let _write_guard = write_lock.lock().await;

    let raw = store.read(&path)?;
    let mut fresh_file = parser::parse(&raw)?;

    let svc_result = keys::rename_key(&mut fresh_file, &params.old_key, &params.new_key)?;

    let formatted = formatter::format_xcstrings(&fresh_file)?;
    store.write(&path, &formatted)?;

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

    mcp_log(&format!(
        "Renamed key '{}' to '{}'",
        svc_result.old_key, svc_result.new_key
    ));

    let result = RenameKeyResult {
        old_key: svc_result.old_key,
        new_key: svc_result.new_key,
    };
    Ok(serde_json::to_value(result)?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct DeleteTranslationsParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Keys to delete translations for
    pub keys: Vec<String>,
    /// Locale to reset translations for. Cannot be the source language.
    pub locale: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct DeleteTranslationsResult {
    reset: Vec<String>,
    not_found: Vec<String>,
    locale: String,
}

pub(crate) async fn handle_delete_translations(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    write_lock: &Mutex<()>,
    params: DeleteTranslationsParams,
) -> Result<serde_json::Value, XcStringsError> {
    let (path, _file) = resolve_file(store, cache, params.file_path.as_deref()).await?;

    let _write_guard = write_lock.lock().await;

    let raw = store.read(&path)?;
    let mut fresh_file = parser::parse(&raw)?;
    let source_language = fresh_file.source_language.clone();

    let svc_result = keys::delete_translations(
        &mut fresh_file,
        &params.keys,
        &params.locale,
        &source_language,
    )?;

    if !svc_result.reset.is_empty() {
        let formatted = formatter::format_xcstrings(&fresh_file)?;
        store.write(&path, &formatted)?;

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
    }

    mcp_log(&format!(
        "Deleted translations for locale '{}': {} reset, {} not found",
        params.locale,
        svc_result.reset.len(),
        svc_result.not_found.len()
    ));

    let result = DeleteTranslationsResult {
        reset: svc_result.reset,
        not_found: svc_result.not_found,
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
    async fn test_delete_keys_success() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let parse_params = ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, parse_params).await.unwrap();

        let params = DeleteKeysParams {
            file_path: None,
            keys: vec!["greeting".to_string()],
        };
        let result = handle_delete_keys(&store, &cache, &write_lock, params)
            .await
            .unwrap();

        assert_eq!(result["deleted"].as_array().unwrap().len(), 1);
        assert!(result["not_found"].as_array().unwrap().is_empty());

        let content = store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap();
        assert!(!content.contains("\"greeting\""));
    }

    #[tokio::test]
    async fn test_rename_key_success() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let parse_params = ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, parse_params).await.unwrap();

        let params = RenameKeyParams {
            file_path: None,
            old_key: "greeting".to_string(),
            new_key: "hello".to_string(),
        };
        let result = handle_rename_key(&store, &cache, &write_lock, params)
            .await
            .unwrap();

        assert_eq!(result["old_key"], "greeting");
        assert_eq!(result["new_key"], "hello");

        let content = store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap();
        assert!(!content.contains("\"greeting\""));
        assert!(content.contains("\"hello\""));
    }

    #[tokio::test]
    async fn test_rename_key_not_found_error() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let parse_params = ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, parse_params).await.unwrap();

        let params = RenameKeyParams {
            file_path: None,
            old_key: "nonexistent".to_string(),
            new_key: "new_name".to_string(),
        };
        let result = handle_rename_key(&store, &cache, &write_lock, params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_translations_success() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let parse_params = ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, parse_params).await.unwrap();

        let params = DeleteTranslationsParams {
            file_path: None,
            keys: vec!["greeting".to_string()],
            locale: "uk".to_string(),
        };
        let result = handle_delete_translations(&store, &cache, &write_lock, params)
            .await
            .unwrap();

        assert_eq!(result["locale"], "uk");
        assert!(!result["reset"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_delete_translations_source_locale_error() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let parse_params = ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, parse_params).await.unwrap();

        let params = DeleteTranslationsParams {
            file_path: None,
            keys: vec!["greeting".to_string()],
            locale: "en".to_string(),
        };
        let result = handle_delete_translations(&store, &cache, &write_lock, params).await;
        assert!(result.is_err());
    }
}
