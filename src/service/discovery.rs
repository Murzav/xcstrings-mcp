use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 20;

/// Recursively find .xcstrings and legacy localization files.
pub fn walk_localization_files(
    dir: &Path,
    xcstrings: &mut Vec<PathBuf>,
    legacy: &mut Vec<(PathBuf, &'static str)>,
) {
    walk_localization_files_inner(dir, xcstrings, legacy, 0);
}

fn walk_localization_files_inner(
    dir: &Path,
    xcstrings: &mut Vec<PathBuf>,
    legacy: &mut Vec<(PathBuf, &'static str)>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Skip symlinks to avoid escaping the project tree
        if path.is_symlink() {
            continue;
        }
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
                walk_localization_files_inner(&path, xcstrings, legacy, depth + 1);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("xcstrings") {
            xcstrings.push(path);
        }
    }
}

/// Convenience: find only .xcstrings files.
pub fn find_xcstrings_files(dir: &Path) -> Vec<PathBuf> {
    let mut xcstrings = Vec::new();
    let mut legacy = Vec::new();
    walk_localization_files(dir, &mut xcstrings, &mut legacy);
    xcstrings.sort();
    xcstrings
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn empty_directory_returns_nothing() {
        let tmp = TempDir::new().unwrap();
        let mut xcstrings = Vec::new();
        let mut legacy = Vec::new();
        walk_localization_files(tmp.path(), &mut xcstrings, &mut legacy);
        assert!(xcstrings.is_empty());
        assert!(legacy.is_empty());
    }

    #[test]
    fn finds_xcstrings_in_root() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("App.xcstrings"), "{}").unwrap();
        let result = find_xcstrings_files(tmp.path());
        assert_eq!(result.len(), 1);
        assert!(result[0].ends_with("App.xcstrings"));
    }

    #[test]
    fn finds_xcstrings_in_nested_dir() {
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("sub1").join("sub2");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("Nested.xcstrings"), "{}").unwrap();
        let result = find_xcstrings_files(tmp.path());
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn finds_legacy_in_lproj() {
        let tmp = TempDir::new().unwrap();
        let lproj = tmp.path().join("en.lproj");
        fs::create_dir_all(&lproj).unwrap();
        fs::write(lproj.join("Localizable.strings"), "").unwrap();
        fs::write(lproj.join("Localizable.stringsdict"), "").unwrap();
        fs::write(lproj.join("README.txt"), "").unwrap(); // should be ignored

        let mut xcstrings = Vec::new();
        let mut legacy = Vec::new();
        walk_localization_files(tmp.path(), &mut xcstrings, &mut legacy);
        assert!(xcstrings.is_empty());
        assert_eq!(legacy.len(), 2);
        let exts: Vec<&str> = legacy.iter().map(|(_, ext)| *ext).collect();
        assert!(exts.contains(&"strings"));
        assert!(exts.contains(&"stringsdict"));
    }

    #[test]
    fn ignores_non_localization_files() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("README.md"), "").unwrap();
        fs::write(tmp.path().join("data.json"), "{}").unwrap();
        let result = find_xcstrings_files(tmp.path());
        assert!(result.is_empty());
    }

    #[test]
    fn max_depth_protection() {
        let tmp = TempDir::new().unwrap();
        // Create a deep nesting beyond MAX_DEPTH (20)
        let mut deep_path = tmp.path().to_path_buf();
        for i in 0..25 {
            deep_path = deep_path.join(format!("d{i}"));
        }
        fs::create_dir_all(&deep_path).unwrap();
        fs::write(deep_path.join("Deep.xcstrings"), "{}").unwrap();

        let result = find_xcstrings_files(tmp.path());
        // File at depth 25 should not be found (MAX_DEPTH is 20)
        assert!(result.is_empty());
    }

    #[test]
    fn nonexistent_directory_returns_nothing() {
        let tmp = TempDir::new().unwrap();
        let nonexistent = tmp.path().join("does_not_exist");
        let mut xcstrings = Vec::new();
        let mut legacy = Vec::new();
        walk_localization_files(&nonexistent, &mut xcstrings, &mut legacy);
        assert!(xcstrings.is_empty());
        assert!(legacy.is_empty());
    }

    #[test]
    fn multiple_xcstrings_sorted() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("Zzz.xcstrings"), "{}").unwrap();
        fs::write(tmp.path().join("Aaa.xcstrings"), "{}").unwrap();
        let result = find_xcstrings_files(tmp.path());
        assert_eq!(result.len(), 2);
        // Should be sorted
        let names: Vec<_> = result
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(names, vec!["Aaa.xcstrings", "Zzz.xcstrings"]);
    }

    #[test]
    fn lproj_with_subdirectories_ignored() {
        let tmp = TempDir::new().unwrap();
        let lproj = tmp.path().join("en.lproj");
        let subdir = lproj.join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        // File in subdir of lproj should not be picked up (only direct children)
        fs::write(subdir.join("Nested.strings"), "").unwrap();
        fs::write(lproj.join("Direct.strings"), "").unwrap();

        let mut xcstrings = Vec::new();
        let mut legacy = Vec::new();
        walk_localization_files(tmp.path(), &mut xcstrings, &mut legacy);
        assert_eq!(legacy.len(), 1);
        assert!(legacy[0].0.ends_with("Direct.strings"));
    }
}
