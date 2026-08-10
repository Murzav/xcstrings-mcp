use std::path::{Path, PathBuf};

use tempfile::TempDir;
use xcstrings_mcp::error::XcStringsError;
use xcstrings_mcp::io::{FileStore, fs::FsFileStore};

struct CurrentDirGuard(PathBuf);

impl CurrentDirGuard {
    fn enter(path: &Path) -> Self {
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(path).unwrap();
        Self(original)
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.0).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn fs_store_supports_bare_relative_cas_without_weakening_path_guards() {
    use std::os::unix::fs::symlink;

    let dir = TempDir::new().unwrap();
    let _cwd = CurrentDirGuard::enter(dir.path());
    let store = FsFileStore::new();
    let output = Path::new("output.xcstrings");

    store.write_if_matches(output, None, "first").unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"first");
    assert!(Path::new("output.xcstrings.xcstrings-mcp.lock").is_file());
    assert!(!Path::new(".output.xcstrings.xcstrings-mcp.tmp").exists());

    let stale_absence = store
        .write_if_matches(output, None, "must not replace")
        .unwrap_err();
    assert!(matches!(
        stale_absence,
        XcStringsError::ConditionalWriteConflict {
            expected_exists: false,
            actual_exists: true,
            ..
        }
    ));
    assert_eq!(std::fs::read(output).unwrap(), b"first");

    store
        .write_if_matches(output, Some(b"first"), "second")
        .unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"second");

    store
        .write_if_matches(Path::new("real.xcstrings"), None, "alias first")
        .unwrap();
    symlink("real.xcstrings", "alias.xcstrings").unwrap();
    store
        .write_if_matches(
            Path::new("alias.xcstrings"),
            Some(b"alias first"),
            "alias second",
        )
        .unwrap();
    assert_eq!(std::fs::read("real.xcstrings").unwrap(), b"alias second");
    assert_eq!(
        std::fs::read_link("alias.xcstrings").unwrap(),
        Path::new("real.xcstrings")
    );

    let traversal = store
        .write_if_matches(Path::new("../escape.xcstrings"), None, "escape")
        .unwrap_err();
    assert!(matches!(
        traversal,
        XcStringsError::InvalidPath { reason, .. }
            if reason == "path traversal detected (contains '..')"
    ));

    let reserved = store
        .write_if_matches(
            Path::new("output.xcstrings.xcstrings-mcp.lock"),
            Some(b""),
            "must not replace lock",
        )
        .unwrap_err();
    assert!(matches!(
        reserved,
        XcStringsError::InvalidPath { reason, .. }
            if reason == "path resolves to a reserved xcstrings-mcp sidecar"
    ));
    assert_eq!(
        std::fs::read("output.xcstrings.xcstrings-mcp.lock").unwrap(),
        b""
    );
}
