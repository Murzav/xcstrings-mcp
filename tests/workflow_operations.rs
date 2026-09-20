use std::path::Path;

use tempfile::TempDir;
use xcstrings_mcp::io::{FilePrecondition, fs::FsFileStore};
use xcstrings_mcp::model::workflow::SyncMode;
use xcstrings_mcp::workflow_operation::{
    CatalogSnapshot, sidecar_path,
    sync::{SyncRequest, synchronize},
};
use xcstrings_mcp::{FileStore, XcStringsError};

const CATALOG: &str = r#"{"sourceLanguage":"en","strings":{"open":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Open"}},"de":{"stringUnit":{"state":"translated","value":"Öffnen"}}}}},"version":"1.0"}"#;

#[test]
fn missing_workflow_is_uninitialized_and_reads_never_create_it() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Localizable.xcstrings");
    std::fs::write(&path, CATALOG).unwrap();

    let snapshot = CatalogSnapshot::load(&FsFileStore::new(), &path).unwrap();

    assert!(snapshot.workflow.source_baseline.is_none());
    assert_eq!(snapshot.workflow_bytes, None);
    assert!(!snapshot.workflow_path.exists());
    assert_eq!(snapshot.catalog_bytes, CATALOG.as_bytes());
}

#[test]
fn context_edit_after_capture_rejects_catalog_write() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Localizable.xcstrings");
    std::fs::write(&path, CATALOG).unwrap();
    let store = FsFileStore::new();
    let snapshot = CatalogSnapshot::load(&store, &path).unwrap();
    std::fs::write(&snapshot.workflow_path, r#"{"version":1,"contexts":{}}"#).unwrap();

    let error = snapshot
        .write_catalog(&store, &snapshot.catalog)
        .unwrap_err();

    assert!(matches!(
        error,
        XcStringsError::ConditionalWriteConflict {
            expected_exists: false,
            actual_exists: true,
            ..
        }
    ));
    assert_eq!(std::fs::read(&path).unwrap(), CATALOG.as_bytes());
}

