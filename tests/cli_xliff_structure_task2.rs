use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

const NS: &str = "urn:oasis:names:tc:xliff:document:1.2";

fn cmd() -> Command {
    Command::cargo_bin("xcstrings-mcp").unwrap()
}

fn catalog_copy(temp: &TempDir) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/simple.xcstrings");
    let destination = temp.path().join("catalog.xcstrings");
    fs::copy(source, &destination).unwrap();
    destination
}

fn assert_rejected_without_write(name: &str, contents: &str, expected: &str) {
    let temp = TempDir::new().unwrap();
    let catalog = catalog_copy(&temp);
    let input = temp.path().join(name);
    let xml = format!(r#"<xliff xmlns="{NS}" version="1.2">{contents}</xliff>"#);
    fs::write(&input, xml).unwrap();
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
        .stderr(predicate::eq(format!(
            "error: XLIFF parse error: {expected}\n"
        )));

    assert_eq!(fs::read(&catalog).unwrap(), before);
}

macro_rules! malformed_cli_case {
    ($name:ident, $file_name:literal, $contents:expr, $error:literal) => {
        #[test]
        fn $name() {
            assert_rejected_without_write($file_name, $contents, $error);
        }
    };
}

malformed_cli_case!(
    cli_rejects_trans_unit_before_file_without_write,
    "trans-unit-before-file.xliff",
    r#"<trans-unit id="greeting"><source>Hello</source><target>Injected</target></trans-unit>
<file target-language="de"><body/></file>"#,
    "element <trans-unit> is not allowed as a child of <xliff>; expected <file>"
);

malformed_cli_case!(
    cli_rejects_file_without_body_without_write,
    "no-body.xliff",
    r#"<file target-language="de"><trans-unit id="greeting"><source>Hello</source><target>Injected</target></trans-unit></file>"#,
    "element <trans-unit> is not allowed as a child of <file>"
);

malformed_cli_case!(
    cli_rejects_target_before_source_without_write,
    "target-before-source.xliff",
    r#"<file target-language="de"><body><trans-unit id="greeting"><target>Injected</target><source>Hello</source></trans-unit></body></file>"#,
    "element <target> must follow <source> inside <trans-unit>"
);

malformed_cli_case!(
    cli_rejects_duplicate_target_without_write,
    "duplicate-target.xliff",
    r#"<file target-language="de"><body><trans-unit id="greeting"><source>Hello</source><target>First</target><target>Second</target></trans-unit></body></file>"#,
    "element <trans-unit> may contain at most one <target> child"
);

malformed_cli_case!(
    cli_rejects_nested_file_without_write,
    "nested-file.xliff",
    r#"<file target-language="de"><file target-language="fr"><body/></file><body/></file>"#,
    "element <file> is not allowed as a child of <file>"
);

malformed_cli_case!(
    cli_rejects_bin_target_nested_in_header_without_write,
    "bin-target-in-header.xliff",
    r#"<file target-language="de"><header><bin-target><external-file href="binary.dat"/></bin-target></header>
<body><trans-unit id="greeting"><source>Hello</source><target>Injected</target></trans-unit></body></file>"#,
    "element <bin-target> is not allowed as a child of <header>"
);

malformed_cli_case!(
    cli_rejects_bin_unit_without_bin_source_without_write,
    "bin-unit-without-bin-source.xliff",
    r#"<file target-language="de"><body><bin-unit/></body></file>"#,
    "element <bin-unit> is missing required <bin-source> child"
);

malformed_cli_case!(
    cli_rejects_group_metadata_out_of_order_without_write,
    "group-metadata-order.xliff",
    r#"<file target-language="de"><body><group><note>late context</note>
<context-group><context>screen</context></context-group>
<trans-unit id="greeting"><source>Hello</source><target>Injected</target></trans-unit>
</group></body></file>"#,
    "element <context-group> is out of order inside <group>"
);
