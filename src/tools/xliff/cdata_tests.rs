use super::*;
use crate::service::parser;
use crate::tools::test_helpers::MemoryStore;
use std::path::Path;

const CATALOG_PATH: &str = "/test/catalog.xcstrings";
const XLIFF_PATH: &str = "/test/input.xliff";
const MIXED_TARGET: &str = "Start %@ <b>&raw + & end";
const FORMAT_FIXTURE: &str = r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "greeting" : {
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "Hello %@"
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

fn mixed_cdata_document() -> String {
    document(
        r#"<file target-language="de"><body><trans-unit id="greeting"><source>Hello %@</source><target>Start <![CDATA[%@ <b>&raw]]> + &amp; end</target></trans-unit></body></file>"#,
    )
}

async fn run_import(
    store: &MemoryStore,
    cache: &Mutex<FileCache>,
    write_lock: &Mutex<()>,
    xliff: &str,
    dry_run: bool,
) -> Result<serde_json::Value, XcStringsError> {
    store.add_file(XLIFF_PATH, xliff);
    handle_import_xliff(
        store,
        cache,
        write_lock,
        ImportXliffParams {
            file_path: Some(CATALOG_PATH.to_string()),
            xliff_path: XLIFF_PATH.to_string(),
            dry_run,
        },
    )
    .await
}

#[tokio::test]
async fn mcp_cdata_dry_run_preserves_full_value_without_write() {
    let store = MemoryStore::new();
    store.add_file(CATALOG_PATH, FORMAT_FIXTURE);
    let before = store.get_content(Path::new(CATALOG_PATH)).unwrap();

    let result = run_import(
        &store,
        &Mutex::new(FileCache::new()),
        &Mutex::new(()),
        &mixed_cdata_document(),
        true,
    )
    .await
    .unwrap();

    assert_eq!(result["accepted"], 1);
    assert_eq!(result["accepted_keys"], serde_json::json!(["greeting"]));
    assert_eq!(result["rejected"], serde_json::json!([]));
    assert_eq!(result["dry_run"], true);
    assert_eq!(store.get_content(Path::new(CATALOG_PATH)).unwrap(), before);
}

#[tokio::test]
async fn mcp_cdata_apply_writes_full_value() {
    let store = MemoryStore::new();
    store.add_file(CATALOG_PATH, FORMAT_FIXTURE);

    let result = run_import(
        &store,
        &Mutex::new(FileCache::new()),
        &Mutex::new(()),
        &mixed_cdata_document(),
        false,
    )
    .await
    .unwrap();

    assert_eq!(result["accepted"], 1);
    assert_eq!(result["accepted_keys"], serde_json::json!(["greeting"]));
    assert_eq!(result["rejected"], serde_json::json!([]));
    let written = store.get_content(Path::new(CATALOG_PATH)).unwrap();
    let parsed = parser::parse(&written).unwrap();
    let unit = parsed.strings["greeting"].localizations.as_ref().unwrap()["de"]
        .string_unit
        .as_ref()
        .unwrap();
    assert_eq!(unit.value, MIXED_TARGET);
}

async fn assert_parse_error_without_write(xliff: &str, expected: &str) {
    let store = MemoryStore::new();
    store.add_file(CATALOG_PATH, FORMAT_FIXTURE);
    let before = store.get_content(Path::new(CATALOG_PATH)).unwrap();

    let error = run_import(
        &store,
        &Mutex::new(FileCache::new()),
        &Mutex::new(()),
        xliff,
        false,
    )
    .await
    .unwrap_err();

    assert_eq!(error.to_string(), format!("XLIFF parse error: {expected}"));
    assert_eq!(store.get_content(Path::new(CATALOG_PATH)).unwrap(), before);
}

#[tokio::test]
async fn mcp_rejects_cdata_outside_root_without_write() {
    let xliff = r#"<![CDATA[outside]]><xliff xmlns="urn:oasis:names:tc:xliff:document:1.2" version="1.2"><file target-language="de"><body/></file></xliff>"#.to_string();
    assert_parse_error_without_write(&xliff, "CDATA is not allowed outside <xliff> document root")
        .await;
}

#[tokio::test]
async fn mcp_rejects_unclosed_cdata_without_write() {
    let xliff = document(
        r#"<file target-language="de"><body><trans-unit id="greeting"><source>Hello</source><target><![CDATA[unterminated</target></trans-unit></body></file>"#,
    );
    assert_parse_error_without_write(
        &xliff,
        "syntax error: CDATA not closed: `]]>` not found before end of input",
    )
    .await;
}
