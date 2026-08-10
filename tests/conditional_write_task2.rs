use std::path::Path;
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant, SystemTime};

use tempfile::TempDir;
use xcstrings_mcp::error::XcStringsError;
use xcstrings_mcp::io::{FileStore, fs::FsFileStore};

struct LegacyStore;

impl FileStore for LegacyStore {
    fn read(&self, path: &Path) -> Result<String, XcStringsError> {
        Err(XcStringsError::FileNotFound { path: path.into() })
    }

    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, XcStringsError> {
        Err(XcStringsError::FileNotFound { path: path.into() })
    }

    fn write(&self, _path: &Path, _content: &str) -> Result<(), XcStringsError> {
        Ok(())
    }

    fn modified_time(&self, path: &Path) -> Result<SystemTime, XcStringsError> {
        Err(XcStringsError::FileNotFound { path: path.into() })
    }

    fn exists(&self, _path: &Path) -> bool {
        false
    }

    fn create_parent_dirs(&self, _path: &Path) -> Result<(), XcStringsError> {
        Ok(())
    }
}

#[test]
fn default_conditional_write_fails_closed_for_source_compatible_stores() {
    let error = LegacyStore
        .write_if_matches(Path::new("/output.xcstrings"), None, "new")
        .unwrap_err();
    assert!(matches!(
        error,
        XcStringsError::ConditionalWriteUnsupported { path }
            if path == Path::new("/output.xcstrings")
    ));
}

#[test]
fn fs_conditional_write_compares_exact_raw_bytes_and_expected_absence() {
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("output.xcstrings");
    let store = FsFileStore::new();

    store.write_if_matches(&output, None, "first").unwrap();
    assert_eq!(std::fs::read(&output).unwrap(), b"first");

    let stale_absent = store.write_if_matches(&output, None, "wrong").unwrap_err();
    assert!(matches!(
        stale_absent,
        XcStringsError::ConditionalWriteConflict {
            expected_exists: false,
            actual_exists: true,
            ..
        }
    ));
    assert_eq!(std::fs::read(&output).unwrap(), b"first");

    let stale_bytes = store
        .write_if_matches(&output, Some(b"First"), "wrong")
        .unwrap_err();
    assert!(matches!(
        stale_bytes,
        XcStringsError::ConditionalWriteConflict {
            expected_exists: true,
            actual_exists: true,
            ..
        }
    ));
    assert_eq!(std::fs::read(&output).unwrap(), b"first");

    store
        .write_if_matches(&output, Some(b"first"), "second")
        .unwrap();
    assert_eq!(std::fs::read(&output).unwrap(), b"second");
}

#[test]
fn fs_conditional_write_rejects_expected_existing_when_target_is_missing() {
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("missing.xcstrings");

    let error = FsFileStore::new()
        .write_if_matches(&output, Some(b"expected"), "replacement")
        .unwrap_err();
    assert!(matches!(
        error,
        XcStringsError::ConditionalWriteConflict {
            expected_exists: true,
            actual_exists: false,
            ..
        }
    ));
    assert!(!output.exists());
}

#[test]
fn expected_absent_race_has_exactly_one_winner_and_no_partial_file() {
    let dir = TempDir::new().unwrap();
    let output = Arc::new(dir.path().join("race.xcstrings"));
    let barrier = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();

    for content in ["current-winner", "incoming-winner"] {
        let output = Arc::clone(&output);
        let barrier = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            let store = FsFileStore::new();
            barrier.wait();
            store.write_if_matches(&output, None, content)
        }));
    }
    barrier.wait();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(XcStringsError::ConditionalWriteConflict { .. })))
            .count(),
        1
    );
    let bytes = std::fs::read(&*output).unwrap();
    assert!(bytes == b"current-winner" || bytes == b"incoming-winner");

    let entries = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(
        entries
            .iter()
            .any(|name| name.ends_with(".xcstrings-mcp.lock"))
    );
    assert!(!entries.iter().any(|name| name.ends_with(".tmp")));
}

