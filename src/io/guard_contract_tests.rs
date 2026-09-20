use super::{FilePrecondition, FileStore, fs::FsFileStore};
use crate::{XcStringsError, tools::test_helpers::MemoryStore};
use std::path::Path;

fn guard_contract(store: &dyn FileStore, root: &Path) {
    let output = root.join("catalog.xcstrings");
    let dependency = root.join("workflow.json");
    store.write(&output, "source one").unwrap();
    let absent = [FilePrecondition {
        path: &dependency,
        expected: None,
    }];
    store
        .write_if_inputs_match(&output, Some(b"source one"), &absent, "draft one")
        .unwrap();
    assert_eq!(store.read_bytes(&output).unwrap(), b"draft one");
    assert!(!store.exists(&dependency));

    store.write(&dependency, "new context").unwrap();
    let error = store
        .write_if_inputs_match(&output, Some(b"draft one"), &absent, "stale write")
        .unwrap_err();
    assert!(matches!(
        error,
        XcStringsError::ConditionalWriteConflict {
            expected_exists: false,
            actual_exists: true,
            ..
        }
    ));
    assert_eq!(store.read_bytes(&output).unwrap(), b"draft one");

    let current = [FilePrecondition {
        path: &dependency,
        expected: Some(b"new context"),
    }];
    let error = store
        .write_if_inputs_match(&output, Some(b"source one"), &current, "stale output")
        .unwrap_err();
    assert!(matches!(
        error,
        XcStringsError::ConditionalWriteConflict {
            expected_exists: true,
            actual_exists: true,
            ..
        }
    ));
    assert_eq!(store.read_bytes(&output).unwrap(), b"draft one");

    store
        .write_if_inputs_match(&output, Some(b"draft one"), &current, "current draft")
        .unwrap();
    assert_eq!(store.read_bytes(&output).unwrap(), b"current draft");
    assert_eq!(store.read_bytes(&dependency).unwrap(), b"new context");
}

#[test]
fn memory_store_obeys_guarded_write_contract() {
    guard_contract(&MemoryStore::new(), Path::new("/test"));
}

#[test]
fn filesystem_store_obeys_guarded_write_contract() {
    let dir = tempfile::TempDir::new().unwrap();
    guard_contract(&FsFileStore::new(), dir.path());
}
