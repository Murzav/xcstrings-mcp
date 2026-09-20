use xcstrings_mcp::{error::XcStringsError, service::xliff};

fn document(target: &str) -> String {
    format!(
        r#"<xliff version="1.2"><file original="A.xcstrings" target-language="de"><body><trans-unit id="key"><source>Source</source>{target}</trans-unit></body></file></xliff>"#
    )
}

#[test]
fn legacy_adapter_rejects_draft_state_instead_of_promoting_it() {
    let error =
        xliff::import_xliff(&document(r#"<target state="new">Entwurf</target>"#)).unwrap_err();

    assert!(matches!(error, XcStringsError::XliffParse(ref message)
        if message == "legacy import_xliff cannot preserve target state for 'key'; use parse_document and plan_import"));
}

#[test]
fn legacy_adapter_rejects_machine_state_instead_of_promoting_it() {
    let error = xliff::import_xliff(&document(
        r#"<target state="translated" state-qualifier="leveraged-mt">Maschinell</target>"#,
    ))
    .unwrap_err();

    assert!(matches!(error, XcStringsError::XliffParse(ref message)
        if message == "legacy import_xliff cannot preserve target state for 'key'; use parse_document and plan_import"));
}

#[test]
fn legacy_adapter_preserves_explicit_empty_ready_translation() {
    let (locale, translations) =
        xliff::import_xliff(&document(r#"<target state="translated"/>"#)).unwrap();

    assert_eq!(locale, "de");
    assert_eq!(
        serde_json::to_value(translations).unwrap(),
        serde_json::json!([
            {"expected_source_version":"", "key":"key", "locale":"de", "value":""}
        ])
    );
}

#[test]
fn legacy_adapter_rejects_distinct_originals_before_flattening_keys() {
    let xml = r#"<xliff version="1.2"><file original="A.xcstrings" target-language="de"><body><trans-unit id="first"><source>First</source><target>Erste</target></trans-unit></body></file><file original="B.xcstrings" target-language="de"><body><trans-unit id="second"><source>Second</source><target>Zweite</target></trans-unit></body></file></xliff>"#;

    let error = xliff::import_xliff(xml).unwrap_err();

    assert!(matches!(error, XcStringsError::XliffParse(ref message)
        if message == "legacy import_xliff cannot preserve multiple file originals; use parse_document and plan_import"));
}