#[test]
fn redirected_workflow_is_rejected_without_reading_or_replacing_the_target() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Localizable.xcstrings");
    std::fs::write(&path, CATALOG).unwrap();
    let sentinel = dir.path().join("sentinel.json");
    std::fs::write(&sentinel, r#"{"version":1}"#).unwrap();
    std::os::unix::fs::symlink(&sentinel, sidecar_path(&path).unwrap()).unwrap();

    let result = CatalogSnapshot::load(&FsFileStore::new(), &path);

    assert!(
        matches!(result, Err(XcStringsError::InvalidPath { reason, .. }) if reason == "workflow sidecar must not redirect to another file")
    );
    assert_eq!(std::fs::read(&sentinel).unwrap(), br#"{"version":1}"#);
}

struct FailCheckpoint(FsFileStore);
impl FileStore for FailCheckpoint {
    fn file_identity(&self, path: &Path) -> Result<std::path::PathBuf, XcStringsError> {
        self.0.file_identity(path)
    }
    fn read(&self, path: &Path) -> Result<String, XcStringsError> {
        self.0.read(path)
    }
    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, XcStringsError> {
        self.0.read_bytes(path)
    }
    fn write(&self, path: &Path, content: &str) -> Result<(), XcStringsError> {
        self.0.write(path, content)
    }
    fn modified_time(&self, path: &Path) -> Result<std::time::SystemTime, XcStringsError> {
        self.0.modified_time(path)
    }
    fn exists(&self, path: &Path) -> bool {
        self.0.exists(path)
    }
    fn create_parent_dirs(&self, path: &Path) -> Result<(), XcStringsError> {
        self.0.create_parent_dirs(path)
    }
    fn write_if_inputs_match(
        &self,
        path: &Path,
        expected: Option<&[u8]>,
        inputs: &[FilePrecondition<'_>],
        content: &str,
    ) -> Result<(), XcStringsError> {
        if path.extension().and_then(|v| v.to_str()) == Some("json") {
            return Err(std::io::Error::other("injected checkpoint failure").into());
        }
        self.0
            .write_if_inputs_match(path, expected, inputs, content)
    }
}

#[test]
fn failed_checkpoint_reports_committed_invalidation_and_can_be_retried() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Localizable.xcstrings");
    std::fs::write(&path, CATALOG).unwrap();
    let store = FailCheckpoint(FsFileStore::new());
    let preview = synchronize(
        &store,
        &path,
        &SyncRequest {
            mode: SyncMode::Review,
            dry_run: true,
            expected: None,
        },
    )
    .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), CATALOG.as_bytes());

    let applied = synchronize(
        &store,
        &path,
        &SyncRequest {
            mode: SyncMode::Review,
            dry_run: false,
            expected: Some(preview.input_revisions),
        },
    )
    .unwrap();
    assert!(applied.catalog_written);
    assert!(!applied.checkpoint_written);
    assert!(applied.retry_required);
    assert!(
        applied
            .phase_error
            .unwrap()
            .contains("injected checkpoint failure")
    );
    let after = CatalogSnapshot::load(&store, &path).unwrap();
    let unit = after.catalog.strings["open"]
        .localizations
        .as_ref()
        .unwrap()["de"]
        .string_unit
        .as_ref()
        .unwrap();
    assert_eq!(
        unit.state,
        xcstrings_mcp::model::xcstrings::TranslationState::NeedsReview
    );
    assert_eq!(unit.value, "Öffnen");
    assert!(after.workflow.source_baseline.is_none());

    let retry = synchronize(
        &store.0,
        &path,
        &SyncRequest {
            mode: SyncMode::Review,
            dry_run: false,
            expected: Some(after.revisions()),
        },
    )
    .unwrap();
    assert!(!retry.catalog_written);
    assert!(retry.checkpoint_written);
    assert!(!retry.retry_required);
    assert_eq!(retry.phase_error, None);
    let completed = CatalogSnapshot::load(&store.0, &path).unwrap();
    assert!(completed.workflow.source_baseline.is_some());
}

#[test]
fn synchronization_without_captured_revisions_rejects_before_invalidation() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Localizable.xcstrings");
    std::fs::write(&path, CATALOG).unwrap();

    let error = synchronize(
        &FsFileStore::new(),
        &path,
        &SyncRequest {
            mode: SyncMode::Review,
            dry_run: false,
            expected: None,
        },
    )
    .unwrap_err();

    assert!(
        matches!(error, XcStringsError::InvalidFormat(message) if message == "sync apply requires the preview input revisions")
    );
    assert_eq!(std::fs::read(&path).unwrap(), CATALOG.as_bytes());
    assert!(!sidecar_path(&path).unwrap().exists());
}

#[test]
fn synchronization_rejects_catalog_edit_since_preview_without_writing_either_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Localizable.xcstrings");
    std::fs::write(&path, CATALOG).unwrap();
    let store = FsFileStore::new();
    let captured = CatalogSnapshot::load(&store, &path).unwrap().revisions();
    let edited = CATALOG.replace("\"Open\"", "\"Open document\"");
    std::fs::write(&path, &edited).unwrap();

    let error = synchronize(
        &store,
        &path,
        &SyncRequest {
            mode: SyncMode::Review,
            dry_run: false,
            expected: Some(captured),
        },
    )
    .unwrap_err();

    assert!(
        matches!(error, XcStringsError::InvalidFormat(message) if message == "stale workflow input: catalog")
    );
    assert_eq!(std::fs::read(&path).unwrap(), edited.as_bytes());
    assert!(!sidecar_path(&path).unwrap().exists());
}

