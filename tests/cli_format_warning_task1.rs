use std::fs;

use assert_cmd::Command;
use indexmap::IndexMap;
use xcstrings_mcp::model::xcstrings::{
    Localization, StringEntry, StringUnit, TranslationState, XcStringsFile,
};
use xcstrings_mcp::service::formatter;

#[test]
fn cli_xliff_import_returns_machine_readable_ambiguous_warning() {
    let temp = tempfile::tempdir().unwrap();
    let catalog_path = temp.path().join("Localizable.xcstrings");
    let xliff_path = temp.path().join("translations.xliff");
    fs::write(&catalog_path, prose_catalog()).unwrap();
    fs::write(
        &xliff_path,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2"><file target-language="de"><body>
<trans-unit id="storage"><source>100% Local Storage</source>
<target>100% lokaler Speicher</target></trans-unit>
</body></file></xliff>"#,
    )
    .unwrap();

    let output = Command::cargo_bin("xcstrings-mcp")
        .unwrap()
        .args([
            "import",
            catalog_path.to_str().unwrap(),
            "--xliff",
            xliff_path.to_str().unwrap(),
            "--dry-run",
            "--json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success(), "{:?}", output);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["accepted"], 1);
    assert!(json["rejected"].as_array().unwrap().is_empty());
    assert_eq!(
        json["warnings"][0]["issue_type"],
        "ambiguous_format_sequence_mismatch"
    );
}

#[test]
fn cli_xliff_import_blocks_definite_modifier_mismatch() {
    let temp = tempfile::tempdir().unwrap();
    let catalog_path = temp.path().join("Localizable.xcstrings");
    let xliff_path = temp.path().join("translations.xliff");
    fs::write(&catalog_path, simple_catalog("days", "%lld days")).unwrap();
    fs::write(
        &xliff_path,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2"><file target-language="de"><body>
<trans-unit id="days"><source>%lld days</source>
<target>%Ld Tage</target></trans-unit>
</body></file></xliff>"#,
    )
    .unwrap();

    let output = Command::cargo_bin("xcstrings-mcp")
        .unwrap()
        .args([
            "import",
            catalog_path.to_str().unwrap(),
            "--xliff",
            xliff_path.to_str().unwrap(),
            "--dry-run",
            "--json",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["accepted"], 0);
    assert_eq!(json["rejected"].as_array().unwrap().len(), 1);
    assert!(
        json["rejected"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("invalid format sequence %Ld")
    );
    assert!(json["warnings"].is_null());
}

fn prose_catalog() -> String {
    simple_catalog("storage", "100% Local Storage")
}

fn simple_catalog(key: &str, value: &str) -> String {
    let localization = Localization {
        string_unit: Some(StringUnit {
            state: TranslationState::Translated,
            value: value.to_string(),
        }),
        variations: None,
        substitutions: None,
    };
    let entry = StringEntry {
        extraction_state: None,
        should_translate: true,
        comment: None,
        localizations: Some(IndexMap::from([("en".to_string(), localization)])),
    };
    formatter::format_xcstrings(&XcStringsFile {
        source_language: "en".to_string(),
        strings: IndexMap::from([(key.to_string(), entry)]),
        version: "1.0".to_string(),
    })
    .unwrap()
}
