use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;
use xcstrings_mcp::service::parser;

const NS: &str = "urn:oasis:names:tc:xliff:document:1.2";
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

fn cmd() -> Command {
    Command::cargo_bin("xcstrings-mcp").unwrap()
}

fn simple_catalog(temp: &TempDir) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/simple.xcstrings");
    let destination = temp.path().join("catalog.xcstrings");
    fs::copy(source, &destination).unwrap();
    destination
}

fn document(contents: &str) -> String {
    format!(r#"<xliff xmlns="{NS}" version="1.2">{contents}</xliff>"#)
}

fn run_import(catalog: &Path, input: &Path) -> std::process::Output {
    cmd()
        .args([
            "--json",
            "import",
            catalog.to_str().unwrap(),
            "--xliff",
            input.to_str().unwrap(),
        ])
        .output()
        .unwrap()
}

#[test]
fn cli_export_includes_every_plural_leaf_and_required_category() {
    let temp = TempDir::new().unwrap();
    let catalog =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/with_plurals.xcstrings");
    let output_path = temp.path().join("output.xliff");

    let output = cmd()
        .args([
            "--json",
            "export",
            catalog.to_str().unwrap(),
            "--locale",
            "uk",
            "--output",
            output_path.to_str().unwrap(),
            "--all",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["exported_count"], 13);
    assert!(output.stderr.is_empty());
    let xml = fs::read_to_string(output_path).unwrap();
    let doc = xcstrings_mcp::service::xliff::parse_document(&xml).unwrap();
    assert_eq!(
        doc.files[0]
            .units
            .iter()
            .map(|u| u.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "days_remaining|==|plural.few",
            "days_remaining|==|plural.many",
            "days_remaining|==|plural.one",
            "days_remaining|==|plural.other",
            "items_count|==|plural.few",
            "items_count|==|plural.many",
            "items_count|==|plural.one",
            "items_count|==|plural.other",
            "photos_count|==|plural.few",
            "photos_count|==|plural.many",
            "photos_count|==|plural.one",
            "photos_count|==|plural.other",
            "simple_key"
        ]
    );
}

#[test]
fn cli_rejects_real_xcode_empty_id_when_catalog_key_is_absent() {
    let temp = TempDir::new().unwrap();
    let catalog = simple_catalog(&temp);
    let input =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/xcode_26_6_empty_id.xliff");
    let before = fs::read(&catalog).unwrap();

    let output = run_import(&catalog, &input);

    assert_eq!(output.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_rejected(&report, "unknown_key", "", "no catalog destination for ''");
    assert!(output.stderr.is_empty());
    assert_eq!(fs::read(&catalog).unwrap(), before);
}

#[test]
fn cli_rejects_nonempty_empty_id_when_catalog_has_no_empty_key_without_write() {
    let temp = TempDir::new().unwrap();
    let catalog = simple_catalog(&temp);
    let input = temp.path().join("empty-id-nonempty-target.xliff");
    fs::write(
        &input,
        document(
            r#"<file target-language="de"><body><trans-unit id=""><source></source><target>Leerzeichenlos</target></trans-unit></body></file>"#,
        ),
    )
    .unwrap();
    let before = fs::read(&catalog).unwrap();

    let output = run_import(&catalog, &input);

    assert_eq!(output.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_rejected(&report, "unknown_key", "", "no catalog destination for ''");
    assert!(output.stderr.is_empty());
    assert_eq!(fs::read(&catalog).unwrap(), before);
}

#[test]
fn cli_writes_nonempty_empty_id_only_when_catalog_has_exact_empty_key() {
    let temp = TempDir::new().unwrap();
    let catalog = temp.path().join("catalog.xcstrings");
    fs::write(&catalog, EMPTY_KEY_FIXTURE).unwrap();
    let input = temp.path().join("empty-id-existing-key.xliff");
    fs::write(
        &input,
        document(
            r#"<file target-language="de"><body><trans-unit id=""><source></source><target>Leerzeichenlos</target></trans-unit></body></file>"#,
        ),
    )
    .unwrap();

    let output = run_import(&catalog, &input);

    assert_eq!(output.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["accepted"], 1);
    assert_eq!(report["written"], true);
    assert_eq!(report["accepted_keys"], serde_json::json!([""]));
    assert_eq!(report["rejected"], serde_json::json!([]));
    assert_eq!(
        report["accepted_destinations"],
        serde_json::json!([{"original":"","key":"","locale":"de","path":[],"unit_id":""}])
    );
    assert!(output.stderr.is_empty());
    let parsed = parser::parse(&fs::read_to_string(&catalog).unwrap()).unwrap();
    let unit = parsed.strings[""].localizations.as_ref().unwrap()["de"]
        .string_unit
        .as_ref()
        .unwrap();
    assert_eq!(unit.value, "Leerzeichenlos");
}

fn assert_parse_failure_without_write(contents: &str, expected: &str) {
    let temp = TempDir::new().unwrap();
    let catalog = simple_catalog(&temp);
    let input = temp.path().join("invalid.xliff");
    fs::write(&input, document(contents)).unwrap();
    let before = fs::read(&catalog).unwrap();

    let output = run_import(&catalog, &input);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!("error: XLIFF parse error: {expected}\n")
    );
    assert_eq!(fs::read(&catalog).unwrap(), before);
}

#[test]
fn cli_rejects_variation_id_without_write() {
    assert_semantic_failure_without_write(
        r#"<file target-language="uk"><body><trans-unit id="days_remaining|==|plural.one"><source>%lld day</source><target>%lld day left</target></trans-unit></body></file>"#,
        "unknown_key",
        "days_remaining|==|plural.one",
        "no catalog destination for 'days_remaining|==|plural.one'",
    );
}

#[test]
fn cli_rejects_duplicate_id_in_one_file_without_write() {
    assert_parse_failure_without_write(
        r#"<file target-language="de"><body>
<trans-unit id="greeting"><source>Hello</source><target>Hallo</target></trans-unit>
<trans-unit id="greeting"><source>Hello again</source><target>Guten Tag</target></trans-unit>
</body></file>"#,
        "duplicate XLIFF unit id 'greeting' inside <file>",
    );
}

#[test]
fn cli_rejects_duplicate_id_across_files_without_write() {
    assert_semantic_failure_without_write(
        r#"<file target-language="de"><body><trans-unit id="greeting"><source>Hello</source><target>Hallo</target></trans-unit></body></file>
<file target-language="de"><body><trans-unit id="greeting"><source>Hello again</source><target>Guten Tag</target></trans-unit></body></file>"#,
        "duplicate_destination",
        "greeting",
        "multiple units address the same catalog leaf",
    );
}

fn assert_rejected(result: &serde_json::Value, code: &str, id: &str, message: &str) {
    assert_eq!(result["accepted"], 0);
    assert_eq!(result["accepted_destinations"], serde_json::json!([]));
    assert_eq!(result["written"], false);
    assert_eq!(result["rejected"].as_array().unwrap().len(), 1);
    assert_eq!(result["rejected"][0]["code"], code);
    assert_eq!(result["rejected"][0]["unit_id"], id);
    assert_eq!(result["rejected"][0]["message"], message);
}

fn assert_semantic_failure_without_write(contents: &str, code: &str, id: &str, message: &str) {
    let temp = TempDir::new().unwrap();
    let catalog = simple_catalog(&temp);
    let input = temp.path().join("invalid.xliff");
    fs::write(&input, document(contents)).unwrap();
    let before = fs::read(&catalog).unwrap();
    let output = run_import(&catalog, &input);
    assert_eq!(output.status.code(), Some(2));
    assert_rejected(
        &serde_json::from_slice(&output.stdout).unwrap(),
        code,
        id,
        message,
    );
    assert!(output.stderr.is_empty());
    assert_eq!(fs::read(&catalog).unwrap(), before);
}