#[cfg(unix)]
#[test]
fn expected_absence_treats_a_dangling_symlink_as_an_existing_path_object() {
    use std::os::unix::fs::symlink;

    let dir = TempDir::new().unwrap();
    let missing_target = dir.path().join("missing-target.xcstrings");
    let output = dir.path().join("dangling-output.xcstrings");
    symlink(&missing_target, &output).unwrap();

    let store = FsFileStore::new();
    assert!(store.exists(&output));
    let error = store
        .write_if_matches(&output, None, "must not replace the link")
        .unwrap_err();

    assert!(matches!(
        error,
        XcStringsError::ConditionalWriteConflict {
            expected_exists: false,
            actual_exists: true,
            ..
        }
    ));
    assert!(
        std::fs::symlink_metadata(&output)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_link(&output).unwrap(), missing_target);
}

#[test]
fn constructing_a_second_store_cannot_remove_an_active_cooperating_writers_temp() {
    let cwd = std::env::current_dir().unwrap();
    let target_file = tempfile::Builder::new()
        .prefix("xcstrings-mcp-active-writer-")
        .suffix(".xcstrings")
        .tempfile_in(&cwd)
        .unwrap();
    let target = target_file.path().to_path_buf();
    target_file.close().unwrap();
    let lock = target.with_file_name(format!(
        "{}.xcstrings-mcp.lock",
        target.file_name().unwrap().to_string_lossy()
    ));
    let owned_temp = target.with_file_name(format!(
        ".{}.xcstrings-mcp.tmp",
        target.file_name().unwrap().to_string_lossy()
    ));
    let content = "x".repeat(128 * 1024 * 1024);
    let expected = content.as_bytes().to_vec();
    let writer_target = target.clone();
    let writer = std::thread::spawn(move || FsFileStore::new().write(&writer_target, &content));

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let legacy_temp_exists = std::fs::read_dir(&cwd).unwrap().flatten().any(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with(&format!(".xcstrings-mcp-{}-", std::process::id()))
                && name.ends_with(".tmp")
        });
        if owned_temp.exists() || legacy_temp_exists {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "writer never exposed its atomic temp file"
        );
        std::thread::yield_now();
    }

    let _second_store = FsFileStore::new();
    writer.join().unwrap().unwrap();

    assert_eq!(std::fs::read(&target).unwrap(), expected);
    assert!(!owned_temp.exists());
    let legacy_temp_exists = std::fs::read_dir(&cwd).unwrap().flatten().any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        name.starts_with(&format!(".xcstrings-mcp-{}-", std::process::id()))
            && name.ends_with(".tmp")
    });
    assert!(!legacy_temp_exists);

    std::fs::remove_file(&target).unwrap();
    std::fs::remove_file(&lock).unwrap();
}

#[test]
fn target_owned_regular_orphan_is_cleaned_under_lock_at_the_next_write() {
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("orphaned-output.xcstrings");
    let orphan = dir
        .path()
        .join(".orphaned-output.xcstrings.xcstrings-mcp.tmp");
    std::fs::write(&orphan, b"incomplete previous write").unwrap();

    FsFileStore::new()
        .write_if_matches(&output, None, "complete replacement")
        .unwrap();

    assert_eq!(std::fs::read(&output).unwrap(), b"complete replacement");
    assert!(!orphan.exists());
}

