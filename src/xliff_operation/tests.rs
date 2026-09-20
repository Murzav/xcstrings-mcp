use super::*;
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::SystemTime;

const CATALOG: &str = r#"{"sourceLanguage":"en","strings":{"greeting":{"future":7,"localizations":{"en":{"stringUnit":{"state":"translated","value":"Hello"}}}}},"version":"1.0","rootMetadata":{"keep":true}}"#;
const EXTERNAL_EDIT: &[u8] = b"external editor owns these bytes";

#[derive(Clone, Copy)]
enum WriteMode {
    Normal,
    ConcurrentEdit,
    Unsupported,
}

struct Store {
    bytes: Mutex<Vec<u8>>,
    mode: WriteMode,
}

impl Store {
    fn new(bytes: &[u8], mode: WriteMode) -> Self {
        Self {
            bytes: Mutex::new(bytes.to_vec()),
            mode,
        }
    }
    fn contents(&self) -> Vec<u8> {
        self.bytes.lock().unwrap().clone()
    }
}

impl FileStore for Store {
    fn read(&self, _: &Path) -> Result<String, XcStringsError> {
        String::from_utf8(self.contents())
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error).into())
    }
    fn read_bytes(&self, _: &Path) -> Result<Vec<u8>, XcStringsError> {
        Ok(self.contents())
    }
    fn write(&self, path: &Path, _: &str) -> Result<(), XcStringsError> {
        Err(XcStringsError::ConditionalWriteUnsupported {
            path: path.to_path_buf(),
        })
    }
    fn write_if_matches(
        &self,
        path: &Path,
        expected: Option<&[u8]>,
        content: &str,
    ) -> Result<(), XcStringsError> {
        let mut bytes = self.bytes.lock().unwrap();
        match self.mode {
            WriteMode::Unsupported => {
                return Err(XcStringsError::ConditionalWriteUnsupported {
                    path: path.to_path_buf(),
                });
            }
            WriteMode::ConcurrentEdit => *bytes = EXTERNAL_EDIT.to_vec(),
            WriteMode::Normal => {}
        }
        if expected != Some(bytes.as_slice()) {
            return Err(XcStringsError::ConditionalWriteConflict {
                path: path.to_path_buf(),
                expected_exists: expected.is_some(),
                actual_exists: true,
            });
        }
        *bytes = content.as_bytes().to_vec();
        Ok(())
    }
    fn write_if_inputs_match(
        &self,
        path: &Path,
        expected: Option<&[u8]>,
        inputs: &[crate::io::FilePrecondition<'_>],
        content: &str,
    ) -> Result<(), XcStringsError> {
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].expected, None);
        self.write_if_matches(path, expected, content)
    }
    fn modified_time(&self, _: &Path) -> Result<SystemTime, XcStringsError> {
        Ok(SystemTime::UNIX_EPOCH)
    }
    fn exists(&self, path: &Path) -> bool {
        path.extension().and_then(|part| part.to_str()) == Some("xcstrings")
    }
    fn create_parent_dirs(&self, _: &Path) -> Result<(), XcStringsError> {
        Ok(())
    }
}

fn xml(units: &str) -> String {
    format!(
        r#"<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2"><file original="Catalog.xcstrings" source-language="en" target-language="de" datatype="plaintext"><body>{units}</body></file></xliff>"#
    )
}

fn draft() -> String {
    xml(
        r#"<trans-unit id="greeting"><source>Hello</source><target state="needs-review-l10n">Hallo Entwurf</target></trans-unit>"#,
    )
}

fn execute(store: &Store, xml: &str, dry_run: bool) -> Result<ImportOutcome, XcStringsError> {
    let snapshot = CatalogSnapshot::load(store, Path::new("Catalog.xcstrings"))?;
    let versions = workflow::inspect(
        snapshot.identity_text()?,
        &snapshot.catalog,
        &snapshot.workflow,
    )?
    .keys
    .into_iter()
    .map(|(key, status)| (key, status.source_version))
    .collect();
    execute_import(
        store,
        Path::new("Catalog.xcstrings"),
        xml,
        ImportOptions {
            original: None,
            dry_run,
            expected_source_versions: &versions,
        },
        Path::new("glossary.json"),
    )
}

#[test]
fn draft_apply_preserves_status_unknown_metadata_and_exact_destination() {
    let store = Store::new(CATALOG.as_bytes(), WriteMode::Normal);
    let outcome = execute(&store, &draft(), false).unwrap();
    assert_eq!(outcome.result.report.accepted, 1);
    assert_eq!(outcome.result.accepted_keys, ["greeting"]);
    assert!(outcome.result.written);
    assert!(!outcome.result.dry_run);
    assert_eq!(
        serde_json::to_value(&outcome.result.report.accepted_destinations).unwrap(),
        json!([{
            "original":"Catalog.xcstrings", "key":"greeting", "locale":"de", "path":[], "unit_id":"greeting"
        }])
    );
    let mut expected: Value = serde_json::from_str(CATALOG).unwrap();
    expected["strings"]["greeting"]["localizations"]["de"] =
        json!({"stringUnit":{"state":"needs_review","value":"Hallo Entwurf"}});
    assert_eq!(
        serde_json::from_slice::<Value>(&store.contents()).unwrap(),
        expected
    );
    assert_eq!(
        serde_json::to_value(outcome.updated_file.unwrap()).unwrap(),
        expected
    );
}