#[test]
fn synchronization_rejects_workflow_edit_since_preview_without_writing_either_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Localizable.xcstrings");
    std::fs::write(&path, CATALOG).unwrap();
    let store = FsFileStore::new();
    let captured = CatalogSnapshot::load(&store, &path).unwrap().revisions();
    let metadata = r#"{"version":1,"contexts":{"open":{"context":{"role":"button"}}}}"#;
    let sidecar = sidecar_path(&path).unwrap();
    std::fs::write(&sidecar, metadata).unwrap();

    let error = synchronize(
        &store,
        &path,
        &SyncRequest {
            mode: SyncMode::Review,
            dry_run: false,
            expected: Some(captured),
        },
    )
    .unwrap_err();

    assert!(
        matches!(error, XcStringsError::InvalidFormat(message) if message == "stale workflow input: workflow")
    );
    assert_eq!(std::fs::read(&path).unwrap(), CATALOG.as_bytes());
    assert_eq!(std::fs::read(&sidecar).unwrap(), metadata.as_bytes());
}

#[test]
fn current_source_synchronization_is_a_byte_preserving_noop() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("Localizable.xcstrings");
    std::fs::write(&path, CATALOG).unwrap();
    let store = FsFileStore::new();
    let captured = CatalogSnapshot::load(&store, &path).unwrap().revisions();
    synchronize(
        &store,
        &path,
        &SyncRequest {
            mode: SyncMode::AdoptExisting,
            dry_run: false,
            expected: Some(captured),
        },
    )
    .unwrap();
    let snapshot = CatalogSnapshot::load(&store, &path).unwrap();
    let metadata = std::fs::read(&snapshot.workflow_path).unwrap();

    let result = synchronize(
        &store,
        &path,
        &SyncRequest {
            mode: SyncMode::Review,
            dry_run: false,
            expected: Some(snapshot.revisions()),
        },
    )
    .unwrap();

    assert!(!result.report.checkpoint_needed);
    assert!(!result.catalog_written);
    assert!(!result.checkpoint_written);
    assert!(!result.retry_required);
    assert_eq!(result.phase_error, None);
    assert_eq!(std::fs::read(&path).unwrap(), CATALOG.as_bytes());
    assert_eq!(std::fs::read(&snapshot.workflow_path).unwrap(), metadata);
}

#[test]
fn snapshot_rejects_non_catalog_path_before_reading_json() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("glossary.json");
    std::fs::write(&path, "sentinel").unwrap();

    let result = CatalogSnapshot::load(&FsFileStore::new(), &path);

    assert!(
        matches!(result, Err(XcStringsError::NotXcStrings {path: rejected}) if rejected == path)
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"sentinel");
}

#[test]
fn terminology_validation_skips_nontranslatable_keys_but_checks_translatable_text() {
    use xcstrings_mcp::model::glossary::TerminologyCode;
    use xcstrings_mcp::{guidance_operation::GuidanceSnapshot, service::parser};
    let dir = TempDir::new().unwrap();
    let glossary_path = dir.path().join("glossary.json");
    std::fs::write(&glossary_path, r#"{"en→de":{"Save":"Speichern"}}"#).unwrap();
    let catalog = parser::parse(r#"{"sourceLanguage":"en","version":"1.0","strings":{
        "internal":{"shouldTranslate":false,"localizations":{"en":{"stringUnit":{"state":"translated","value":"Save"}},"de":{"stringUnit":{"state":"translated","value":"Sichern"}}}},
        "button":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Save"}},"de":{"stringUnit":{"state":"needs_review","value":"Sichern"}}}}
    }}"#).unwrap();
    let original = serde_json::to_value(&catalog).unwrap();

    let report = GuidanceSnapshot::load(&FsFileStore::new(), &glossary_path).check_catalog(
        &catalog,
        &Default::default(),
        Some("de"),
    );

    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].key, "button");
    assert_eq!(report.issues[0].code, TerminologyCode::PreferredMissing);
    assert_eq!(report.issues[0].expected, vec!["Speichern"]);
    assert_eq!(serde_json::to_value(&catalog).unwrap(), original);
}
