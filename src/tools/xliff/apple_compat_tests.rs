use super::*;
use crate::service::parser;
use crate::tools::test_helpers::MemoryStore;
use std::path::Path;

const CATALOG_PATH: &str = "/test/catalog.xcstrings";
const XLIFF_PATH: &str = "/test/input.xliff";
const SIMPLE_FIXTURE: &str = include_str!("../../../tests/fixtures/simple.xcstrings");
const XCODE_EMPTY_ID_FIXTURE: &str =
    include_str!("../../../tests/fixtures/xcode_26_6_empty_id.xliff");
const PLURAL_FIXTURE: &str = include_str!("../../../tests/fixtures/with_plurals.xcstrings");
const EMPTY_KEY_FIXTURE: &str = r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "" : {
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : ""
          }
        }
      }
    }
  },
  "version" : "1.0"
}"#;

fn document(contents: &str) -> String {
    format!(
        r#"<xliff xmlns="urn:oasis:names:tc:xliff:document:1.2" version="1.2">{contents}</xliff>"#
    )
}

async fn import(
    store: &MemoryStore,
    catalog: &str,
    xliff: &str,
) -> Result<serde_json::Value, XcStringsError> {
    store.add_file(CATALOG_PATH, catalog);
    store.add_file(XLIFF_PATH, xliff);
    handle_import_xliff(
        store,
        &Mutex::new(FileCache::new()),
        &Mutex::new(()),
        ImportXliffParams {
            file_path: Some(CATALOG_PATH.to_string()),
            xliff_path: XLIFF_PATH.to_string(),
            dry_run: false,
        },
    )
    .await
}

#[tokio::test]
async fn mcp_export_excludes_variation_only_entries_from_file_and_count() {
    let store = MemoryStore::new();
    store.add_file(CATALOG_PATH, PLURAL_FIXTURE);

    let result = handle_export_xliff(
        &store,
        &Mutex::new(FileCache::new()),
        ExportXliffParams {
            file_path: Some(CATALOG_PATH.to_string()),
            locale: "uk".to_string(),
            output_path: "/test/output.xliff".to_string(),
            untranslated_only: false,
        },
    )
    .await
    .unwrap();

    assert_eq!(result["exported_count"], 1);
    let output = store.get_content(Path::new("/test/output.xliff")).unwrap();
    assert!(output.contains(r#"<trans-unit id="simple_key">"#));
    assert!(!output.contains(r#"<trans-unit id="days_remaining">"#));
    assert!(!output.contains(r#"<trans-unit id="items_count">"#));
    assert!(!output.contains(r#"<trans-unit id="photos_count">"#));
}

#[tokio::test]
async fn mcp_accepts_real_xcode_empty_id_empty_target_without_write() {
    let store = MemoryStore::new();
    let result = import(&store, SIMPLE_FIXTURE, XCODE_EMPTY_ID_FIXTURE)
        .await
        .unwrap();

    assert_eq!(result["accepted"], 0);
    assert_eq!(result["rejected"], serde_json::json!([]));
    assert_eq!(
        store.get_content(Path::new(CATALOG_PATH)).unwrap(),
        SIMPLE_FIXTURE
    );
}

#[tokio::test]
async fn mcp_rejects_nonempty_empty_id_when_catalog_has_no_empty_key_without_write() {
    let store = MemoryStore::new();
    let xliff = document(
        r#"<file target-language="de"><body><trans-unit id=""><source></source><target>Leerzeichenlos</target></trans-unit></body></file>"#,
    );
    let result = import(&store, SIMPLE_FIXTURE, &xliff).await.unwrap();

    assert_eq!(result["accepted"], 0);
    assert_eq!(
        result["rejected"],
        serde_json::json!([{"key": "", "reason": "key not found in file"}])
    );
    assert_eq!(
        store.get_content(Path::new(CATALOG_PATH)).unwrap(),
        SIMPLE_FIXTURE
    );
}

#[tokio::test]
async fn mcp_writes_nonempty_empty_id_only_when_catalog_has_exact_empty_key() {
    let store = MemoryStore::new();
    let xliff = document(
        r#"<file target-language="de"><body><trans-unit id=""><source></source><target>Leerzeichenlos</target></trans-unit></body></file>"#,
    );
    let result = import(&store, EMPTY_KEY_FIXTURE, &xliff).await.unwrap();

    assert_eq!(result["accepted"], 1);
    assert_eq!(result["accepted_keys"], serde_json::json!([""]));
    assert_eq!(result["rejected"], serde_json::json!([]));
    let written = store.get_content(Path::new(CATALOG_PATH)).unwrap();
    let parsed = parser::parse(&written).unwrap();
    let unit = parsed.strings[""].localizations.as_ref().unwrap()["de"]
        .string_unit
        .as_ref()
        .unwrap();
    assert_eq!(unit.value, "Leerzeichenlos");
}

async fn assert_parse_rejected_without_write(xliff: &str, expected: &str) {
    let store = MemoryStore::new();
    store.add_file(CATALOG_PATH, SIMPLE_FIXTURE);
    store.add_file(XLIFF_PATH, xliff);
    let before = store.get_content(Path::new(CATALOG_PATH)).unwrap();

    let error = import(&store, SIMPLE_FIXTURE, xliff).await.unwrap_err();

    assert_eq!(error.to_string(), format!("XLIFF parse error: {expected}"));
    assert_eq!(store.get_content(Path::new(CATALOG_PATH)).unwrap(), before);
}

#[tokio::test]
async fn mcp_rejects_variation_id_without_write() {
    let xliff = document(
        r#"<file target-language="uk"><body><trans-unit id="days_remaining|==|plural.one"><source>%lld day</source><target>%lld day left</target></trans-unit></body></file>"#,
    );
    assert_parse_rejected_without_write(
        &xliff,
        "Apple XLIFF variation unit id 'days_remaining|==|plural.one' is unsupported; import simple stringUnit ids only",
    )
    .await;
}

#[tokio::test]
async fn mcp_rejects_duplicate_id_in_one_file_without_write() {
    let xliff = document(
        r#"<file target-language="de"><body>
<trans-unit id="greeting"><source>Hello</source><target>Hallo</target></trans-unit>
<trans-unit id="greeting"><source>Hello again</source><target>Guten Tag</target></trans-unit>
</body></file>"#,
    );
    assert_parse_rejected_without_write(&xliff, "duplicate XLIFF unit id 'greeting' inside <file>")
        .await;
}

#[tokio::test]
async fn mcp_rejects_duplicate_id_across_files_without_write() {
    let xliff = document(
        r#"<file target-language="de"><body><trans-unit id="greeting"><source>Hello</source><target>Hallo</target></trans-unit></body></file>
<file target-language="de"><body><trans-unit id="greeting"><source>Hello again</source><target>Guten Tag</target></trans-unit></body></file>"#,
    );
    assert_parse_rejected_without_write(
        &xliff,
        "XLIFF unit id 'greeting' is repeated across <file> elements and cannot be flattened safely",
    )
    .await;
}
