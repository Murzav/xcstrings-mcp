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
fn cli_export_excludes_variation_only_entries_from_file_and_count() {
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
    assert_eq!(report["exported_count"], 1);
    assert!(output.stderr.is_empty());
    let xliff = fs::read_to_string(output_path).unwrap();
    assert!(xliff.contains(r#"<trans-unit id="simple_key">"#));
    assert!(!xliff.contains(r#"<trans-unit id="days_remaining">"#));
    assert!(!xliff.contains(r#"<trans-unit id="items_count">"#));
    assert!(!xliff.contains(r#"<trans-unit id="photos_count">"#));
}

#[test]
fn cli_accepts_real_xcode_empty_id_empty_target_without_write() {
    let temp = TempDir::new().unwrap();
    let catalog = simple_catalog(&temp);
    let input =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/xcode_26_6_empty_id.xliff");
    let before = fs::read(&catalog).unwrap();

    let output = run_import(&catalog, &input);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::json!({"accepted": 0, "rejected": [], "dry_run": false})
    );
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
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::json!({
            "accepted": 0,
            "rejected": [{"key": "", "reason": "key not found in file"}],
            "dry_run": false
        })
    );
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
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::json!({
            "accepted": 1,
            "accepted_keys": [""],
            "rejected": [],
            "dry_run": false
        })
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
    assert_parse_failure_without_write(
        r#"<file target-language="uk"><body><trans-unit id="days_remaining|==|plural.one"><source>%lld day</source><target>%lld day left</target></trans-unit></body></file>"#,
        "Apple XLIFF variation unit id 'days_remaining|==|plural.one' is unsupported; import simple stringUnit ids only",
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
    assert_parse_failure_without_write(
        r#"<file target-language="de"><body><trans-unit id="greeting"><source>Hello</source><target>Hallo</target></trans-unit></body></file>
<file target-language="de"><body><trans-unit id="greeting"><source>Hello again</source><target>Guten Tag</target></trans-unit></body></file>"#,
        "XLIFF unit id 'greeting' is repeated across <file> elements and cannot be flattened safely",
    );
}
