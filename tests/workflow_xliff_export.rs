mod workflow_xml_support;
use std::path::Path;
use workflow_xml_support::*;
use xcstrings_mcp::{
    error::XcStringsError,
    io::FileStore,
    xliff_operation::{prepare_export, save_export_bundle},
};
#[test]
fn export_bundle_keeps_map_and_xml_from_the_same_snapshot() {
    let store = Store::new();
    let snapshot = store.snapshot();
    let prepared = prepare_export(&snapshot, "de", "App/Catalog.xcstrings", false).unwrap();
    assert_eq!(prepared.exported_count, 1);
    assert_eq!(prepared.source_versions, store.versions());
    let report = save_export_bundle(
        &store,
        &snapshot,
        &prepared,
        Path::new("/test/de.xliff"),
        None,
    )
    .unwrap();
    assert!(report.xml_written);
    assert!(report.source_versions_written);
    assert_eq!(report.phase_error, None);
    assert_eq!(
        store.read(Path::new("/test/de.xliff")).unwrap(),
        prepared.xml
    );
    assert_eq!(
        serde_json::from_str::<std::collections::BTreeMap<String, String>>(
            &store
                .read(Path::new("/test/de.xliff.source-versions.json"))
                .unwrap()
        )
        .unwrap(),
        prepared.source_versions
    );
}
#[test]
fn export_second_output_conflict_reports_committed_map_without_false_xml_success() {
    let mut store = Store::new();
    store.xml_conflict = true;
    let snapshot = store.snapshot();
    let prepared = prepare_export(&snapshot, "de", "Catalog.xcstrings", false).unwrap();
    let report = save_export_bundle(
        &store,
        &snapshot,
        &prepared,
        Path::new("/test/de.xliff"),
        None,
    )
    .unwrap();
    assert!(!report.xml_written);
    assert!(report.source_versions_written);
    assert!(
        report
            .phase_error
            .unwrap()
            .contains("source-version map committed, but XML output was not written")
    );
    assert_eq!(
        store.read(Path::new("/test/de.xliff")).unwrap(),
        "external XML edit"
    );
    assert!(store.exists(Path::new("/test/de.xliff.source-versions.json")));
    assert_eq!(store.read(Path::new(PATH)).unwrap(), CATALOG);
}
#[test]
fn export_version_map_cannot_overwrite_workflow_sidecar() {
    let store = Store::new();
    let snapshot = store.snapshot();
    let prepared = prepare_export(&snapshot, "de", "Catalog.xcstrings", false).unwrap();
    let error = save_export_bundle(
        &store,
        &snapshot,
        &prepared,
        Path::new("/test/de.xliff"),
        Some(Path::new(SIDECAR)),
    )
    .unwrap_err();
    assert!(matches!(error, XcStringsError::InvalidPath { .. }));
    assert!(!store.exists(Path::new(SIDECAR)));
    assert!(!store.exists(Path::new("/test/de.xliff")));
}
#[test]
fn export_version_map_alias_cannot_overwrite_source_catalog() {
    let mut store = Store::new();
    store
        .aliases
        .insert("/test/versions.json".into(), PATH.into());
    let snapshot = store.snapshot();
    let prepared = prepare_export(&snapshot, "de", "Catalog.xcstrings", false).unwrap();
    let error = save_export_bundle(
        &store,
        &snapshot,
        &prepared,
        Path::new("/test/de.xliff"),
        Some(Path::new("/test/versions.json")),
    )
    .unwrap_err();
    assert!(matches!(error, XcStringsError::InvalidPath { .. }));
    assert_eq!(store.read(Path::new(PATH)).unwrap(), CATALOG);
    assert!(!store.exists(Path::new("/test/de.xliff")));
}

#[test]
fn untranslated_export_includes_stale_ready_targets_and_versions_for_emitted_keys_only() {
    let store = Store::new();
    let ready = r#"{"sourceLanguage":"en","version":"1.0","strings":{"k":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Delete"}},"de":{"stringUnit":{"state":"translated","value":"Löschen"}}}},"ready":{"localizations":{"de":{"stringUnit":{"state":"translated","value":"Fertig"}}}}}}"#;
    store.put(PATH, ready);
    store.adopt();
    let before = prepare_export(&store.snapshot(), "de", "Catalog.xcstrings", true).unwrap();
    assert_eq!(before.exported_count, 0);
    assert!(before.source_versions.is_empty());
    let changed = ready.replace("\"value\":\"Delete\"", "\"value\":\"Delete permanently\"");
    store.put(PATH, &changed);
    let prepared = prepare_export(&store.snapshot(), "de", "Catalog.xcstrings", true).unwrap();
    assert_eq!(prepared.exported_count, 1);
    assert_eq!(
        prepared.source_versions.keys().collect::<Vec<_>>(),
        vec!["k"]
    );
    let parsed = xcstrings_mcp::service::xliff::parse_document(&prepared.xml).unwrap();
    assert_eq!(
        parsed.files[0].units[0].state.as_deref(),
        Some("needs-review-l10n")
    );
    assert_eq!(parsed.files[0].units[0].target.as_deref(), Some("Löschen"));
    assert_eq!(store.read(Path::new(PATH)).unwrap(), changed);
}
