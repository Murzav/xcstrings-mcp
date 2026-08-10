use std::path::Path;

use tempfile::TempDir;
use tokio::sync::Mutex;

use super::FileCache;
use super::merge::{MergeXcStringsParams, handle_merge_xcstrings};
use super::parse::{ParseParams, handle_parse};
use crate::io::{FileStore, fs::FsFileStore};
use crate::service::semantic_merge::fingerprint;

fn catalog(comment: &str) -> String {
    format!(
        r#"{{"sourceLanguage":"en","strings":{{"key":{{"comment":"{comment}"}}}},"version":"1.0"}}"#
    )
}

fn catalog_with_keys(keys: &[&str]) -> String {
    let strings = keys
        .iter()
        .map(|key| ((*key).to_string(), serde_json::json!({})))
        .collect::<serde_json::Map<_, _>>();
    serde_json::to_string(&serde_json::json!({
        "sourceLanguage": "en",
        "strings": strings,
        "version": "1.0"
    }))
    .unwrap()
}

fn write(path: &Path, content: &str) {
    std::fs::write(path, content).unwrap();
}

fn params(dir: &TempDir, dry_run: bool) -> MergeXcStringsParams {
    MergeXcStringsParams {
        base_path: dir.path().join("base.xcstrings").display().to_string(),
        current_path: dir.path().join("current.xcstrings").display().to_string(),
        incoming_path: dir.path().join("incoming.xcstrings").display().to_string(),
        output_path: dir.path().join("output.xcstrings").display().to_string(),
        dry_run,
        resolutions: Vec::new(),
        expected_fingerprints: None,
        conflict_offset: 0,
        conflict_limit: 50,
    }
}

#[tokio::test]
async fn merge_reads_explicit_inputs_fresh_instead_of_using_cached_content() {
    let dir = TempDir::new().unwrap();
    let base = dir.path().join("base.xcstrings");
    let current = dir.path().join("current.xcstrings");
    let incoming = dir.path().join("incoming.xcstrings");
    write(&base, &catalog("base"));
    write(&current, &catalog("cached"));
    write(&incoming, &catalog("base"));
    let store = FsFileStore::new();
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: current.display().to_string(),
        },
    )
    .await
    .unwrap();

    let fresh = catalog("fresh");
    write(&current, &fresh);
    let report = handle_merge_xcstrings(&store, &cache, &write_lock, params(&dir, true))
        .await
        .unwrap();
    assert_eq!(
        report.fingerprints.current.sha256,
        fingerprint(fresh.as_bytes())
    );
}

#[tokio::test]
async fn apply_refreshes_only_an_already_cached_output_without_switching_active_file() {
    let dir = TempDir::new().unwrap();
    let base = dir.path().join("base.xcstrings");
    let current = dir.path().join("current.xcstrings");
    let incoming = dir.path().join("incoming.xcstrings");
    let output = dir.path().join("output.xcstrings");
    let active = dir.path().join("active.xcstrings");
    write(&base, &catalog("base"));
    write(&current, &catalog("current"));
    write(&incoming, &catalog("base"));
    write(&output, &catalog("old output"));
    write(&active, &catalog("active"));
    let store = FsFileStore::new();
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    for path in [&output, &active] {
        handle_parse(
            &store,
            &cache,
            ParseParams {
                file_path: path.display().to_string(),
            },
        )
        .await
        .unwrap();
    }
    assert_eq!(cache.lock().await.active_path(), Some(&active));

    let dry = handle_merge_xcstrings(&store, &cache, &write_lock, params(&dir, true))
        .await
        .unwrap();
    let mut apply = params(&dir, false);
    apply.expected_fingerprints = dry.expected_fingerprints;
    handle_merge_xcstrings(&store, &cache, &write_lock, apply)
        .await
        .unwrap();

    let guard = cache.lock().await;
    assert_eq!(guard.active_path(), Some(&active));
    let output_identity = store.file_identity(&output).unwrap();
    assert_eq!(
        guard.get(&output_identity).unwrap().content.strings["key"]
            .comment
            .as_deref(),
        Some("current")
    );
}

#[tokio::test]
async fn apply_does_not_insert_an_uncached_output_or_switch_active_file() {
    let dir = TempDir::new().unwrap();
    let base = dir.path().join("base.xcstrings");
    let current = dir.path().join("current.xcstrings");
    let incoming = dir.path().join("incoming.xcstrings");
    let output = dir.path().join("output.xcstrings");
    let active = dir.path().join("active.xcstrings");
    write(&base, &catalog("base"));
    write(&current, &catalog("current"));
    write(&incoming, &catalog("base"));
    write(&active, &catalog("active"));
    let store = FsFileStore::new();
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: active.display().to_string(),
        },
    )
    .await
    .unwrap();

    let dry = handle_merge_xcstrings(&store, &cache, &write_lock, params(&dir, true))
        .await
        .unwrap();
    let mut apply = params(&dir, false);
    apply.expected_fingerprints = dry.expected_fingerprints;
    handle_merge_xcstrings(&store, &cache, &write_lock, apply)
        .await
        .unwrap();

    let guard = cache.lock().await;
    assert_eq!(guard.active_path(), Some(&active));
    let output_identity = store.file_identity(&output).unwrap();
    assert!(guard.get(&output_identity).is_none());
}

