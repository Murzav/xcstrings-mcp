use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;
use xcstrings_mcp::service::parser;

const NS: &str = "urn:oasis:names:tc:xliff:document:1.2";
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

fn cmd() -> Command {
    Command::cargo_bin("xcstrings-mcp").unwrap()
}

fn catalog_copy(temp: &TempDir) -> PathBuf {
    let destination = temp.path().join("catalog.xcstrings");
    fs::write(&destination, FORMAT_FIXTURE).unwrap();
    destination
}

fn document(contents: &str) -> String {
    format!(r#"<xliff xmlns="{NS}" version="1.2">{contents}</xliff>"#)
}

fn mixed_cdata_document() -> String {
    document(
        r#"<file target-language="de"><body><trans-unit id="greeting"><source>Hello %@</source><target>Start <![CDATA[%@ <b>&raw]]> + &amp; end</target></trans-unit></body></file>"#,
    )
}

fn run_import(catalog: &Path, input: &Path, dry_run: bool) -> std::process::Output {
    let mut command = cmd();
    command.args([
        "--json",
        "import",
        catalog.to_str().unwrap(),
        "--xliff",
        input.to_str().unwrap(),
    ]);
    if dry_run {
        command.arg("--dry-run");
    }
    command.output().unwrap()
}

#[test]
fn cli_cdata_dry_run_preserves_full_value_without_write() {
    let temp = TempDir::new().unwrap();
    let catalog = catalog_copy(&temp);
    let input = temp.path().join("mixed-cdata.xliff");
    fs::write(&input, mixed_cdata_document()).unwrap();
    let before = fs::read(&catalog).unwrap();

    let output = run_import(&catalog, &input, true);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::json!({
            "accepted": 1,
            "accepted_keys": ["greeting"],
            "rejected": [],
            "dry_run": true
        })
    );
    assert!(output.stderr.is_empty());
    assert_eq!(fs::read(&catalog).unwrap(), before);
}

#[test]
fn cli_cdata_apply_writes_full_value() {
    let temp = TempDir::new().unwrap();
    let catalog = catalog_copy(&temp);
    let input = temp.path().join("mixed-cdata.xliff");
    fs::write(&input, mixed_cdata_document()).unwrap();

    let output = run_import(&catalog, &input, false);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::json!({
            "accepted": 1,
            "accepted_keys": ["greeting"],
            "rejected": [],
            "dry_run": false
        })
    );
    assert!(output.stderr.is_empty());
    let parsed = parser::parse(&fs::read_to_string(&catalog).unwrap()).unwrap();
    let unit = parsed.strings["greeting"].localizations.as_ref().unwrap()["de"]
        .string_unit
        .as_ref()
        .unwrap();
    assert_eq!(unit.value, MIXED_TARGET);
}

fn assert_parse_error_without_write(contents: &str, expected: &str, wrap: bool) {
    let temp = TempDir::new().unwrap();
    let catalog = catalog_copy(&temp);
    let input = temp.path().join("invalid-cdata.xliff");
    let xliff = if wrap {
        document(contents)
    } else {
        contents.to_string()
    };
    fs::write(&input, xliff).unwrap();
    let before = fs::read(&catalog).unwrap();

    let output = run_import(&catalog, &input, false);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!("error: XLIFF parse error: {expected}\n")
    );
    assert_eq!(fs::read(&catalog).unwrap(), before);
}

#[test]
fn cli_rejects_cdata_outside_root_without_write() {
    let xliff = format!(
        r#"<![CDATA[outside]]><xliff xmlns="{NS}" version="1.2"><file target-language="de"><body/></file></xliff>"#
    );
    assert_parse_error_without_write(
        &xliff,
        "CDATA is not allowed outside <xliff> document root",
        false,
    );
}

#[test]
fn cli_rejects_unclosed_cdata_without_write() {
    assert_parse_error_without_write(
        r#"<file target-language="de"><body><trans-unit id="greeting"><source>Hello</source><target><![CDATA[unterminated</target></trans-unit></body></file>"#,
        "syntax error: CDATA not closed: `]]>` not found before end of input",
        true,
    );
}
