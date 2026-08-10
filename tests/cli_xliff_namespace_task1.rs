use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

const XLIFF_NAMESPACE: &str = "urn:oasis:names:tc:xliff:document:1.2";

fn cmd() -> Command {
    Command::cargo_bin("xcstrings-mcp").unwrap()
}

fn catalog_copy(temp: &TempDir) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/simple.xcstrings");
    let destination = temp.path().join("catalog.xcstrings");
    fs::copy(source, &destination).unwrap();
    destination
}

fn prefixed_xliff(namespace: &str, target: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ns0:xliff xmlns:ns0="{namespace}" version="1.2">
  <ns0:file source-language="en" target-language="de" original="catalog.xcstrings" datatype="plaintext">
    <ns0:body><ns0:trans-unit id="greeting"><ns0:source>Hello</ns0:source>
      <ns0:target state="translated">{target}</ns0:target>
    </ns0:trans-unit></ns0:body>
  </ns0:file>
</ns0:xliff>"#
    )
}

fn assert_cli_rejected_without_writing(file_name: &str, xliff: &str, expected_error: &str) {
    let temp = TempDir::new().unwrap();
    let catalog = catalog_copy(&temp);
    let input = temp.path().join(file_name);
    fs::write(&input, xliff).unwrap();
    let before = fs::read(&catalog).unwrap();

    cmd()
        .args([
            "--json",
            "import",
            catalog.to_str().unwrap(),
            "--xliff",
            input.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::eq(format!("error: {expected_error}\n")));

    assert_eq!(fs::read(&catalog).unwrap(), before);
}

#[test]
fn cli_import_accepts_prefix_bound_xliff_for_dry_run_and_apply() {
    let temp = TempDir::new().unwrap();
    let catalog = catalog_copy(&temp);
    let input = temp.path().join("prefixed.xliff");
    let exported = temp.path().join("exported.xliff");
    fs::write(&input, prefixed_xliff(XLIFF_NAMESPACE, "Namespace Hallo")).unwrap();
    let before = fs::read(&catalog).unwrap();

    let dry_run = cmd()
        .args([
            "--json",
            "import",
            catalog.to_str().unwrap(),
            "--xliff",
            input.to_str().unwrap(),
            "--dry-run",
        ])
        .assert()
        .success();
    let dry_json: serde_json::Value = serde_json::from_slice(&dry_run.get_output().stdout).unwrap();

    assert_eq!(dry_json["accepted"], 1);
    assert_eq!(dry_json["accepted_keys"], serde_json::json!(["greeting"]));
    assert_eq!(dry_json["rejected"], serde_json::json!([]));
    assert_eq!(dry_json["dry_run"], true);
    assert!(dry_json["warnings"].is_null());
    assert_eq!(fs::read(&catalog).unwrap(), before);

    let applied = cmd()
        .args([
            "--json",
            "import",
            catalog.to_str().unwrap(),
            "--xliff",
            input.to_str().unwrap(),
        ])
        .assert()
        .success();
    let apply_json: serde_json::Value =
        serde_json::from_slice(&applied.get_output().stdout).unwrap();

    assert_eq!(apply_json["accepted"], 1);
    assert_eq!(apply_json["accepted_keys"], serde_json::json!(["greeting"]));
    assert_eq!(apply_json["rejected"], serde_json::json!([]));
    assert_eq!(apply_json["dry_run"], false);
    assert!(apply_json["warnings"].is_null());

    cmd()
        .args([
            "export",
            catalog.to_str().unwrap(),
            "--locale",
            "de",
            "--all",
            "--output",
            exported.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert!(
        fs::read_to_string(exported)
            .unwrap()
            .contains("Namespace Hallo")
    );
}

#[test]
fn cli_import_rejects_wrong_namespace_without_writing() {
    let temp = TempDir::new().unwrap();
    let catalog = catalog_copy(&temp);
    let input = temp.path().join("wrong-namespace.xliff");
    fs::write(
        &input,
        prefixed_xliff("urn:example:wrong", "Namespace Hallo"),
    )
    .unwrap();
    let before = fs::read(&catalog).unwrap();

    cmd()
        .args([
            "--json",
            "import",
            catalog.to_str().unwrap(),
            "--xliff",
            input.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::eq(
            "error: XLIFF parse error: element <xliff> uses namespace 'urn:example:wrong'; expected 'urn:oasis:names:tc:xliff:document:1.2'\n",
        ));

    assert_eq!(fs::read(&catalog).unwrap(), before);
}

#[test]
fn cli_import_rejects_unqualified_child_in_official_document_without_writing() {
    let temp = TempDir::new().unwrap();
    let catalog = catalog_copy(&temp);
    let input = temp.path().join("official-root-unqualified-child.xliff");
    fs::write(
        &input,
        format!(
            r#"<x:xliff xmlns:x="{XLIFF_NAMESPACE}" version="1.2">
  <file target-language="de"><body><trans-unit id="greeting"><source>Hello</source>
    <target>Mixed Hallo</target></trans-unit></body></file>
</x:xliff>"#
        ),
    )
    .unwrap();
    let before = fs::read(&catalog).unwrap();

    cmd()
        .args([
            "--json",
            "import",
            catalog.to_str().unwrap(),
            "--xliff",
            input.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::eq(
            "error: XLIFF parse error: element <file> is unqualified in namespace-qualified XLIFF document; expected 'urn:oasis:names:tc:xliff:document:1.2'\n",
        ));

    assert_eq!(fs::read(&catalog).unwrap(), before);
}

#[test]
fn cli_import_rejects_qualified_child_in_legacy_document_without_writing() {
    let temp = TempDir::new().unwrap();
    let catalog = catalog_copy(&temp);
    let input = temp.path().join("legacy-root-qualified-child.xliff");
    fs::write(
        &input,
        format!(
            r#"<xliff xmlns:x="{XLIFF_NAMESPACE}" version="1.2">
  <x:file target-language="de"><x:body><x:trans-unit id="greeting"><x:source>Hello</x:source>
    <x:target>Mixed Hallo</x:target></x:trans-unit></x:body></x:file>
</xliff>"#
        ),
    )
    .unwrap();
    let before = fs::read(&catalog).unwrap();

    cmd()
        .args([
            "--json",
            "import",
            catalog.to_str().unwrap(),
            "--xliff",
            input.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::eq(
            "error: XLIFF parse error: element <file> uses namespace 'urn:oasis:names:tc:xliff:document:1.2' in legacy unqualified XLIFF document; expected no namespace\n",
        ));

    assert_eq!(fs::read(&catalog).unwrap(), before);
}

#[test]
fn cli_import_rejects_duplicate_namespace_without_writing() {
    let temp = TempDir::new().unwrap();
    let catalog = catalog_copy(&temp);
    let input = temp.path().join("duplicate-namespace.xliff");
    fs::write(
        &input,
        format!(
            r#"<x:xliff xmlns:x="urn:example:wrong" xmlns:x="{XLIFF_NAMESPACE}" version="1.2">
  <x:file target-language="de"/>
</x:xliff>"#
        ),
    )
    .unwrap();
    let before = fs::read(&catalog).unwrap();

    cmd()
        .args([
            "--json",
            "import",
            catalog.to_str().unwrap(),
            "--xliff",
            input.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::eq(
            "error: XLIFF parse error: duplicate attribute on <xliff>\n",
        ));

    assert_eq!(fs::read(&catalog).unwrap(), before);
}

#[test]
fn cli_import_rejects_foreign_wrapper_without_writing() {
    assert_cli_rejected_without_writing(
        "foreign-wrapper.xliff",
        r#"<wrapper xmlns:x="urn:oasis:names:tc:xliff:document:1.2"><x:xliff version="1.2">
  <x:file target-language="de"><x:body><x:trans-unit id="greeting"><x:source>Hello</x:source>
    <x:target>Wrapped</x:target></x:trans-unit></x:body></x:file>
</x:xliff></wrapper>"#,
        "XLIFF parse error: document root must be <xliff>; found <wrapper>",
    );
}

#[test]
fn cli_import_rejects_nested_xliff_without_writing() {
    assert_cli_rejected_without_writing(
        "nested-xliff.xliff",
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" version="1.2"><x:xliff version="1.2">
  <x:file target-language="de"><x:body><x:trans-unit id="greeting"><x:source>Hello</x:source>
    <x:target>Nested</x:target></x:trans-unit></x:body></x:file>
</x:xliff></x:xliff>"#,
        "XLIFF parse error: nested <xliff> element is not allowed",
    );
}

#[test]
fn cli_import_rejects_multiple_roots_without_writing() {
    assert_cli_rejected_without_writing(
        "multiple-roots.xliff",
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" version="1.2"/>
<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" version="1.2"><x:file target-language="de"/></x:xliff>"#,
        "XLIFF parse error: element <xliff> appears after </xliff> document root",
    );
}

#[test]
fn cli_import_rejects_empty_root_fragment_without_writing() {
    assert_cli_rejected_without_writing(
        "empty-root-fragment.xliff",
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" version="1.2"/>
<x:file xmlns:x="urn:oasis:names:tc:xliff:document:1.2" target-language="de"/>"#,
        "XLIFF parse error: element <file> appears after </xliff> document root",
    );
}

#[test]
fn cli_import_rejects_text_outside_root_without_writing() {
    assert_cli_rejected_without_writing(
        "text-outside-root.xliff",
        r#"unexpected<xliff version="1.2"><file target-language="de"/></xliff>"#,
        "XLIFF parse error: non-whitespace text is not allowed outside <xliff> document root",
    );
}

#[test]
fn cli_import_rejects_missing_root_without_writing() {
    assert_cli_rejected_without_writing(
        "missing-root.xliff",
        "\n<!-- no document element -->\n",
        "XLIFF parse error: missing <xliff> document root",
    );
}

#[test]
fn cli_import_rejects_duplicate_expanded_attribute_without_writing() {
    assert_cli_rejected_without_writing(
        "duplicate-expanded-attribute.xliff",
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" xmlns:a="urn:attr" xmlns:b="urn:attr" version="1.2">
  <x:file target-language="de"><x:body a:custom="first" b:custom="last"/></x:file>
</x:xliff>"#,
        "XLIFF parse error: duplicate expanded attribute '{urn:attr}custom' on <body>",
    );
}

#[test]
fn cli_import_rejects_unbound_attribute_prefix_without_writing() {
    assert_cli_rejected_without_writing(
        "unbound-attribute-prefix.xliff",
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
  <x:file target-language="de"><x:body missing:custom="value"/></x:file>
</x:xliff>"#,
        "XLIFF parse error: attribute <missing:custom> on <body> uses unbound namespace prefix 'missing'",
    );
}

#[test]
fn cli_import_rejects_unbound_non_empty_extension_without_writing() {
    assert_cli_rejected_without_writing(
        "unbound-non-empty-extension.xliff",
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
  <missing:group><x:file target-language="de"><x:body><x:trans-unit id="greeting">
    <x:source>Hello</x:source><x:target>Unbound extension</x:target>
  </x:trans-unit></x:body></x:file></missing:group>
</x:xliff>"#,
        "XLIFF parse error: element <group> uses unbound namespace prefix 'missing'",
    );
}

#[test]
fn cli_import_rejects_unbound_empty_extension_without_writing() {
    assert_cli_rejected_without_writing(
        "unbound-empty-extension.xliff",
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
  <missing:marker/><x:file target-language="de"/>
</x:xliff>"#,
        "XLIFF parse error: element <marker> uses unbound namespace prefix 'missing'",
    );
}

#[test]
fn cli_import_accepts_normalized_official_namespace_and_bound_extension() {
    let temp = TempDir::new().unwrap();
    let catalog = catalog_copy(&temp);
    let input = temp.path().join("normalized-official.xliff");
    let exported = temp.path().join("normalized-exported.xliff");
    fs::write(
        &input,
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.&#50;" xmlns:ext="urn:example:extension" version="1.2">
  <ext:group><x:file target-language="de"><x:body><x:trans-unit id="greeting">
    <x:source>Hello</x:source><x:target>Normalized namespace</x:target>
  </x:trans-unit></x:body></x:file></ext:group>
</x:xliff>"#,
    )
    .unwrap();
    let before = fs::read(&catalog).unwrap();

    let dry_run = cmd()
        .args([
            "--json",
            "import",
            catalog.to_str().unwrap(),
            "--xliff",
            input.to_str().unwrap(),
            "--dry-run",
        ])
        .assert()
        .success();
    let dry_json: serde_json::Value = serde_json::from_slice(&dry_run.get_output().stdout).unwrap();
    assert_eq!(dry_json["accepted"], 1);
    assert_eq!(dry_json["accepted_keys"], serde_json::json!(["greeting"]));
    assert_eq!(dry_json["rejected"], serde_json::json!([]));
    assert_eq!(dry_json["dry_run"], true);
    assert!(dry_json["warnings"].is_null());
    assert_eq!(fs::read(&catalog).unwrap(), before);

    let applied = cmd()
        .args([
            "--json",
            "import",
            catalog.to_str().unwrap(),
            "--xliff",
            input.to_str().unwrap(),
        ])
        .assert()
        .success();
    let apply_json: serde_json::Value =
        serde_json::from_slice(&applied.get_output().stdout).unwrap();
    assert_eq!(apply_json["accepted"], 1);
    assert_eq!(apply_json["accepted_keys"], serde_json::json!(["greeting"]));
    assert_eq!(apply_json["rejected"], serde_json::json!([]));
    assert_eq!(apply_json["dry_run"], false);
    assert!(apply_json["warnings"].is_null());

    cmd()
        .args([
            "export",
            catalog.to_str().unwrap(),
            "--locale",
            "de",
            "--all",
            "--output",
            exported.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert!(
        fs::read_to_string(exported)
            .unwrap()
            .contains("Normalized namespace")
    );
}

#[test]
fn cli_import_rejects_normalized_alias_collision_without_writing() {
    assert_cli_rejected_without_writing(
        "normalized-alias-collision.xliff",
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" xmlns:a="urn:attr" xmlns:b="urn:&#97;ttr" version="1.2">
  <x:file target-language="de"><x:body a:custom="first" b:custom="last"/></x:file>
</x:xliff>"#,
        "XLIFF parse error: duplicate expanded attribute '{urn:attr}custom' on <body>",
    );
}

#[test]
fn cli_import_rejects_malformed_namespace_reference_without_writing() {
    assert_cli_rejected_without_writing(
        "malformed-namespace-reference.xliff",
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.&bogus;" version="1.2"><x:file target-language="de"/></x:xliff>"#,
        "XLIFF parse error: invalid XML namespace value on <xliff>",
    );
}
