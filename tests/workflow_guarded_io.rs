use std::path::Path;
use std::sync::{Arc, Barrier};

use tempfile::TempDir;
use xcstrings_mcp::error::XcStringsError;
use xcstrings_mcp::io::{FilePrecondition, FileStore, fs::FsFileStore};

fn input<'a>(path: &'a Path, expected: Option<&'a [u8]>) -> FilePrecondition<'a> {
    FilePrecondition { path, expected }
}

#[test]
fn guarded_write_preserves_inputs_and_replaces_only_matching_output() {
    let dir = TempDir::new().unwrap();
    let catalog = dir.path().join("catalog.xcstrings");
    let sidecar = dir.path().join("catalog.xcstrings.xcstrings-mcp.json");
    std::fs::write(&catalog, "source one").unwrap();
    let store = FsFileStore::new();

    store
        .write_if_inputs_match(
            &sidecar,
            None,
            &[input(&catalog, Some(b"source one"))],
            "checkpoint one",
        )
        .unwrap();

    assert_eq!(std::fs::read(&catalog).unwrap(), b"source one");
    assert_eq!(std::fs::read(&sidecar).unwrap(), b"checkpoint one");
}

#[test]
fn changed_guard_rejects_without_modifying_output() {
    let dir = TempDir::new().unwrap();
    let catalog = dir.path().join("catalog.xcstrings");
    let context = dir.path().join("context.json");
    std::fs::write(&catalog, "original catalog").unwrap();
    std::fs::write(&context, "new purpose").unwrap();

    let error = FsFileStore::new()
        .write_if_inputs_match(
            &catalog,
            Some(b"original catalog"),
            &[input(&context, Some(b"old purpose"))],
            "stale draft",
        )
        .unwrap_err();

    assert!(
        matches!(error, XcStringsError::ConditionalWriteConflict { path, expected_exists: true, actual_exists: true } if path == std::fs::canonicalize(&context).unwrap())
    );
    assert_eq!(std::fs::read(&catalog).unwrap(), b"original catalog");
    assert_eq!(std::fs::read(&context).unwrap(), b"new purpose");
}

#[test]
fn expected_absent_guard_rejects_a_dangling_symlink() {
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("catalog.xcstrings");
    let guard = dir.path().join("context.json");
    let missing = dir.path().join("missing.json");
    std::os::unix::fs::symlink(&missing, &guard).unwrap();

    let error = FsFileStore::new()
        .write_if_inputs_match(&output, None, &[input(&guard, None)], "must not write")
        .unwrap_err();

    assert!(matches!(
        error,
        XcStringsError::ConditionalWriteConflict {
            expected_exists: false,
            actual_exists: true,
            ..
        }
    ));
    assert!(!output.exists());
    assert_eq!(std::fs::read_link(&guard).unwrap(), missing);
}

#[test]
fn aliased_input_output_is_rejected_before_locking() {
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("catalog.xcstrings");
    let alias = dir.path().join("alias.xcstrings");
    std::fs::write(&output, "original").unwrap();
    std::os::unix::fs::symlink(&output, &alias).unwrap();

    let error = FsFileStore::new()
        .write_if_inputs_match(
            &output,
            Some(b"original"),
            &[input(&alias, Some(b"original"))],
            "replacement",
        )
        .unwrap_err();

    assert!(
        matches!(error, XcStringsError::InvalidPath { reason, .. } if reason == "guarded write inputs must have distinct canonical identities")
    );
    assert_eq!(std::fs::read(&output).unwrap(), b"original");
}

