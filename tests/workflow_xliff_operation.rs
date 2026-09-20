mod workflow_xml_support;
use std::{collections::BTreeMap, path::Path};
use workflow_xml_support::*;
use xcstrings_mcp::{
    error::XcStringsError,
    io::FileStore,
    xliff_operation::{ImportOptions, execute_import},
};
fn import(
    store: &Store,
    xml: &str,
    versions: &BTreeMap<String, String>,
) -> Result<xcstrings_mcp::xliff_operation::ImportOutcome, XcStringsError> {
    execute_import(
        store,
        Path::new(PATH),
        xml,
        ImportOptions {
            original: None,
            dry_run: false,
            expected_source_versions: versions,
        },
        Path::new("/test/glossary.json"),
    )
}
#[test]
fn xml_draft_requires_exported_versions_and_preserves_draft_state() {
    let store = Store::new();
    let xml = xml("Delete", "Löschen", "needs-review-l10n");
    let missing = import(&store, &xml, &BTreeMap::new()).unwrap();
    assert_eq!(
        missing.result.report.rejected[0].code,
        "source_version_required"
    );
    assert_eq!(missing.result.report.accepted, 0);
    assert!(!missing.result.written);
    assert_eq!(store.read(Path::new(PATH)).unwrap(), CATALOG);
    let outcome = import(&store, &xml, &store.versions()).unwrap();
    assert_eq!(outcome.result.report.accepted, 1);
    assert!(outcome.result.written);
    let written: serde_json::Value =
        serde_json::from_str(&store.read(Path::new(PATH)).unwrap()).unwrap();
    assert_eq!(
        written["strings"]["k"]["localizations"]["de"]["stringUnit"],
        serde_json::json!({"state":"needs_review","value":"Löschen"})
    );
    assert_eq!(
        serde_json::to_value(&outcome.result.guidance).unwrap()["status"],
        "absent"
    );
}
#[test]
fn xml_ready_import_requires_current_checkpoint_even_with_matching_version() {
    let store = Store::new();
    let xml = xml("Delete", "Löschen", "translated");
    let untracked = import(&store, &xml, &store.versions()).unwrap();
    assert_eq!(untracked.result.report.accepted, 0);
    assert_eq!(
        untracked.result.report.rejected[0].code,
        "source_checkpoint_required"
    );
    assert_eq!(store.read(Path::new(PATH)).unwrap(), CATALOG);
    store.adopt();
    let ready = import(&store, &xml, &store.versions()).unwrap();
    assert_eq!(ready.result.report.accepted, 1);
    assert!(ready.result.written);
    assert_eq!(
        serde_json::to_value(ready.updated_file.unwrap()).unwrap()["strings"]["k"]["localizations"]
            ["de"]["stringUnit"]["state"],
        "translated"
    );
}
#[test]
fn xml_context_change_rejects_old_map_even_when_xml_source_still_matches() {
    let store = Store::new();
    let versions = store.versions();
    store.put(SIDECAR, EXTERNAL_CONTEXT);
    let result = import(&store, &xml("Delete", "Löschen", "new"), &versions).unwrap();
    assert_eq!(result.result.report.accepted, 0);
    assert_eq!(
        result.result.report.rejected[0].code,
        "source_version_mismatch"
    );
    assert!(result.result.report.accepted_destinations.is_empty());
    assert_eq!(store.read(Path::new(PATH)).unwrap(), CATALOG);
}
#[test]
fn xml_commit_guards_absent_sidecar_against_concurrent_context_creation() {
    let mut store = Store::new();
    store.context_conflict = true;
    let error = import(&store, &xml("Delete", "Löschen", "new"), &store.versions()).unwrap_err();
    assert!(
        matches!(error,XcStringsError::ConditionalWriteConflict{path,expected_exists:false,actual_exists:true} if path==Path::new(SIDECAR))
    );
    assert_eq!(store.read(Path::new(PATH)).unwrap(), CATALOG);
    assert_eq!(store.read(Path::new(SIDECAR)).unwrap(), EXTERNAL_CONTEXT);
}
#[test]
fn xml_current_map_cannot_hide_outdated_source_text() {
    let store = Store::new();
    let result = import(
        &store,
        &xml("Old action", "Löschen", "new"),
        &store.versions(),
    )
    .unwrap();
    assert_eq!(result.result.report.accepted, 0);
    assert_eq!(
        result.result.report.rejected[0].code,
        "source_text_mismatch"
    );
    assert_eq!(store.read(Path::new(PATH)).unwrap(), CATALOG);
}
#[test]
fn xml_missing_target_is_noop_without_any_source_version() {
    let store = Store::new();
    let xml = r#"<xliff version="1.2"><file target-language="de"><body><trans-unit id="k"><source>Delete</source></trans-unit></body></file></xliff>"#;
    let result = import(&store, xml, &BTreeMap::new()).unwrap();
    assert_eq!(result.result.report.accepted, 0);
    assert_eq!(result.result.report.missing_targets, 1);
    assert!(result.result.report.rejected.is_empty());
    assert!(!result.result.written);
    assert_eq!(store.read(Path::new(PATH)).unwrap(), CATALOG);
}

#[test]
fn source_less_apple_plural_reimport_uses_captured_map_and_preserves_explicit_blank() {
    let store = Store::new();
    store.put(PATH,r#"{"sourceLanguage":"en","version":"1.1","strings":{"k":{"localizations":{"en":{"variations":{"plural":{"other":{"stringUnit":{"state":"translated","value":"%lld items"}}}}},"de":{"variations":{"plural":{"one":{"stringUnit":{"state":"new","value":"%lld Ding"}},"other":{"stringUnit":{"state":"new","value":"%lld Dinge"}}}}}}}}}"#);
    let xml = r#"<xliff version="1.2"><file target-language="de"><body><trans-unit id="k|==|plural.other"><target state="needs-review-l10n"></target></trans-unit></body></file></xliff>"#;
    let missing = import(&store, xml, &BTreeMap::new()).unwrap();
    assert_eq!(
        missing.result.report.rejected[0].code,
        "source_version_required"
    );
    assert_eq!(missing.result.report.accepted, 0);
    let captured = store.versions();
    let imported = import(&store, xml, &captured).unwrap();
    assert_eq!(imported.result.report.accepted, 1);
    let updated = serde_json::to_value(imported.updated_file.unwrap()).unwrap();
    assert_eq!(
        updated["strings"]["k"]["localizations"]["de"]["variations"]["plural"]["other"]["stringUnit"],
        serde_json::json!({"state":"needs_review","value":""})
    );
}