#[test]
fn dry_run_and_apply_share_the_plan_without_dry_run_writes() {
    let dry_store = Store::new(CATALOG.as_bytes(), WriteMode::Unsupported);
    let dry = execute(&dry_store, &draft(), true).unwrap();
    let apply_store = Store::new(CATALOG.as_bytes(), WriteMode::Normal);
    let applied = execute(&apply_store, &draft(), false).unwrap();
    assert_eq!(
        serde_json::to_value(dry.result.report).unwrap(),
        serde_json::to_value(applied.result.report).unwrap()
    );
    assert_eq!(dry_store.contents(), CATALOG.as_bytes());
    assert!(!dry.result.written);
    assert!(dry.result.dry_run);
    assert!(dry.updated_file.is_none());
}

#[test]
fn one_unknown_key_rejects_the_whole_batch_without_changing_the_catalog() {
    let store = Store::new(CATALOG.as_bytes(), WriteMode::Normal);
    let document = xml(
        r#"<trans-unit id="greeting"><source>Hello</source><target>Hallo</target></trans-unit><trans-unit id="missing"><source>Missing</source><target>Fehlt</target></trans-unit>"#,
    );
    let outcome = execute(&store, &document, false).unwrap();
    assert_eq!(outcome.result.report.accepted, 0);
    assert!(outcome.result.report.accepted_destinations.is_empty());
    assert_eq!(outcome.result.report.rejected.len(), 1);
    assert_eq!(outcome.result.report.rejected[0].code, "unknown_key");
    assert_eq!(outcome.result.report.rejected[0].unit_id, "missing");
    assert!(!outcome.result.written);
    assert!(outcome.updated_file.is_none());
    assert_eq!(store.contents(), CATALOG.as_bytes());
}

#[test]
fn concurrent_edit_survives_a_failed_import_without_staged_cache_content() {
    let store = Store::new(CATALOG.as_bytes(), WriteMode::ConcurrentEdit);
    let error = execute(&store, &draft(), false).unwrap_err();
    assert!(matches!(
        error,
        XcStringsError::ConditionalWriteConflict {
            expected_exists: true,
            actual_exists: true,
            ..
        }
    ));
    assert_eq!(store.contents(), EXTERNAL_EDIT);
}

#[test]
fn store_without_conditional_writes_fails_closed() {
    let store = Store::new(CATALOG.as_bytes(), WriteMode::Unsupported);
    let error = execute(&store, &draft(), false).unwrap_err();
    assert!(matches!(
        error,
        XcStringsError::ConditionalWriteUnsupported { .. }
    ));
    assert_eq!(store.contents(), CATALOG.as_bytes());
}

#[test]
fn missing_target_is_a_byte_preserving_no_op() {
    let store = Store::new(CATALOG.as_bytes(), WriteMode::Unsupported);
    let outcome = execute(
        &store,
        &xml(r#"<trans-unit id="greeting"><source>Hello</source></trans-unit>"#),
        false,
    )
    .unwrap();
    assert_eq!(
        (
            outcome.result.report.accepted,
            outcome.result.report.missing_targets
        ),
        (0, 1)
    );
    assert!(!outcome.result.written);
    assert!(outcome.updated_file.is_none());
    assert_eq!(store.contents(), CATALOG.as_bytes());
}

#[test]
fn bom_is_part_of_expected_bytes_but_not_written() {
    let bytes = format!("\u{feff}{CATALOG}").into_bytes();
    let store = Store::new(&bytes, WriteMode::Normal);
    let outcome = execute(&store, &draft(), false).unwrap();
    assert!(outcome.result.written);
    assert!(!store.contents().starts_with(&[0xef, 0xbb, 0xbf]));
    assert_eq!(
        serde_json::from_slice::<Value>(&store.contents()).unwrap()["strings"]["greeting"]["localizations"]
            ["de"]["stringUnit"]["state"],
        "needs_review"
    );
}

#[test]
fn invalid_utf8_catalog_returns_io_error_and_keeps_bytes() {
    let store = Store::new(&[0xff, 0xfe], WriteMode::Normal);
    let error = execute(&store, &draft(), false).unwrap_err();
    assert!(
        matches!(error, XcStringsError::Io(ref error) if error.kind() == std::io::ErrorKind::InvalidData)
    );
    assert_eq!(store.contents(), [0xff, 0xfe]);
}
