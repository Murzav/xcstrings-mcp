use assert_cmd::Command;
use serde_json::json;
use tempfile::TempDir;
use xcstrings_mcp::{
    io::fs::FsFileStore,
    model::workflow::SyncMode,
    workflow_operation::{
        CatalogSnapshot,
        sync::{SyncRequest, synchronize},
    },
};

fn fixture(directory: &TempDir) -> std::path::PathBuf {
    let path = directory.path().join("Localizable.xcstrings");
    std::fs::write(&path, serde_json::to_vec(&json!({
        "sourceLanguage":"en", "version":"1.0", "strings":{
            "Open":{"localizations":{"de":{"stringUnit":{"state":"translated","value":"Öffnen"}}}}
        }
    })).unwrap()).unwrap();
    path
}

fn assert_uninitialized(command: &str) {
    let directory = TempDir::new().unwrap();
    let path = fixture(&directory);
    let before = std::fs::read(&path).unwrap();
    let output = Command::cargo_bin("xcstrings-mcp")
        .unwrap()
        .current_dir(directory.path())
        .arg(command)
        .arg(&path)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).unwrap();
    assert_eq!(
        text.lines().next(),
        Some("Source tracking: uninitialized; historical freshness unknown.")
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(
        !path
            .with_file_name("Localizable.xcstrings.xcstrings-mcp.json")
            .exists()
    );
}

#[test]
fn coverage_discloses_uninitialized_source_tracking() {
    assert_uninitialized("coverage");
}
#[test]
fn info_discloses_uninitialized_source_tracking() {
    assert_uninitialized("info");
}
#[test]
fn validation_discloses_uninitialized_source_tracking() {
    assert_uninitialized("validate");
}

#[test]
fn validation_lists_new_untracked_keys_after_initialization() {
    let directory = TempDir::new().unwrap();
    let path = fixture(&directory);
    let store = FsFileStore::new();
    let snapshot = CatalogSnapshot::load(&store, &path).unwrap();
    synchronize(
        &store,
        &path,
        &SyncRequest {
            mode: SyncMode::AdoptExisting,
            dry_run: false,
            expected: Some(snapshot.revisions()),
        },
    )
    .unwrap();
    let mut catalog: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    catalog["strings"]["Close"] =
        json!({"localizations":{"de":{"stringUnit":{"state":"translated","value":"Schließen"}}}});
    std::fs::write(&path, serde_json::to_vec(&catalog).unwrap()).unwrap();

    let output = Command::cargo_bin("xcstrings-mcp")
        .unwrap()
        .current_dir(directory.path())
        .arg("validate")
        .arg(&path)
        .assert()
        .success()
        .get_output()
        .clone();
    let text = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        text.lines().next(),
        Some("Source tracking: initialized; source-changed keys: 0; untracked keys: 1.")
    );
    assert!(
        text.lines()
            .any(|line| line == "Untracked source keys: Close")
    );
}