#[cfg(unix)]
#[test]
fn target_owned_non_regular_temp_fails_closed_without_deleting_or_replacing() {
    use std::os::unix::fs::symlink;

    let dir = TempDir::new().unwrap();
    let output = dir.path().join("guarded-output.xcstrings");
    let temp = dir
        .path()
        .join(".guarded-output.xcstrings.xcstrings-mcp.tmp");
    let sentinel = dir.path().join("sentinel");
    std::fs::write(&output, b"original").unwrap();
    std::fs::write(&sentinel, b"must survive").unwrap();
    symlink(&sentinel, &temp).unwrap();

    let error = FsFileStore::new()
        .write_if_matches(&output, Some(b"original"), "replacement")
        .unwrap_err();

    let canonical_temp = std::fs::canonicalize(dir.path())
        .unwrap()
        .join(temp.file_name().unwrap());
    assert!(matches!(
        error,
        XcStringsError::InvalidPath { path, reason }
            if path == canonical_temp && reason == "target-owned temp path is not a regular file"
    ));
    assert_eq!(std::fs::read(&output).unwrap(), b"original");
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"must survive");
    assert!(
        std::fs::symlink_metadata(&temp)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn catalog_alias_to_held_lock_fails_closed_and_same_target_writer_stays_excluded() {
    use std::fs::OpenOptions;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{MetadataExt, symlink};
    use std::sync::mpsc::{RecvTimeoutError, channel};

    let dir = TempDir::new().unwrap();
    let target = dir.path().join("a.xcstrings");
    let alias = dir.path().join("b.xcstrings");
    let lock = dir.path().join("a.xcstrings.xcstrings-mcp.lock");
    let temp = dir.path().join(".a.xcstrings.xcstrings-mcp.tmp");
    let store = FsFileStore::new();
    store.write(&target, "original").unwrap();

    let held = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock)
        .unwrap();
    assert_eq!(unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX) }, 0);
    let lock_before = std::fs::metadata(&lock).unwrap();
    symlink(&lock, &alias).unwrap();

    let alias_result = store.write(&alias, "must not replace a lock");
    let writer_target = target.clone();
    let (started_tx, started_rx) = channel();
    let (result_tx, result_rx) = channel();
    let writer = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        result_tx
            .send(FsFileStore::new().write_if_matches(
                &writer_target,
                Some(b"original"),
                "updated after unlock",
            ))
            .unwrap();
    });
    started_rx.recv().unwrap();
    let early_result = result_rx.recv_timeout(Duration::from_millis(500));
    let writer_was_excluded = matches!(early_result, Err(RecvTimeoutError::Timeout));

    assert_eq!(unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_UN) }, 0);
    let writer_result = match early_result {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => result_rx.recv().unwrap(),
        Err(error) => panic!("writer result channel failed: {error}"),
    };
    writer.join().unwrap();

    assert!(matches!(
        alias_result,
        Err(XcStringsError::InvalidPath { reason, .. })
            if reason == "path resolves to a reserved xcstrings-mcp sidecar"
    ));
    assert!(writer_was_excluded);
    writer_result.unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"updated after unlock");
    assert!(!temp.exists());
    assert!(
        std::fs::symlink_metadata(&alias)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_link(&alias).unwrap(), lock);
    assert_eq!(std::fs::read(&lock).unwrap(), b"");
    let lock_after = std::fs::metadata(&lock).unwrap();
    assert_eq!(lock_after.dev(), lock_before.dev());
    assert_eq!(lock_after.ino(), lock_before.ino());
    assert_eq!(lock_after.nlink(), 1);
}

#[cfg(unix)]
#[test]
fn catalog_alias_to_active_target_temp_fails_closed_without_changing_either_path() {
    use std::fs::OpenOptions;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::symlink;

    let dir = TempDir::new().unwrap();
    let target = dir.path().join("a.xcstrings");
    let alias = dir.path().join("b.xcstrings");
    let lock = dir.path().join("a.xcstrings.xcstrings-mcp.lock");
    let temp = dir.path().join(".a.xcstrings.xcstrings-mcp.tmp");
    FsFileStore::new().write(&target, "original").unwrap();
    let held = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock)
        .unwrap();
    assert_eq!(unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX) }, 0);
    std::fs::write(&temp, b"active writer bytes").unwrap();
    symlink(&temp, &alias).unwrap();

    let result = FsFileStore::new().write(&alias, "must not replace active temp");

    assert_eq!(unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_UN) }, 0);
    assert!(matches!(
        result,
        Err(XcStringsError::InvalidPath { reason, .. })
            if reason == "path resolves to a reserved xcstrings-mcp sidecar"
    ));
    assert_eq!(std::fs::read(&target).unwrap(), b"original");
    assert_eq!(std::fs::read(&temp).unwrap(), b"active writer bytes");
    assert!(
        std::fs::symlink_metadata(&alias)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_link(&alias).unwrap(), temp);
}

