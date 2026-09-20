use super::*;
use crate::tools::test_helpers::MemoryStore;
use crate::workflow_operation::CatalogSnapshot;
use std::path::Path;

const PATH: &str = "/test/Guarded.xcstrings";
const CATALOG: &str = r#"{"sourceLanguage":"en","version":"1.0","strings":{"k":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Delete"}}}}}}"#;
fn request(version: String) -> SubmitTranslationsParams {
    SubmitTranslationsParams {
        file_path: Some(PATH.into()),
        translations: vec![CompletedTranslation {
            key: "k".into(),
            locale: "de".into(),
            value: "Löschen".into(),
            expected_source_version: version,
            ..Default::default()
        }],
        dry_run: false,
        continue_on_error: true,
    }
}
fn version(store: &MemoryStore) -> String {
    let snapshot = CatalogSnapshot::load(store, Path::new(PATH)).unwrap();
    crate::service::workflow::inspect(
        snapshot.identity_text().unwrap(),
        &snapshot.catalog,
        &snapshot.workflow,
    )
    .unwrap()
    .keys["k"]
        .source_version
        .clone()
}
#[tokio::test]
async fn stale_source_revision_rejects_native_save_without_writing() {
    let store = MemoryStore::new();
    store.add_file(PATH, CATALOG);
    let revision = version(&store);
    let changed = CATALOG.replace("Delete", "Delete permanently");
    store.update_file(PATH, &changed);
    let result = handle_submit_translations(
        &store,
        &Mutex::new(FileCache::new()),
        &Mutex::new(()),
        Path::new("/test/glossary.json"),
        request(revision),
    )
    .await
    .unwrap();
    assert_eq!(result["accepted"], 0);
    assert_eq!(result["rejected"][0]["code"], "source_version_mismatch");
    assert_eq!(store.read(Path::new(PATH)).unwrap(), changed);
}
#[tokio::test]
async fn empty_source_revision_is_not_a_compatibility_bypass() {
    let store = MemoryStore::new();
    store.add_file(PATH, CATALOG);
    let result = handle_submit_translations(
        &store,
        &Mutex::new(FileCache::new()),
        &Mutex::new(()),
        Path::new("/test/glossary.json"),
        request(String::new()),
    )
    .await
    .unwrap();
    assert_eq!(result["accepted"], 0);
    assert_eq!(result["rejected"][0]["code"], "source_version_mismatch");
    assert_eq!(store.read(Path::new(PATH)).unwrap(), CATALOG);
}
#[tokio::test]
async fn authored_context_change_rejects_inflight_native_save() {
    let store = MemoryStore::new();
    store.add_file(PATH, CATALOG);
    let revision = version(&store);
    store.add_file(
        "/test/Guarded.xcstrings.xcstrings-mcp.json",
        r#"{"version":1,"contexts":{"k":{"context":{"purpose":"Account removal"}}}}"#,
    );
    let result = handle_submit_translations(
        &store,
        &Mutex::new(FileCache::new()),
        &Mutex::new(()),
        Path::new("/test/glossary.json"),
        request(revision),
    )
    .await
    .unwrap();
    assert_eq!(result["accepted"], 0);
    assert_eq!(result["rejected"][0]["code"], "source_version_mismatch");
    assert_eq!(store.read(Path::new(PATH)).unwrap(), CATALOG);
}

struct ContextRaceStore {
    inner: MemoryStore,
}
impl FileStore for ContextRaceStore {
    fn read(&self, path: &Path) -> Result<String, XcStringsError> {
        self.inner.read(path)
    }
    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, XcStringsError> {
        self.inner.read_bytes(path)
    }
    fn write(&self, path: &Path, content: &str) -> Result<(), XcStringsError> {
        self.inner.write(path, content)
    }
    fn write_if_inputs_match(
        &self,
        path: &Path,
        expected: Option<&[u8]>,
        inputs: &[crate::io::FilePrecondition<'_>],
        content: &str,
    ) -> Result<(), XcStringsError> {
        self.inner.add_file(
            "/test/Guarded.xcstrings.xcstrings-mcp.json",
            r#"{"version":1,"contexts":{"k":{"context":{"purpose":"Changed during write"}}}}"#,
        );
        self.inner
            .write_if_inputs_match(path, expected, inputs, content)
    }
    fn modified_time(&self, path: &Path) -> Result<std::time::SystemTime, XcStringsError> {
        self.inner.modified_time(path)
    }
    fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path)
    }
    fn create_parent_dirs(&self, path: &Path) -> Result<(), XcStringsError> {
        self.inner.create_parent_dirs(path)
    }
}
#[tokio::test]
async fn context_creation_during_native_commit_prevents_stale_draft_write() {
    let inner = MemoryStore::new();
    inner.add_file(PATH, CATALOG);
    let revision = version(&inner);
    let store = ContextRaceStore { inner };
    let error = handle_submit_translations(
        &store,
        &Mutex::new(FileCache::new()),
        &Mutex::new(()),
        Path::new("/test/glossary.json"),
        request(revision),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        XcStringsError::ConditionalWriteConflict {
            expected_exists: false,
            actual_exists: true,
            ..
        }
    ));
    assert_eq!(store.read(Path::new(PATH)).unwrap(), CATALOG);
    assert_eq!(
        store
            .read(Path::new("/test/Guarded.xcstrings.xcstrings-mcp.json"))
            .unwrap(),
        r#"{"version":1,"contexts":{"k":{"context":{"purpose":"Changed during write"}}}}"#
    );
}
#[tokio::test]
async fn native_partial_batch_saves_only_current_source_requests_as_drafts() {
    let store = MemoryStore::new();
    let raw = r#"{"sourceLanguage":"en","version":"1.0","strings":{"a":{},"b":{}}}"#;
    store.add_file(PATH, raw);
    let snapshot = CatalogSnapshot::load(&store, Path::new(PATH)).unwrap();
    let view = crate::service::workflow::inspect(
        snapshot.identity_text().unwrap(),
        &snapshot.catalog,
        &snapshot.workflow,
    )
    .unwrap();
    let params = SubmitTranslationsParams {
        file_path: Some(PATH.into()),
        translations: vec![
            CompletedTranslation {
                key: "a".into(),
                locale: "de".into(),
                value: "A Text".into(),
                expected_source_version: "old".into(),
                ..Default::default()
            },
            CompletedTranslation {
                key: "b".into(),
                locale: "de".into(),
                value: "B Text".into(),
                expected_source_version: view.keys["b"].source_version.clone(),
                ..Default::default()
            },
        ],
        dry_run: false,
        continue_on_error: true,
    };
    let result = handle_submit_translations(
        &store,
        &Mutex::new(FileCache::new()),
        &Mutex::new(()),
        Path::new("/test/glossary.json"),
        params,
    )
    .await
    .unwrap();
    assert_eq!(result["accepted"], 1);
    assert_eq!(result["accepted_keys"], serde_json::json!(["b"]));
    assert_eq!(result["rejected"][0]["key"], "a");
    assert_eq!(result["rejected"][0]["code"], "source_version_mismatch");
    let written: serde_json::Value =
        serde_json::from_str(&store.read(Path::new(PATH)).unwrap()).unwrap();
    assert_eq!(written["strings"]["a"], serde_json::json!({}));
    assert_eq!(
        written["strings"]["b"]["localizations"]["de"]["stringUnit"],
        serde_json::json!({"state":"needs_review","value":"B Text"})
    );
}