#[cfg(unix)]
#[tokio::test]
async fn apply_refreshes_active_cached_symlink_by_file_identity_and_keeps_display_path() {
    let dir = TempDir::new().unwrap();
    let base = dir.path().join("base.xcstrings");
    let current = dir.path().join("current.xcstrings");
    let incoming = dir.path().join("incoming.xcstrings");
    let output = dir.path().join("output.xcstrings");
    let alias = dir.path().join("output-alias.xcstrings");
    write(&base, &catalog_with_keys(&["base"]));
    write(&current, &catalog_with_keys(&["base", "current"]));
    write(&incoming, &catalog_with_keys(&["base", "incoming"]));
    write(&output, &catalog_with_keys(&["old"]));
    std::os::unix::fs::symlink(&output, &alias).unwrap();
    let store = FsFileStore::new();
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: alias.display().to_string(),
        },
    )
    .await
    .unwrap();

    let dry = handle_merge_xcstrings(&store, &cache, &write_lock, params(&dir, true))
        .await
        .unwrap();
    let mut apply = params(&dir, false);
    apply.expected_fingerprints = dry.expected_fingerprints;
    handle_merge_xcstrings(&store, &cache, &write_lock, apply)
        .await
        .unwrap();

    let entries = cache.lock().await.list();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, alias);
    assert_eq!(entries[0].total_keys, 3);
    assert!(entries[0].is_active);
}

#[tokio::test]
async fn apply_refreshes_inactive_cached_relative_path_without_switching_active_file() {
    let cwd = std::env::current_dir().unwrap();
    let dir = tempfile::Builder::new()
        .prefix("task2-cache-relative-")
        .tempdir_in(&cwd)
        .unwrap();
    let relative_dir = dir.path().strip_prefix(&cwd).unwrap();
    let base = dir.path().join("base.xcstrings");
    let current = dir.path().join("current.xcstrings");
    let incoming = dir.path().join("incoming.xcstrings");
    let output = dir.path().join("output.xcstrings");
    let relative_output = relative_dir.join("output.xcstrings");
    let active = dir.path().join("active.xcstrings");
    write(&base, &catalog_with_keys(&["base"]));
    write(&current, &catalog_with_keys(&["base", "current"]));
    write(&incoming, &catalog_with_keys(&["base", "incoming"]));
    write(&output, &catalog_with_keys(&["old"]));
    write(&active, &catalog_with_keys(&["active"]));
    let store = FsFileStore::new();
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    for path in [&relative_output, &active] {
        handle_parse(
            &store,
            &cache,
            ParseParams {
                file_path: path.display().to_string(),
            },
        )
        .await
        .unwrap();
    }

    let dry = handle_merge_xcstrings(&store, &cache, &write_lock, params(&dir, true))
        .await
        .unwrap();
    let mut apply = params(&dir, false);
    apply.expected_fingerprints = dry.expected_fingerprints;
    handle_merge_xcstrings(&store, &cache, &write_lock, apply)
        .await
        .unwrap();

    let entries = cache.lock().await.list();
    let cached_output = entries
        .iter()
        .find(|entry| entry.path == relative_output)
        .unwrap();
    assert_eq!(cached_output.total_keys, 3);
    assert!(!cached_output.is_active);
    let cached_active = entries.iter().find(|entry| entry.path == active).unwrap();
    assert_eq!(cached_active.total_keys, 1);
    assert!(cached_active.is_active);
}

#[cfg(unix)]
#[tokio::test]
async fn reparsing_retargeted_symlink_evicts_old_identity_and_keeps_one_active_display_path() {
    use std::os::unix::fs::symlink;

    let dir = TempDir::new().unwrap();
    let first = dir.path().join("first.xcstrings");
    let second = dir.path().join("second.xcstrings");
    let alias = dir.path().join("catalog-alias.xcstrings");
    write(&first, &catalog_with_keys(&["first.a", "first.b"]));
    write(
        &second,
        &catalog_with_keys(&["second.a", "second.b", "second.c"]),
    );
    symlink(&first, &alias).unwrap();
    let store = FsFileStore::new();
    let cache = Mutex::new(FileCache::new());

    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: alias.display().to_string(),
        },
    )
    .await
    .unwrap();
    let old_identity = store.file_identity(&alias).unwrap();
    std::fs::remove_file(&alias).unwrap();
    symlink(&second, &alias).unwrap();
    let new_identity = store.file_identity(&alias).unwrap();
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: alias.display().to_string(),
        },
    )
    .await
    .unwrap();

    let guard = cache.lock().await;
    let entries = guard.list();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, alias);
    assert_eq!(entries[0].total_keys, 3);
    assert!(entries[0].is_active);
    assert!(guard.get(&old_identity).is_none());
    assert_eq!(guard.get(&new_identity).unwrap().content.strings.len(), 3);
    assert_eq!(guard.active_path(), Some(&alias));
}
