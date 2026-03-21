use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
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

// ── discover_files ──

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct DiscoverFilesParams {
    /// Directory path to search recursively for localization files
    pub directory: String,
}

#[derive(Debug, Serialize)]
struct DiscoveredFile {
    path: String,
    file_type: &'static str,
}

#[derive(Debug, Serialize)]
struct DiscoverFilesResult {
    files: Vec<DiscoveredFile>,
    count: usize,
    /// Legacy .strings/.stringsdict files found inside .lproj directories
    legacy_files: Vec<DiscoveredFile>,
    legacy_count: usize,
}

/// Recursively walk a directory to find localization files.
fn walk_localization_files(
    dir: &Path,
    xcstrings: &mut Vec<PathBuf>,
    legacy: &mut Vec<(PathBuf, &'static str)>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if name.ends_with(".lproj") {
                // Scan inside .lproj for .strings/.stringsdict
                if let Ok(lproj_entries) = std::fs::read_dir(&path) {
                    for lproj_entry in lproj_entries.flatten() {
                        let lp = lproj_entry.path();
                        if !lp.is_file() {
                            continue;
                        }
                        match lp.extension().and_then(|e| e.to_str()) {
                            Some("strings") => legacy.push((lp, "strings")),
                            Some("stringsdict") => legacy.push((lp, "stringsdict")),
                            _ => {}
                        }
                    }
                }
            } else {
                walk_localization_files(&path, xcstrings, legacy);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("xcstrings") {
            xcstrings.push(path);
        }
    }
}

/// Discover all localization files in a directory tree.
/// Returns both modern .xcstrings and legacy .strings/.stringsdict files.
pub(crate) async fn handle_discover_files(
    params: DiscoverFilesParams,
) -> Result<serde_json::Value, XcStringsError> {
    let dir = PathBuf::from(&params.directory);
    if !dir.is_dir() {
        return Err(XcStringsError::InvalidPath {
            path: dir,
            reason: "not a directory".to_string(),
        });
    }

    let mut xcstrings_paths = Vec::new();
    let mut legacy_paths = Vec::new();
    walk_localization_files(&dir, &mut xcstrings_paths, &mut legacy_paths);

    xcstrings_paths.sort();
    legacy_paths.sort_by(|a, b| a.0.cmp(&b.0));

    let files: Vec<DiscoveredFile> = xcstrings_paths
        .iter()
        .map(|p| DiscoveredFile {
            path: p.to_string_lossy().to_string(),
            file_type: "xcstrings",
        })
        .collect();
    let count = files.len();

    let legacy_files: Vec<DiscoveredFile> = legacy_paths
        .iter()
        .map(|(p, ft)| DiscoveredFile {
            path: p.to_string_lossy().to_string(),
            file_type: ft,
        })
        .collect();
    let legacy_count = legacy_files.len();

    let result = DiscoverFilesResult {
        files,
        count,
        legacy_files,
        legacy_count,
    };
    Ok(serde_json::to_value(result)?)
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
        handle_parse(&store, &cache, params_a, None).await.unwrap();

        let params_b = ParseParams {
            file_path: "/test/b.xcstrings".to_string(),
        };
        handle_parse(&store, &cache, params_b, None).await.unwrap();

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

    #[tokio::test]
    async fn test_discover_files_on_fixtures() {
        let fixture_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
        let params = DiscoverFilesParams {
            directory: fixture_dir.to_string(),
        };
        let result = handle_discover_files(params).await.unwrap();

        let count = result["count"].as_u64().unwrap();
        assert!(count > 0, "should find at least one .xcstrings file");

        let files = result["files"].as_array().unwrap();
        assert!(
            files
                .iter()
                .any(|f| f["path"].as_str().unwrap().ends_with(".xcstrings"))
        );

        // Verify sorted
        let paths: Vec<&str> = files.iter().map(|f| f["path"].as_str().unwrap()).collect();
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(paths, sorted, "discover_files output must be sorted");
    }

    #[tokio::test]
    async fn test_discover_files_finds_legacy() {
        let fixture_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
        let params = DiscoverFilesParams {
            directory: fixture_dir.to_string(),
        };
        let result = handle_discover_files(params).await.unwrap();

        let legacy_count = result["legacy_count"].as_u64().unwrap();
        assert!(
            legacy_count > 0,
            "should find legacy .strings/.stringsdict files"
        );

        let legacy_files = result["legacy_files"].as_array().unwrap();
        assert!(
            legacy_files
                .iter()
                .any(|f| f["file_type"].as_str() == Some("strings"))
        );
        assert!(
            legacy_files
                .iter()
                .any(|f| f["file_type"].as_str() == Some("stringsdict"))
        );
    }

    #[tokio::test]
    async fn test_discover_files_invalid_dir() {
        let params = DiscoverFilesParams {
            directory: "/nonexistent/path".to_string(),
        };
        let result = handle_discover_files(params).await;
        assert!(result.is_err());
    }
}