#[cfg(unix)]
#[test]
fn redirected_or_hardlinked_stable_lock_fails_closed_without_touching_sentinel() {
    use std::os::unix::fs::symlink;

    for lock_kind in ["symlink", "hardlink"] {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("guarded.xcstrings");
        let lock = dir.path().join("guarded.xcstrings.xcstrings-mcp.lock");
        let sentinel = dir.path().join("sentinel");
        std::fs::write(&sentinel, b"sentinel must survive").unwrap();
        match lock_kind {
            "symlink" => symlink(&sentinel, &lock).unwrap(),
            "hardlink" => std::fs::hard_link(&sentinel, &lock).unwrap(),
            _ => unreachable!(),
        }

        let error = FsFileStore::new()
            .write_if_matches(&target, None, "replacement")
            .unwrap_err();

        assert!(matches!(
            error,
            XcStringsError::InvalidPath { path, reason }
                if path.file_name() == lock.file_name()
                    && reason == "stable lock must be a uniquely linked regular sidecar"
        ));
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"sentinel must survive");
        assert!(!target.exists());
        if lock_kind == "symlink" {
            assert!(
                std::fs::symlink_metadata(&lock)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
        } else {
            assert_eq!(std::fs::metadata(&lock).unwrap().len(), 21);
        }
    }
}

#[cfg(unix)]
#[test]
fn live_xcstrings_alias_to_non_catalog_file_fails_closed_without_replacement() {
    use std::os::unix::fs::symlink;

    let dir = TempDir::new().unwrap();
    let sentinel = dir.path().join("sentinel.data");
    let alias = dir.path().join("catalog-alias.xcstrings");
    std::fs::write(&sentinel, b"not a catalog target").unwrap();
    symlink(&sentinel, &alias).unwrap();

    let error = FsFileStore::new()
        .write(&alias, "must not replace sentinel")
        .unwrap_err();

    assert!(matches!(
        error,
        XcStringsError::InvalidPath { reason, .. }
            if reason == "live .xcstrings alias must resolve to an .xcstrings catalog"
    ));
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"not a catalog target");
    assert!(
        std::fs::symlink_metadata(&alias)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_link(&alias).unwrap(), sentinel);
}

#[cfg(unix)]
#[test]
fn hardlink_catalog_spelling_cannot_replace_held_lock_inode() {
    use std::fs::OpenOptions;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::MetadataExt;

    let dir = TempDir::new().unwrap();
    let target = dir.path().join("a.xcstrings");
    let alias = dir.path().join("b.xcstrings");
    let lock = dir.path().join("a.xcstrings.xcstrings-mcp.lock");
    let store = FsFileStore::new();
    store.write(&target, "original").unwrap();
    let held = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock)
        .unwrap();
    assert_eq!(unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX) }, 0);
    let before = std::fs::metadata(&lock).unwrap();
    std::fs::hard_link(&lock, &alias).unwrap();

    store.write(&alias, "independent catalog bytes").unwrap();

    let after = std::fs::metadata(&lock).unwrap();
    assert_eq!(after.dev(), before.dev());
    assert_eq!(after.ino(), before.ino());
    assert_eq!(after.nlink(), 1);
    assert_eq!(std::fs::read(&lock).unwrap(), b"");
    assert_eq!(std::fs::read(&alias).unwrap(), b"independent catalog bytes");
    assert_eq!(unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_UN) }, 0);
}