#[test]
fn opposing_guarded_writes_have_one_winner_without_deadlock() {
    let dir = TempDir::new().unwrap();
    let a = Arc::new(dir.path().join("a.json"));
    let b = Arc::new(dir.path().join("b.json"));
    std::fs::write(&*a, "a0").unwrap();
    std::fs::write(&*b, "b0").unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let run = |output: Arc<std::path::PathBuf>,
               dependency: Arc<std::path::PathBuf>,
               old: &'static [u8],
               dep: &'static [u8],
               new: &'static str| {
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            FsFileStore::new().write_if_inputs_match(
                &output,
                Some(old),
                &[input(&dependency, Some(dep))],
                new,
            )
        })
    };
    let first = run(Arc::clone(&a), Arc::clone(&b), b"a0", b"b0", "a1");
    let second = run(Arc::clone(&b), Arc::clone(&a), b"b0", b"a0", "b1");
    barrier.wait();
    let results = [first.join().unwrap(), second.join().unwrap()];

    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(XcStringsError::ConditionalWriteConflict { .. })))
            .count(),
        1
    );
    let values = (std::fs::read(&*a).unwrap(), std::fs::read(&*b).unwrap());
    assert!(
        values == (b"a1".to_vec(), b"b0".to_vec()) || values == (b"a0".to_vec(), b"b1".to_vec())
    );
}

#[test]
fn case_aliases_of_absent_paths_cannot_deadlock_on_one_physical_lock() {
    use std::os::unix::fs::MetadataExt;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    let dir = TempDir::new().unwrap();
    let output = dir.path().join("A.json");
    let dependency = dir.path().join("a.json");
    let first_lock = dir.path().join("A.json.xcstrings-mcp.lock");
    let second_lock = dir.path().join("a.json.xcstrings-mcp.lock");
    std::fs::write(&first_lock, "").unwrap();
    std::fs::write(&second_lock, "").unwrap();
    let first = std::fs::metadata(&first_lock).unwrap();
    let second = std::fs::metadata(&second_lock).unwrap();
    let same_lock = (first.dev(), first.ino()) == (second.dev(), second.ino());
    let (sender, receiver) = channel();
    let worker = std::thread::spawn(move || {
        sender
            .send(FsFileStore::new().write_if_inputs_match(
                &output,
                None,
                &[input(&dependency, None)],
                "new",
            ))
            .unwrap();
    });

    let result = receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("guarded write deadlocked on case-equivalent lock names");
    worker.join().unwrap();
    assert_eq!(result.is_err(), same_lock);
    if same_lock {
        assert!(
            matches!(result, Err(XcStringsError::InvalidPath { reason, .. }) if reason == "guarded write inputs share a physical lock identity")
        );
        assert!(!dir.path().join("A.json").exists());
    } else {
        assert_eq!(std::fs::read(dir.path().join("A.json")).unwrap(), b"new");
        assert!(!dir.path().join("a.json").exists());
    }
}

#[test]
fn conditional_metadata_write_refuses_a_redirect_with_matching_bytes() {
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("workflow.json");
    let sentinel = dir.path().join("sentinel.json");
    std::fs::write(&sentinel, "same bytes").unwrap();
    std::os::unix::fs::symlink(&sentinel, &output).unwrap();

    let error = FsFileStore::new()
        .write_if_matches(&output, Some(b"same bytes"), "new metadata")
        .unwrap_err();

    assert!(
        matches!(error, XcStringsError::InvalidPath { reason, .. } if reason == "conditional metadata path must not redirect to another file")
    );
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"same bytes");
    assert_eq!(std::fs::read_link(&output).unwrap(), sentinel);
}

#[test]
fn redirected_metadata_guard_cannot_authorize_a_catalog_write() {
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("catalog.xcstrings");
    let dependency = dir.path().join("workflow.json");
    let sentinel = dir.path().join("sentinel.json");
    std::fs::write(&output, "catalog").unwrap();
    std::fs::write(&sentinel, "same context").unwrap();
    std::os::unix::fs::symlink(&sentinel, &dependency).unwrap();

    let error = FsFileStore::new()
        .write_if_inputs_match(
            &output,
            Some(b"catalog"),
            &[input(&dependency, Some(b"same context"))],
            "new draft",
        )
        .unwrap_err();

    assert!(
        matches!(error, XcStringsError::InvalidPath { reason, .. } if reason == "conditional metadata path must not redirect to another file")
    );
    assert_eq!(std::fs::read(&output).unwrap(), b"catalog");
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"same context");
}
