use std::fs;

use assert_cmd::Command;
use tempfile::TempDir;
use xcstrings_mcp::{
    XcStringsError, io::fs::FsFileStore, xliff_operation::resolve_export_destination,
};

const INPUT: &str = include_str!("fixtures/simple.xcstrings");

#[test]
fn cli_export_refuses_to_overwrite_its_source_catalog() {
    let directory = TempDir::new().unwrap();
    let source = directory.path().join("Localizable.xcstrings");
    fs::write(&source, INPUT).unwrap();

    let output = Command::cargo_bin("xcstrings-mcp")
        .unwrap()
        .args([
            "export",
            source.to_str().unwrap(),
            "--locale",
            "de",
            "--all",
            "--output",
            source.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("output file must have .xliff or .xlf extension")
    );
    assert_eq!(fs::read(&source).unwrap(), INPUT.as_bytes());
}

#[cfg(unix)]
#[test]
fn output_xml_alias_to_the_source_is_rejected_without_writing() {
    let directory = TempDir::new().unwrap();
    let source = directory.path().join("Localizable.xcstrings");
    let alias = directory.path().join("export.xliff");
    fs::write(&source, INPUT).unwrap();
    std::os::unix::fs::symlink(&source, &alias).unwrap();

    let error = resolve_export_destination(&FsFileStore::new(), &source, &alias).unwrap_err();

    assert!(
        matches!(error, XcStringsError::InvalidPath { reason, .. } if reason == "XLIFF output must not overwrite the source catalog")
    );
    assert_eq!(fs::read(&source).unwrap(), INPUT.as_bytes());
    assert_eq!(fs::read_link(&alias).unwrap(), source);
}

#[cfg(unix)]
#[test]
fn output_xml_alias_cannot_destroy_another_catalog_either() {
    let directory = TempDir::new().unwrap();
    let source = directory.path().join("Localizable.xcstrings");
    let other = directory.path().join("Other.xcstrings");
    let alias = directory.path().join("export.xliff");
    fs::write(&source, INPUT).unwrap();
    fs::write(&other, "another catalog's bytes").unwrap();
    std::os::unix::fs::symlink(&other, &alias).unwrap();

    let error = resolve_export_destination(&FsFileStore::new(), &source, &alias).unwrap_err();

    assert!(
        matches!(error, XcStringsError::InvalidPath { reason, .. } if reason == "output file must have .xliff or .xlf extension")
    );
    assert_eq!(fs::read(&source).unwrap(), INPUT.as_bytes());
    assert_eq!(fs::read(&other).unwrap(), b"another catalog's bytes");
}

#[test]
fn regular_xml_destination_resolves_without_creating_it() {
    let directory = TempDir::new().unwrap();
    let source = directory.path().join("Localizable.xcstrings");
    let output = directory.path().join("export.xlf");
    fs::write(&source, INPUT).unwrap();

    let resolved = resolve_export_destination(&FsFileStore::new(), &source, &output).unwrap();

    assert_eq!(
        resolved,
        fs::canonicalize(directory.path())
            .unwrap()
            .join("export.xlf")
    );
    assert!(!output.exists());
    assert_eq!(fs::read(&source).unwrap(), INPUT.as_bytes());
}
