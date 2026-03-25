use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

const EXIT_VALIDATION_ISSUES: i32 = 2;

fn cmd() -> Command {
    Command::cargo_bin("xcstrings-mcp").unwrap()
}

fn fixture_path(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// Copy a fixture to tmpdir for mutation tests.
fn copy_fixture_to_tmp(fixture: &str, tmp: &TempDir) -> std::path::PathBuf {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(fixture);
    let dst = tmp.path().join(fixture);
    fs::copy(&src, &dst).unwrap();
    dst
}

// ══════════════════════════════════════════════════════════════
// Backward Compatibility
// ══════════════════════════════════════════════════════════════

#[test]
fn version_flag() {
    cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("xcstrings-mcp"));
}

#[test]
fn help_flag() {
    cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("coverage"));
}

// ══════════════════════════════════════════════════════════════
// Info Command
// ══════════════════════════════════════════════════════════════

#[test]
fn info_with_fixture() {
    cmd()
        .args(["info", &fixture_path("simple.xcstrings")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Source language: en"));
}

#[test]
fn info_json() {
    let output = cmd()
        .args(["--json", "info", &fixture_path("simple.xcstrings")])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(parsed["source_language"], "en");
}

#[test]
fn info_nonexistent_file() {
    cmd()
        .args(["info", "/tmp/nonexistent_file_12345.xcstrings"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

// ══════════════════════════════════════════════════════════════
// Coverage Command
// ══════════════════════════════════════════════════════════════

#[test]
fn coverage_with_fixture() {
    cmd()
        .args(["coverage", &fixture_path("simple.xcstrings")])
        .assert()
        .success()
        .stdout(predicate::str::contains("Locale"));
}

#[test]
fn coverage_json() {
    let output = cmd()
        .args(["--json", "coverage", &fixture_path("simple.xcstrings")])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(parsed["source_language"], "en");
    assert!(parsed["locales"].is_array());
}

#[test]
fn coverage_locale_filter() {
    cmd()
        .args([
            "coverage",
            &fixture_path("simple.xcstrings"),
            "--locale",
            "en",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("en"));
}

#[test]
fn coverage_locale_filter_json() {
    let output = cmd()
        .args([
            "--json",
            "coverage",
            &fixture_path("simple.xcstrings"),
            "--locale",
            "en",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let locales = parsed["locales"].as_array().expect("array");
    assert!(
        locales.iter().all(|l| l["locale"] == "en"),
        "should only contain en locale"
    );
}

#[test]
fn coverage_nonexistent_file() {
    cmd()
        .args(["coverage", "/tmp/nonexistent_file_12345.xcstrings"])
        .assert()
        .failure();
}

// ══════════════════════════════════════════════════════════════
// Validate Command
// ══════════════════════════════════════════════════════════════

#[test]
fn validate_clean_fixture() {
    cmd()
        .args(["validate", &fixture_path("simple.xcstrings")])
        .assert()
        .success()
        .stdout(predicate::str::contains("No validation issues"));
}

#[test]
fn validate_json() {
    let output = cmd()
        .args(["--json", "validate", &fixture_path("simple.xcstrings")])
        .assert();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(parsed.is_array());
}

#[test]
fn validate_nonexistent_file() {
    cmd()
        .args(["validate", "/tmp/nonexistent_file_12345.xcstrings"])
        .assert()
        .failure();
}

// ══════════════════════════════════════════════════════════════
// Search Command
// ══════════════════════════════════════════════════════════════

#[test]
fn search_matching_keys() {
    cmd()
        .args(["search", "greeting", &fixture_path("simple.xcstrings")])
        .assert()
        .success()
        .stdout(predicate::str::contains("greeting"));
}

#[test]
fn search_no_results() {
    cmd()
        .args([
            "search",
            "nonexistent_pattern_xyz_99999",
            &fixture_path("simple.xcstrings"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("No keys matching"));
}

#[test]
fn search_json() {
    let output = cmd()
        .args([
            "--json",
            "search",
            "greeting",
            &fixture_path("simple.xcstrings"),
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(parsed.is_array());
    assert!(!parsed.as_array().unwrap().is_empty());
}

#[test]
fn search_limit() {
    cmd()
        .args([
            "search",
            "e",
            &fixture_path("simple.xcstrings"),
            "--limit",
            "1",
        ])
        .assert()
        .success();
}

// ══════════════════════════════════════════════════════════════
// Stale Command
// ══════════════════════════════════════════════════════════════

#[test]
fn stale_finds_stale_keys() {
    cmd()
        .args(["stale", &fixture_path("with_stale.xcstrings")])
        .assert()
        .success()
        .stdout(predicate::str::contains("stale key"));
}

#[test]
fn stale_json() {
    let output = cmd()
        .args(["--json", "stale", &fixture_path("with_stale.xcstrings")])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(parsed.is_array());
    assert!(!parsed.as_array().unwrap().is_empty());
}

#[test]
fn stale_with_locale_and_limit() {
    cmd()
        .args([
            "stale",
            &fixture_path("with_stale.xcstrings"),
            "--locale",
            "en",
            "--limit",
            "5",
        ])
        .assert()
        .success();
}

#[test]
fn stale_clean_fixture() {
    cmd()
        .args(["stale", &fixture_path("simple.xcstrings")])
        .assert()
        .success()
        .stdout(predicate::str::contains("No stale keys"));
}

// ══════════════════════════════════════════════════════════════
// Add-locale Command
// ══════════════════════════════════════════════════════════════

#[test]
fn add_locale_success() {
    let tmp = TempDir::new().unwrap();
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);

    cmd()
        .args(["add-locale", "fr", dst.to_str().unwrap()])
        .assert()
        .success();

    let content = fs::read_to_string(&dst).unwrap();
    assert!(content.contains("\"fr\""), "file should contain fr locale");
}

#[test]
fn add_locale_dry_run() {
    let tmp = TempDir::new().unwrap();
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);
    let before = fs::read_to_string(&dst).unwrap();

    cmd()
        .args(["add-locale", "fr", dst.to_str().unwrap(), "--dry-run"])
        .assert()
        .success();

    let after = fs::read_to_string(&dst).unwrap();
    assert_eq!(before, after, "dry run should not change the file");
}

#[test]
fn add_locale_json() {
    let tmp = TempDir::new().unwrap();
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);

    let output = cmd()
        .args(["--json", "add-locale", "de", dst.to_str().unwrap()])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(parsed["locale"], "de");
    assert!(parsed["keys_initialized"].is_number());
}

// ══════════════════════════════════════════════════════════════
// Remove-locale Command
// ══════════════════════════════════════════════════════════════

#[test]
fn remove_locale_success() {
    let tmp = TempDir::new().unwrap();
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);

    // simple.xcstrings has "uk" locale
    cmd()
        .args(["remove-locale", "uk", dst.to_str().unwrap()])
        .assert()
        .success();

    let content = fs::read_to_string(&dst).unwrap();
    // uk locale entries should be removed
    assert!(
        !content.contains("\"uk\""),
        "file should no longer contain uk locale"
    );
}

#[test]
fn remove_locale_dry_run() {
    let tmp = TempDir::new().unwrap();
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);
    let before = fs::read_to_string(&dst).unwrap();

    cmd()
        .args(["remove-locale", "uk", dst.to_str().unwrap(), "--dry-run"])
        .assert()
        .success();

    let after = fs::read_to_string(&dst).unwrap();
    assert_eq!(before, after, "dry run should not change the file");
}

#[test]
fn remove_locale_source_language_error() {
    let tmp = TempDir::new().unwrap();
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);

    // Source language is "en", removing it should fail
    cmd()
        .args(["remove-locale", "en", dst.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn remove_locale_json() {
    let tmp = TempDir::new().unwrap();
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);

    let output = cmd()
        .args(["--json", "remove-locale", "uk", dst.to_str().unwrap()])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(parsed["locale"], "uk");
    assert!(parsed["entries_affected"].is_number());
}

// ══════════════════════════════════════════════════════════════
// Export Command
// ══════════════════════════════════════════════════════════════

#[test]
fn export_creates_xliff() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("uk.xliff");

    cmd()
        .args([
            "export",
            &fixture_path("simple.xcstrings"),
            "--locale",
            "uk",
            "-o",
            output_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(output_path.exists(), "XLIFF file should be created");
    let content = fs::read_to_string(&output_path).unwrap();
    assert!(!content.is_empty(), "XLIFF file should not be empty");
    assert!(content.contains("<xliff"), "should be XLIFF format");
}

#[test]
fn export_json() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("uk.xliff");

    let output = cmd()
        .args([
            "--json",
            "export",
            &fixture_path("simple.xcstrings"),
            "--locale",
            "uk",
            "-o",
            output_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(parsed["locale"], "uk");
    assert!(parsed["exported_count"].is_number());
}

#[test]
fn export_missing_locale_flag() {
    cmd()
        .args(["export", &fixture_path("simple.xcstrings")])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--locale"));
}

// ══════════════════════════════════════════════════════════════
// Import Command (export -> import roundtrip)
// ══════════════════════════════════════════════════════════════

#[test]
fn import_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let xliff_path = tmp.path().join("de.xliff");
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);

    // First export
    cmd()
        .args([
            "export",
            dst.to_str().unwrap(),
            "--locale",
            "de",
            "-o",
            xliff_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Then import
    cmd()
        .args([
            "import",
            dst.to_str().unwrap(),
            "--xliff",
            xliff_path.to_str().unwrap(),
        ])
        .assert()
        .success();
}

#[test]
fn import_dry_run() {
    let tmp = TempDir::new().unwrap();
    let xliff_path = tmp.path().join("de.xliff");
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);

    // Export first
    cmd()
        .args([
            "export",
            dst.to_str().unwrap(),
            "--locale",
            "de",
            "-o",
            xliff_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    let before = fs::read_to_string(&dst).unwrap();

    // Import with dry-run
    cmd()
        .args([
            "import",
            dst.to_str().unwrap(),
            "--xliff",
            xliff_path.to_str().unwrap(),
            "--dry-run",
        ])
        .assert()
        .success();

    let after = fs::read_to_string(&dst).unwrap();
    assert_eq!(before, after, "dry run should not change the file");
}

// ══════════════════════════════════════════════════════════════
// Migrate Command
// ══════════════════════════════════════════════════════════════

#[test]
fn migrate_from_directory() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("Migrated.xcstrings");
    let fixtures_dir = format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR"));

    cmd()
        .args([
            "migrate",
            "--source-language",
            "en",
            "-o",
            output_path.to_str().unwrap(),
            "--directory",
            &fixtures_dir,
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Migrated"));

    assert!(output_path.exists(), "output .xcstrings should be created");
    let content = fs::read_to_string(&output_path).unwrap();
    assert!(content.contains("\"sourceLanguage\""));
}

#[test]
fn migrate_from_files() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("Output.xcstrings");

    cmd()
        .args([
            "migrate",
            "--source-language",
            "en",
            "-o",
            output_path.to_str().unwrap(),
            "--files",
            &fixture_path("en.lproj/Localizable.strings"),
            &fixture_path("es.lproj/Localizable.strings"),
        ])
        .assert()
        .success();

    assert!(output_path.exists());
}

#[test]
fn migrate_dry_run() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("DryRun.xcstrings");

    cmd()
        .args([
            "migrate",
            "--source-language",
            "en",
            "-o",
            output_path.to_str().unwrap(),
            "--directory",
            &format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR")),
            "--dry-run",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("dry run"));

    assert!(
        !output_path.exists(),
        "dry run should not create output file"
    );
}

#[test]
fn migrate_json() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("JsonOut.xcstrings");

    let output = cmd()
        .args([
            "--json",
            "migrate",
            "--source-language",
            "en",
            "-o",
            output_path.to_str().unwrap(),
            "--files",
            &fixture_path("en.lproj/Localizable.strings"),
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(parsed["source_language"], "en");
    assert!(parsed["total_keys"].is_number());
}

// ══════════════════════════════════════════════════════════════
// Completions Command
// ══════════════════════════════════════════════════════════════

#[test]
fn completions_bash() {
    cmd()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

#[test]
fn completions_zsh() {
    cmd()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

#[test]
fn completions_fish() {
    cmd()
        .args(["completions", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

#[test]
fn completions_invalid_shell() {
    cmd()
        .args(["completions", "nosuchshell"])
        .assert()
        .failure();
}

// ══════════════════════════════════════════════════════════════
// Auto-discovery
// ══════════════════════════════════════════════════════════════

#[test]
fn auto_discovery_single_file() {
    let tmp = TempDir::new().unwrap();
    copy_fixture_to_tmp("simple.xcstrings", &tmp);

    cmd()
        .arg("info")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Source language"));
}

#[test]
fn auto_discovery_no_files() {
    let tmp = TempDir::new().unwrap();

    cmd()
        .arg("info")
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("no .xcstrings files"));
}

#[test]
fn auto_discovery_multiple_files() {
    let tmp = TempDir::new().unwrap();
    copy_fixture_to_tmp("simple.xcstrings", &tmp);
    // Copy a second file
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/with_stale.xcstrings");
    fs::copy(&src, tmp.path().join("with_stale.xcstrings")).unwrap();

    cmd()
        .arg("info")
        .current_dir(tmp.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("found 2 .xcstrings files"));
}

// ══════════════════════════════════════════════════════════════
// JSON Output Consistency
// ══════════════════════════════════════════════════════════════

#[test]
fn all_json_commands_produce_valid_json() {
    let fixture = fixture_path("simple.xcstrings");

    // info --json
    let out = cmd().args(["--json", "info", &fixture]).output().unwrap();
    assert!(
        serde_json::from_slice::<serde_json::Value>(&out.stdout).is_ok(),
        "info --json should produce valid JSON"
    );

    // coverage --json
    let out = cmd()
        .args(["--json", "coverage", &fixture])
        .output()
        .unwrap();
    assert!(
        serde_json::from_slice::<serde_json::Value>(&out.stdout).is_ok(),
        "coverage --json should produce valid JSON"
    );

    // validate --json
    let out = cmd()
        .args(["--json", "validate", &fixture])
        .output()
        .unwrap();
    assert!(
        serde_json::from_slice::<serde_json::Value>(&out.stdout).is_ok(),
        "validate --json should produce valid JSON"
    );

    // search --json
    let out = cmd()
        .args(["--json", "search", "greeting", &fixture])
        .output()
        .unwrap();
    assert!(
        serde_json::from_slice::<serde_json::Value>(&out.stdout).is_ok(),
        "search --json should produce valid JSON"
    );

    // stale --json
    let out = cmd()
        .args(["--json", "stale", &fixture_path("with_stale.xcstrings")])
        .output()
        .unwrap();
    assert!(
        serde_json::from_slice::<serde_json::Value>(&out.stdout).is_ok(),
        "stale --json should produce valid JSON"
    );
}

// ══════════════════════════════════════════════════════════════
// Import Command — additional coverage
// ══════════════════════════════════════════════════════════════

#[test]
fn import_json_output() {
    let tmp = TempDir::new().unwrap();
    let xliff_path = tmp.path().join("de.xliff");
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);

    // Export first
    cmd()
        .args([
            "export",
            dst.to_str().unwrap(),
            "--locale",
            "de",
            "-o",
            xliff_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Import with --json
    let output = cmd()
        .args([
            "--json",
            "import",
            dst.to_str().unwrap(),
            "--xliff",
            xliff_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(parsed["accepted"].is_number());
    assert!(parsed["dry_run"].is_boolean());
}

#[test]
fn import_json_dry_run() {
    let tmp = TempDir::new().unwrap();
    let xliff_path = tmp.path().join("de.xliff");
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);

    // Export first
    cmd()
        .args([
            "export",
            dst.to_str().unwrap(),
            "--locale",
            "de",
            "-o",
            xliff_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Import with --json --dry-run
    let output = cmd()
        .args([
            "--json",
            "import",
            dst.to_str().unwrap(),
            "--xliff",
            xliff_path.to_str().unwrap(),
            "--dry-run",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(parsed["dry_run"], true);
}

#[test]
fn import_nonexistent_xcstrings() {
    let tmp = TempDir::new().unwrap();
    let xliff_path = tmp.path().join("dummy.xliff");
    fs::write(&xliff_path, "<xliff/>").unwrap();

    cmd()
        .args([
            "import",
            "/tmp/nonexistent_xcstrings_12345.xcstrings",
            "--xliff",
            xliff_path.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn import_nonexistent_xliff() {
    let tmp = TempDir::new().unwrap();
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);

    cmd()
        .args([
            "import",
            dst.to_str().unwrap(),
            "--xliff",
            "/tmp/nonexistent_xliff_12345.xliff",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn import_invalid_xliff() {
    let tmp = TempDir::new().unwrap();
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);
    let xliff_path = tmp.path().join("bad.xliff");
    fs::write(&xliff_path, "this is not valid xliff content").unwrap();

    cmd()
        .args([
            "import",
            dst.to_str().unwrap(),
            "--xliff",
            xliff_path.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn import_with_all_flag() {
    let tmp = TempDir::new().unwrap();
    let xliff_path = tmp.path().join("uk.xliff");
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);

    // Export all (including already translated)
    cmd()
        .args([
            "export",
            dst.to_str().unwrap(),
            "--locale",
            "uk",
            "-o",
            xliff_path.to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success();

    // Import should work
    cmd()
        .args([
            "import",
            dst.to_str().unwrap(),
            "--xliff",
            xliff_path.to_str().unwrap(),
        ])
        .assert()
        .success();
}

// ══════════════════════════════════════════════════════════════
// Export Command — additional coverage
// ══════════════════════════════════════════════════════════════

#[test]
fn export_nonexistent_file() {
    cmd()
        .args([
            "export",
            "/tmp/nonexistent_file_12345.xcstrings",
            "--locale",
            "uk",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn export_all_flag() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("uk_all.xliff");

    cmd()
        .args([
            "export",
            &fixture_path("simple.xcstrings"),
            "--locale",
            "uk",
            "-o",
            output_path.to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success();

    assert!(output_path.exists());
    let content = fs::read_to_string(&output_path).unwrap();
    assert!(content.contains("<xliff"));
}

#[test]
fn export_json_dry_run_equivalent() {
    // Export with --json and verify structure contains all expected fields
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("test.xliff");

    let output = cmd()
        .args([
            "--json",
            "export",
            &fixture_path("simple.xcstrings"),
            "--locale",
            "uk",
            "-o",
            output_path.to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(parsed["output_path"].is_string());
    assert_eq!(parsed["locale"], "uk");
    assert!(parsed["exported_count"].as_u64().unwrap() > 0);
}

// ══════════════════════════════════════════════════════════════
// Validate Command — additional coverage
// ══════════════════════════════════════════════════════════════

#[test]
fn validate_with_locale_filter() {
    cmd()
        .args([
            "validate",
            &fixture_path("simple.xcstrings"),
            "--locale",
            "en",
        ])
        .assert()
        .success();
}

#[test]
fn validate_json_with_locale() {
    let output = cmd()
        .args([
            "--json",
            "validate",
            &fixture_path("simple.xcstrings"),
            "--locale",
            "en",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(parsed.is_array());
}

// ══════════════════════════════════════════════════════════════
// Migrate Command — additional coverage
// ══════════════════════════════════════════════════════════════

#[test]
fn migrate_no_source_error() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("Output.xcstrings");

    // No --directory and no --files: should fail from clap or validation
    cmd()
        .args([
            "migrate",
            "--source-language",
            "en",
            "-o",
            output_path.to_str().unwrap(),
        ])
        .assert()
        .failure();
}

#[test]
fn migrate_both_dir_and_files_error() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("Output.xcstrings");
    let fixtures_dir = format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR"));

    cmd()
        .args([
            "migrate",
            "--source-language",
            "en",
            "-o",
            output_path.to_str().unwrap(),
            "--directory",
            &fixtures_dir,
            "--files",
            &fixture_path("en.lproj/Localizable.strings"),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn migrate_bad_output_extension() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("Output.txt");

    cmd()
        .args([
            "migrate",
            "--source-language",
            "en",
            "-o",
            output_path.to_str().unwrap(),
            "--files",
            &fixture_path("en.lproj/Localizable.strings"),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn migrate_bad_file_extension() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("Output.xcstrings");
    let bad_file = tmp.path().join("en.lproj");
    fs::create_dir_all(&bad_file).unwrap();
    let bad_path = bad_file.join("Localizable.txt");
    fs::write(&bad_path, "dummy").unwrap();

    cmd()
        .args([
            "migrate",
            "--source-language",
            "en",
            "-o",
            output_path.to_str().unwrap(),
            "--files",
            bad_path.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn migrate_missing_source_language() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("Output.xcstrings");

    // Use "zz" as source language that doesn't exist in any locale data
    cmd()
        .args([
            "migrate",
            "--source-language",
            "zz",
            "-o",
            output_path.to_str().unwrap(),
            "--files",
            &fixture_path("en.lproj/Localizable.strings"),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn migrate_empty_directory() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("Output.xcstrings");
    let empty_dir = tmp.path().join("empty");
    fs::create_dir_all(&empty_dir).unwrap();

    cmd()
        .args([
            "migrate",
            "--source-language",
            "en",
            "-o",
            output_path.to_str().unwrap(),
            "--directory",
            empty_dir.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn migrate_json_dry_run() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("DryRunJson.xcstrings");

    let output = cmd()
        .args([
            "--json",
            "migrate",
            "--source-language",
            "en",
            "-o",
            output_path.to_str().unwrap(),
            "--files",
            &fixture_path("en.lproj/Localizable.strings"),
            "--dry-run",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(parsed["dry_run"], true);
    assert!(!output_path.exists(), "dry run should not create file");
}

#[test]
fn migrate_merge_mode() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("Merged.xcstrings");

    // First migration creates the file
    cmd()
        .args([
            "migrate",
            "--source-language",
            "en",
            "-o",
            output_path.to_str().unwrap(),
            "--files",
            &fixture_path("en.lproj/Localizable.strings"),
        ])
        .assert()
        .success();

    assert!(output_path.exists());
    let before = fs::read_to_string(&output_path).unwrap();

    // Second migration with same output should merge (skip existing keys)
    cmd()
        .args([
            "migrate",
            "--source-language",
            "en",
            "-o",
            output_path.to_str().unwrap(),
            "--files",
            &fixture_path("en.lproj/Localizable.strings"),
        ])
        .assert()
        .success();

    let after = fs::read_to_string(&output_path).unwrap();
    // File should still be valid
    let parsed: serde_json::Value =
        serde_json::from_str(&after).expect("merged file should be valid JSON");
    assert!(parsed["strings"].is_object());
    // Keys should be same count (merge skips existing)
    let before_parsed: serde_json::Value = serde_json::from_str(&before).unwrap();
    assert_eq!(
        before_parsed["strings"].as_object().unwrap().len(),
        parsed["strings"].as_object().unwrap().len()
    );
}

#[test]
fn migrate_with_stringsdict_files() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("WithPlurals.xcstrings");

    cmd()
        .args([
            "migrate",
            "--source-language",
            "en",
            "-o",
            output_path.to_str().unwrap(),
            "--files",
            &fixture_path("en.lproj/Localizable.strings"),
            &fixture_path("en.lproj/Localizable.stringsdict"),
        ])
        .assert()
        .success();

    assert!(output_path.exists());
    let content = fs::read_to_string(&output_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).expect("valid JSON");
    assert!(parsed["strings"].is_object());
}

#[test]
fn migrate_multiple_locales_with_stringsdict() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("MultiLocale.xcstrings");

    cmd()
        .args([
            "migrate",
            "--source-language",
            "en",
            "-o",
            output_path.to_str().unwrap(),
            "--files",
            &fixture_path("en.lproj/Localizable.strings"),
            &fixture_path("en.lproj/Localizable.stringsdict"),
            &fixture_path("es.lproj/Localizable.strings"),
            &fixture_path("es.lproj/Localizable.stringsdict"),
        ])
        .assert()
        .success();

    let content = fs::read_to_string(&output_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).expect("valid JSON");
    assert!(parsed["strings"].is_object());
}

#[test]
fn migrate_json_with_warnings() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("Warnings.xcstrings");

    // Migrate with both .strings and .stringsdict that have overlapping keys
    let output = cmd()
        .args([
            "--json",
            "migrate",
            "--source-language",
            "en",
            "-o",
            output_path.to_str().unwrap(),
            "--files",
            &fixture_path("en.lproj/Localizable.strings"),
            &fixture_path("en.lproj/Localizable.stringsdict"),
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(parsed["total_keys"].is_number());
    assert!(parsed["plural_keys"].is_number());
}

// ══════════════════════════════════════════════════════════════
// Locale Commands — additional coverage
// ══════════════════════════════════════════════════════════════

#[test]
fn add_locale_json_dry_run() {
    let tmp = TempDir::new().unwrap();
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);

    let output = cmd()
        .args([
            "--json",
            "add-locale",
            "fr",
            dst.to_str().unwrap(),
            "--dry-run",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(parsed["dry_run"], true);
    assert_eq!(parsed["locale"], "fr");
}

#[test]
fn add_locale_nonexistent_file() {
    cmd()
        .args(["add-locale", "fr", "/tmp/nonexistent_file_12345.xcstrings"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn remove_locale_json_dry_run() {
    let tmp = TempDir::new().unwrap();
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);

    let output = cmd()
        .args([
            "--json",
            "remove-locale",
            "uk",
            dst.to_str().unwrap(),
            "--dry-run",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(parsed["dry_run"], true);
    assert_eq!(parsed["locale"], "uk");
}

#[test]
fn remove_locale_nonexistent_file() {
    cmd()
        .args([
            "remove-locale",
            "uk",
            "/tmp/nonexistent_file_12345.xcstrings",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn remove_locale_not_found() {
    let tmp = TempDir::new().unwrap();
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);

    cmd()
        .args(["remove-locale", "zz", dst.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

// ══════════════════════════════════════════════════════════════
// Common module — additional coverage (auto-discovery edge cases)
// ══════════════════════════════════════════════════════════════

// ══════════════════════════════════════════════════════════════
// Validate Command — text output with issues
// ══════════════════════════════════════════════════════════════

#[test]
fn validate_with_issues_text_output() {
    let tmp = TempDir::new().unwrap();
    // Create a file with a validation issue: format specifier mismatch
    let xcstrings = r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "items_count" : {
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "%lld items"
          }
        },
        "uk" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "елементів"
          }
        }
      }
    }
  },
  "version" : "1.0"
}"#;
    let path = tmp.path().join("issues.xcstrings");
    fs::write(&path, xcstrings).unwrap();

    cmd()
        .args(["validate", path.to_str().unwrap()])
        .assert()
        .code(EXIT_VALIDATION_ISSUES)
        .stdout(predicate::str::contains("Locale"))
        .stdout(predicate::str::contains("items_count"));
}

#[test]
fn validate_with_issues_json_output() {
    let tmp = TempDir::new().unwrap();
    let xcstrings = r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "items_count" : {
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "%lld items"
          }
        },
        "uk" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "елементів"
          }
        }
      }
    }
  },
  "version" : "1.0"
}"#;
    let path = tmp.path().join("issues.xcstrings");
    fs::write(&path, xcstrings).unwrap();

    let output = cmd()
        .args(["--json", "validate", path.to_str().unwrap()])
        .assert();

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(parsed.is_array());
}

// ══════════════════════════════════════════════════════════════
// Import Command — write path and rejected text output
// ══════════════════════════════════════════════════════════════

#[test]
fn import_writes_valid_translations() {
    let tmp = TempDir::new().unwrap();
    let xliff_path = tmp.path().join("uk.xliff");
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);

    // Export with --all to get translations including already translated ones
    cmd()
        .args([
            "export",
            dst.to_str().unwrap(),
            "--locale",
            "uk",
            "-o",
            xliff_path.to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success();

    // Import (actual write, not dry-run)
    cmd()
        .args([
            "import",
            dst.to_str().unwrap(),
            "--xliff",
            xliff_path.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Accepted"));

    // File should be valid JSON
    let after = fs::read_to_string(&dst).unwrap();
    let _parsed: serde_json::Value =
        serde_json::from_str(&after).expect("file should remain valid JSON");
}

#[test]
fn import_dry_run_with_valid_translations() {
    let tmp = TempDir::new().unwrap();
    let xliff_path = tmp.path().join("uk.xliff");
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);

    // Export with --all to ensure we get non-empty translations
    cmd()
        .args([
            "export",
            dst.to_str().unwrap(),
            "--locale",
            "uk",
            "-o",
            xliff_path.to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success();

    let before = fs::read_to_string(&dst).unwrap();

    // Import with --dry-run (should not write)
    cmd()
        .args([
            "import",
            dst.to_str().unwrap(),
            "--xliff",
            xliff_path.to_str().unwrap(),
            "--dry-run",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Dry run"));

    let after = fs::read_to_string(&dst).unwrap();
    assert_eq!(before, after, "dry run should not change the file");
}

// ══════════════════════════════════════════════════════════════
// Migrate Command — text output with warnings
// ══════════════════════════════════════════════════════════════

#[test]
fn migrate_text_output_with_warnings() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("Warned.xcstrings");

    // Migrate with .strings and .stringsdict that have overlapping keys to generate warnings
    cmd()
        .args([
            "migrate",
            "--source-language",
            "en",
            "-o",
            output_path.to_str().unwrap(),
            "--files",
            &fixture_path("en.lproj/Localizable.strings"),
            &fixture_path("en.lproj/Localizable.stringsdict"),
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Migrated"));
}

#[test]
fn migrate_text_output_multiple_locales() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("MultiText.xcstrings");

    cmd()
        .args([
            "migrate",
            "--source-language",
            "en",
            "-o",
            output_path.to_str().unwrap(),
            "--files",
            &fixture_path("en.lproj/Localizable.strings"),
            &fixture_path("es.lproj/Localizable.strings"),
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Migrated"))
        .stderr(predicate::str::contains("en:"))
        .stderr(predicate::str::contains("es:"));
}

#[test]
fn import_with_rejected_translations_text() {
    let tmp = TempDir::new().unwrap();
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);

    // Create an XLIFF with a translation for a nonexistent key
    let xliff_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">
  <file source-language="en" target-language="uk" datatype="plaintext" original="simple.xcstrings">
    <body>
      <trans-unit id="nonexistent_key_12345">
        <source>Source</source>
        <target>Target</target>
      </trans-unit>
      <trans-unit id="greeting">
        <source>Hello</source>
        <target>Updated</target>
      </trans-unit>
    </body>
  </file>
</xliff>"#;
    let xliff_path = tmp.path().join("bad.xliff");
    fs::write(&xliff_path, xliff_content).unwrap();

    // Import should succeed but with rejected translations
    cmd()
        .args([
            "import",
            dst.to_str().unwrap(),
            "--xliff",
            xliff_path.to_str().unwrap(),
        ])
        .assert()
        .code(EXIT_VALIDATION_ISSUES)
        .stderr(predicate::str::contains("Rejected"))
        .stderr(predicate::str::contains("nonexistent_key_12345"));
}

#[test]
fn import_with_rejected_translations_json() {
    let tmp = TempDir::new().unwrap();
    let dst = copy_fixture_to_tmp("simple.xcstrings", &tmp);

    let xliff_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">
  <file source-language="en" target-language="uk" datatype="plaintext" original="simple.xcstrings">
    <body>
      <trans-unit id="nonexistent_key_12345">
        <source>Source</source>
        <target>Target</target>
      </trans-unit>
    </body>
  </file>
</xliff>"#;
    let xliff_path = tmp.path().join("bad.xliff");
    fs::write(&xliff_path, xliff_content).unwrap();

    let output = cmd()
        .args([
            "--json",
            "import",
            dst.to_str().unwrap(),
            "--xliff",
            xliff_path.to_str().unwrap(),
        ])
        .assert()
        .code(EXIT_VALIDATION_ISSUES);

    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(!parsed["rejected"].as_array().unwrap().is_empty());
}

#[test]
fn migrate_stringsdict_with_skipped_keys() {
    let tmp = TempDir::new().unwrap();
    let output_path = tmp.path().join("Skipped.xcstrings");

    // Create lproj with a stringsdict containing an unsupported rule type
    let lproj = tmp.path().join("en.lproj");
    fs::create_dir_all(&lproj).unwrap();
    fs::write(
        lproj.join("Localizable.strings"),
        "\"greeting\" = \"Hello\";",
    )
    .unwrap();
    fs::write(
        lproj.join("Localizable.stringsdict"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>width_rule</key>
    <dict>
        <key>NSStringLocalizedFormatKey</key>
        <string>%#@width@</string>
        <key>width</key>
        <dict>
            <key>NSStringFormatSpecTypeKey</key>
            <string>NSStringVariableWidthRuleType</string>
            <key>NSStringFormatValueTypeKey</key>
            <string>lld</string>
            <key>one</key>
            <string>one</string>
            <key>other</key>
            <string>others</string>
        </dict>
    </dict>
</dict>
</plist>"#,
    )
    .unwrap();

    cmd()
        .args([
            "migrate",
            "--source-language",
            "en",
            "-o",
            output_path.to_str().unwrap(),
            "--directory",
            tmp.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Warnings"))
        .stderr(predicate::str::contains("skipped"));
}

// ══════════════════════════════════════════════════════════════
// Error handling edge cases for common module coverage
// ══════════════════════════════════════════════════════════════

#[test]
fn load_invalid_json_xcstrings() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("bad.xcstrings");
    fs::write(&path, "this is not valid json").unwrap();

    cmd()
        .args(["info", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn validate_invalid_json_xcstrings() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("bad.xcstrings");
    fs::write(&path, "{ invalid json }").unwrap();

    cmd()
        .args(["validate", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn export_invalid_json_xcstrings() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("bad.xcstrings");
    fs::write(&path, "not json").unwrap();

    cmd()
        .args(["export", path.to_str().unwrap(), "--locale", "uk"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn import_invalid_json_xcstrings() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("bad.xcstrings");
    fs::write(&path, "not json").unwrap();
    let xliff_path = tmp.path().join("dummy.xliff");
    fs::write(&xliff_path, "<xliff/>").unwrap();

    cmd()
        .args([
            "import",
            path.to_str().unwrap(),
            "--xliff",
            xliff_path.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn auto_discovery_nested_xcstrings() {
    let tmp = TempDir::new().unwrap();
    let nested = tmp.path().join("subdir");
    fs::create_dir_all(&nested).unwrap();
    let src =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/simple.xcstrings");
    fs::copy(&src, nested.join("simple.xcstrings")).unwrap();

    cmd()
        .arg("info")
        .current_dir(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("Source language"));
}
