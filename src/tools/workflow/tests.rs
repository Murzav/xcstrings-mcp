use super::*;
use crate::model::workflow::SyncMode;
use crate::tools::test_helpers::MemoryStore;
use serde_json::json;
use std::path::Path;

const PATH: &str = "/test/Localizable.xcstrings";
const CATALOG: &str = r#"{"sourceLanguage":"en","strings":{"open":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Open"}},"fr":{"stringUnit":{"state":"translated","value":"Ouvrir"}}}}},"version":"1.0"}"#;

fn tracked_store() -> MemoryStore {
    let store = MemoryStore::new();
    store.add_file(PATH, CATALOG);
    let preview = synchronize(
        &store,
        Path::new(PATH),
        &SyncRequest {
            mode: SyncMode::AdoptExisting,
            dry_run: true,
            expected: None,
        },
    )
    .unwrap();
    let result = synchronize(
        &store,
        Path::new(PATH),
        &SyncRequest {
            mode: SyncMode::AdoptExisting,
            dry_run: false,
            expected: Some(preview.input_revisions),
        },
    )
    .unwrap();
    assert!(result.checkpoint_written);
    store
}

#[tokio::test]
async fn coverage_excludes_changed_sources_before_any_invalidation_write() {
    let store = tracked_store();
    let changed = CATALOG.replace("\"Open\"", "\"Open permanently\"");
    store.update_file(PATH, &changed);
    let cache = Mutex::new(FileCache::new());

    let result = crate::tools::coverage::handle_get_coverage(
        &store,
        &cache,
        crate::tools::coverage::GetCoverageParams {
            file_path: Some(PATH.into()),
        },
    )
    .await
    .unwrap();

    let french = result["locales"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["locale"] == "fr")
        .unwrap();
    assert_eq!(french["translated"], 0);
    assert_eq!(french["percentage"], 0.0);
    assert_eq!(result["tracking"], "initialized");
    assert_eq!(store.get_content(Path::new(PATH)).unwrap(), changed);
}

#[tokio::test]
async fn guarded_context_edit_invalidates_readiness_and_preserves_native_text() {
    let store = tracked_store();
    let cache = Mutex::new(FileCache::new());
    let lock = Mutex::new(());
    let snapshot = CatalogSnapshot::load(&store, Path::new(PATH)).unwrap();
    let old_source =
        workflow::source_snapshot(&snapshot.catalog, &snapshot.workflow, "open").unwrap();
    let params: UpdateContextParams = serde_json::from_value(json!({"file_path":PATH,"expected":snapshot.revisions(),"edits":[{"action":"set","key":"open","context":{"context":{"purpose":"State indicator"}}}]})).unwrap();

    let result = handle_update_context(&store, &cache, &lock, params)
        .await
        .unwrap();

    assert_eq!(result["written"], true);
    assert_eq!(store.get_content(Path::new(PATH)).unwrap(), CATALOG);
    let after = CatalogSnapshot::load(&store, Path::new(PATH)).unwrap();
    assert_ne!(
        workflow::source_snapshot(&after.catalog, &after.workflow, "open").unwrap(),
        old_source
    );
    assert_eq!(
        after.workflow.contexts["open"].context.purpose.as_deref(),
        Some("State indicator")
    );
}

#[tokio::test]
async fn context_apply_requires_a_captured_revision_without_writing() {
    let store = tracked_store();
    let cache = Mutex::new(FileCache::new());
    let lock = Mutex::new(());
    let before = CatalogSnapshot::load(&store, Path::new(PATH)).unwrap();

    let error = handle_update_context(
        &store,
        &cache,
        &lock,
        UpdateContextParams {
            file_path: Some(PATH.into()),
            edits: vec![],
            expected: None,
            dry_run: false,
        },
    )
    .await
    .unwrap_err();

    assert!(
        matches!(error, XcStringsError::InvalidFormat(ref message) if message == "context apply requires captured input revisions")
    );
    assert_eq!(
        store.read_bytes(&before.workflow_path).unwrap(),
        before.workflow_bytes.unwrap()
    );
    assert_eq!(store.get_content(Path::new(PATH)).unwrap(), CATALOG);
}
