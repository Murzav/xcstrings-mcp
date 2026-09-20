use serde_json::json;
use xcstrings_mcp::model::xcstrings::XcStringsFile;
use xcstrings_mcp::service::xliff::{export_xliff, import_xliff};

fn catalog(key: &str, target: &str) -> XcStringsFile {
    serde_json::from_value(json!({
        "sourceLanguage": "en",
        "strings": {
            key: {
                "localizations": {
                    "en": {"stringUnit": {"state": "translated", "value": "Source"}},
                    "de": {"stringUnit": {"state": "translated", "value": target}}
                }
            }
        },
        "version": "1.0"
    }))
    .unwrap()
}

#[test]
fn xliff_roundtrip_preserves_control_whitespace_in_unit_id() {
    let file = catalog("key\r\nwith\tspace & café", "Übersetzung");

    let (xml, count) = export_xliff(&file, "de", "Localizable.xcstrings", false).unwrap();
    let (locale, translations) = import_xliff(&xml).unwrap();

    assert_eq!(count, 1);
    assert!(xml.contains("id=\"key&#13;&#10;with&#9;space &amp; café\""));
    assert_eq!(locale, "de");
    assert_eq!(
        serde_json::to_value(translations).unwrap(),
        json!([{"expected_source_version":"", "key": "key\r\nwith\tspace & café", "locale": "de", "value": "Übersetzung"}])
    );
}

#[test]
fn xliff_roundtrip_preserves_carriage_returns_in_translation() {
    let file = catalog("message", "Erste\r\nZweite\rDritte\n\t& Ende");

    let (xml, count) = export_xliff(&file, "de", "Localizable.xcstrings", false).unwrap();
    let (locale, translations) = import_xliff(&xml).unwrap();

    assert_eq!(count, 1);
    assert!(xml.contains("Erste&#13;\nZweite&#13;Dritte\n\t&amp; Ende"));
    assert_eq!(locale, "de");
    assert_eq!(
        serde_json::to_value(translations).unwrap(),
        json!([{"expected_source_version":"", "key": "message", "locale": "de", "value": "Erste\r\nZweite\rDritte\n\t& Ende"}])
    );
}

#[test]
fn xliff_import_preserves_subflow_nested_inside_inline_content() {
    let xml = r#"<xliff xmlns="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
<file target-language="de"><body><trans-unit id="subflow">
<source>Source</source><target>Vor <ph id="1">Code <sub>übersetzbar &amp; Text</sub></ph> nach</target>
</trans-unit></body></file></xliff>"#;

    let (locale, translations) = import_xliff(xml).unwrap();

    assert_eq!(locale, "de");
    assert_eq!(
        serde_json::to_value(translations).unwrap(),
        json!([{"expected_source_version":"", "key": "subflow", "locale": "de", "value": "Vor Code übersetzbar & Text nach"}])
    );
}

#[test]
fn xliff_import_rejects_subflow_bound_to_wrong_namespace() {
    let xml = r#"<xliff xmlns="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
<file target-language="de"><body><trans-unit id="subflow">
<source>Source</source><target><ph id="1"><sub xmlns="urn:wrong">Text</sub></ph></target>
</trans-unit></body></file></xliff>"#;

    let error = import_xliff(xml).unwrap_err();

    match error {
        xcstrings_mcp::error::XcStringsError::XliffParse(message) => assert_eq!(
            message,
            "element <sub> uses namespace 'urn:wrong'; expected 'urn:oasis:names:tc:xliff:document:1.2'"
        ),
        other => panic!("expected XliffParse, got {other:?}"),
    }
}
