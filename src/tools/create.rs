use std::path::PathBuf;

use rmcp::RoleServer;
use rmcp::model::LoggingLevel;
use rmcp::service::Peer;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::service::{creator, formatter, parser};
use crate::tools::parse::CachedFile;
use crate::tools::resolve_file;
use crate::tools::{FileCache, mcp_log};

// ── create_xcstrings ──

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CreateXcStringsParams {
    /// Full path for the new .xcstrings file
    pub file_path: String,
    /// Source language code (e.g., "en")
    pub source_language: String,
}

#[derive(Debug, Serialize)]
struct CreateXcStringsResult {
    path: String,
    source_language: String,
}

pub(crate) async fn handle_create_xcstrings(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    params: CreateXcStringsParams,
    peer: Option<&Peer<RoleServer>>,
) -> Result<serde_json::Value, XcStringsError> {
    let path = PathBuf::from(&params.file_path);

    match path.extension().and_then(|e| e.to_str()) {
        Some("xcstrings") => {}
        _ => return Err(XcStringsError::NotXcStrings { path }),
    }

    if store.exists(&path) {
        return Err(XcStringsError::FileAlreadyExists { path });
    }

    let file = creator::create_empty_file(&params.source_language)?;

    store.create_parent_dirs(&path)?;
    let formatted = formatter::format_xcstrings(&file)?;
    store.write(&path, &formatted)?;

    // Update cache
    let mtime = store.modified_time(&path)?;
    let mut guard = cache.lock().await;
    guard.insert(
        path.clone(),
        CachedFile {
            path: path.clone(),
            content: file,
            modified: mtime,
        },
    );

    mcp_log(
        peer,
        LoggingLevel::Info,
        &format!("Created {}", params.file_path),
    )
    .await;

    let result = CreateXcStringsResult {
        path: params.file_path,
        source_language: params.source_language,
    };
    Ok(serde_json::to_value(result)?)
}

// ── add_keys ──

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct AddKeysParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Keys to add
    pub keys: Vec<AddKeyEntry>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct AddKeyEntry {
    /// Localization key name
    pub key: String,
    /// Source language text
    pub source_text: String,
    /// Optional developer comment
    #[serde(default)]
    pub comment: Option<String>,
}

#[derive(Debug, Serialize)]
struct AddKeysResult {
    added: usize,
    skipped: Vec<String>,
}

pub(crate) async fn handle_add_keys(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    write_lock: &Mutex<()>,
    params: AddKeysParams,
    peer: Option<&Peer<RoleServer>>,
) -> Result<serde_json::Value, XcStringsError> {
    let (path, _file) = resolve_file(store, cache, params.file_path.as_deref()).await?;

    let _write_guard = write_lock.lock().await;

    // Re-read fresh from disk
    let raw = store.read(&path)?;
    let mut fresh_file = parser::parse(&raw)?;

    let requests: Vec<creator::AddKeyRequest> = params
        .keys
        .iter()
        .map(|e| creator::AddKeyRequest {
            key: e.key.clone(),
            source_text: e.source_text.clone(),
            comment: e.comment.clone(),
        })
        .collect();

    let add_result = creator::add_keys(&mut fresh_file, &requests);

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
            "Added {} keys, skipped {}",
            add_result.added,
            add_result.skipped.len()
        ),
    )
    .await;

    let result = AddKeysResult {
        added: add_result.added,
        skipped: add_result.skipped,
    };
    Ok(serde_json::to_value(result)?)
}

// ── update_comments ──

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateCommentsParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Comment updates
    pub comments: Vec<CommentEntry>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CommentEntry {
    /// Localization key name
    pub key: String,
    /// New comment text
    pub comment: String,
}

#[derive(Debug, Serialize)]
struct UpdateCommentsResult {
    updated: usize,
}

pub(crate) async fn handle_update_comments(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    write_lock: &Mutex<()>,
    params: UpdateCommentsParams,
    peer: Option<&Peer<RoleServer>>,
) -> Result<serde_json::Value, XcStringsError> {
    let (path, _file) = resolve_file(store, cache, params.file_path.as_deref()).await?;

    let _write_guard = write_lock.lock().await;

    let raw = store.read(&path)?;
    let mut fresh_file = parser::parse(&raw)?;

    let updates: Vec<(String, String)> = params
        .comments
        .iter()
        .map(|e| (e.key.clone(), e.comment.clone()))
        .collect();

    let updated = creator::update_comments(&mut fresh_file, &updates);

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

    mcp_log(
        peer,
        LoggingLevel::Info,
        &format!("Updated {updated} comments"),
    )
    .await;

    let result = UpdateCommentsResult { updated };
    Ok(serde_json::to_value(result)?)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::tools::parse::{ParseParams, handle_parse};
    use crate::tools::test_helpers::{MemoryStore, SIMPLE_FIXTURE};

    #[tokio::test]
    async fn test_create_xcstrings_success() {
        let store = MemoryStore::new();
        let cache = Mutex::new(FileCache::new());

        let params = CreateXcStringsParams {
            file_path: "/test/New.xcstrings".to_string(),
            source_language: "en".to_string(),
        };
        let result = handle_create_xcstrings(&store, &cache, params, None)
            .await
            .unwrap();

        assert_eq!(result["path"], "/test/New.xcstrings");
        assert_eq!(result["source_language"], "en");

        let content = store.get_content(Path::new("/test/New.xcstrings")).unwrap();
        assert!(content.contains("\"sourceLanguage\" : \"en\""));

        let guard = cache.lock().await;
        assert!(guard.active_path().is_some());
    }

    #[tokio::test]
    async fn test_create_xcstrings_already_exists() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());

        let params = CreateXcStringsParams {
            file_path: "/test/file.xcstrings".to_string(),
            source_language: "en".to_string(),
        };
        let result = handle_create_xcstrings(&store, &cache, params, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_xcstrings_not_xcstrings_ext() {
        let store = MemoryStore::new();
        let cache = Mutex::new(FileCache::new());

        let params = CreateXcStringsParams {
            file_path: "/test/file.json".to_string(),
            source_language: "en".to_string(),
        };
        let result = handle_create_xcstrings(&store, &cache, params, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_add_keys_success() {
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

        let params = AddKeysParams {
            file_path: None,
            keys: vec![AddKeyEntry {
                key: "new_key".to_string(),
                source_text: "New Value".to_string(),
                comment: Some("A comment".to_string()),
            }],
        };
        let result = handle_add_keys(&store, &cache, &write_lock, params, None)
            .await
            .unwrap();

        assert_eq!(result["added"], 1);
        assert!(result["skipped"].as_array().unwrap().is_empty());

        let content = store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap();
        assert!(content.contains("new_key"));
    }

    #[tokio::test]
    async fn test_add_keys_duplicate() {
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

        let params = AddKeysParams {
            file_path: None,
            keys: vec![AddKeyEntry {
                key: "greeting".to_string(),
                source_text: "Duplicate".to_string(),
                comment: None,
            }],
        };
        let result = handle_add_keys(&store, &cache, &write_lock, params, None)
            .await
            .unwrap();

        assert_eq!(result["added"], 0);
        assert_eq!(result["skipped"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_update_comments_success() {
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

        let params = UpdateCommentsParams {
            file_path: None,
            comments: vec![CommentEntry {
                key: "greeting".to_string(),
                comment: "Updated comment".to_string(),
            }],
        };
        let result = handle_update_comments(&store, &cache, &write_lock, params, None)
            .await
            .unwrap();

        assert_eq!(result["updated"], 1);

        let content = store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap();
        assert!(content.contains("Updated comment"));
    }
}
