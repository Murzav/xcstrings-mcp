use serde_json::{Value, json};
use tempfile::TempDir;
use xcstrings_mcp::error::XcStringsError;
use xcstrings_mcp::io::fs::FsFileStore;
use xcstrings_mcp::service::merge_operation::{MergeRequest, execute_merge};
use xcstrings_mcp::service::semantic_merge::fingerprint;
use xcstrings_mcp::service::semantic_merge::{ConflictChoice, ConflictResolution};

fn catalog(strings: Value) -> String {
    serde_json::to_string_pretty(&json!({
        "sourceLanguage": "en",
        "strings": strings,
        "version": "1.0",
        "futureRoot": {"preserved": true}
    }))
    .unwrap()
}

fn request(dir: &TempDir, dry_run: bool) -> MergeRequest {
    MergeRequest {
        base_path: dir.path().join("base.xcstrings"),
        current_path: dir.path().join("current.xcstrings"),
        incoming_path: dir.path().join("incoming.xcstrings"),
        output_path: dir.path().join("output.xcstrings"),
        dry_run,
        resolutions: Vec::new(),
        expected_fingerprints: None,
        conflict_offset: 0,
        conflict_limit: 50,
    }
}

fn write_inputs(dir: &TempDir, base: Value, current: Value, incoming: Value) {
    std::fs::write(dir.path().join("base.xcstrings"), catalog(base)).unwrap();
    std::fs::write(dir.path().join("current.xcstrings"), catalog(current)).unwrap();
    std::fs::write(dir.path().join("incoming.xcstrings"), catalog(incoming)).unwrap();
}

#[test]
fn dry_run_returns_apply_fingerprints_and_apply_writes_formatted_unknown_preserving_result() {
    let dir = TempDir::new().unwrap();
    write_inputs(
        &dir,
        json!({"a": {"comment": "base"}, "b": {"comment": "base"}}),
        json!({"a": {"comment": "current"}, "b": {"comment": "base"}}),
        json!({"a": {"comment": "base"}, "b": {"comment": "incoming"}}),
    );
    let store = FsFileStore::new();
    let dry_request = request(&dir, true);
    let base_before = std::fs::read(&dry_request.base_path).unwrap();
    let current_before = std::fs::read(&dry_request.current_path).unwrap();
    let incoming_before = std::fs::read(&dry_request.incoming_path).unwrap();
    let dry = execute_merge(&store, &dry_request).unwrap();

    assert!(dry.report.dry_run);
    assert!(!dry.report.written);
    assert!(!dry_request.output_path.exists());
    assert_eq!(std::fs::read(&dry_request.base_path).unwrap(), base_before);
    assert_eq!(
        std::fs::read(&dry_request.current_path).unwrap(),
        current_before
    );
    assert_eq!(
        std::fs::read(&dry_request.incoming_path).unwrap(),
        incoming_before
    );
    let expected = dry.report.expected_fingerprints.clone().unwrap();
    assert_eq!(expected.output, None);
    assert_eq!(dry.report.fingerprints.result.key_count, 2);

    let mut apply_request = request(&dir, false);
    apply_request.expected_fingerprints = Some(expected);
    let applied = execute_merge(&store, &apply_request).unwrap();
    assert!(!applied.report.dry_run);
    assert!(applied.report.written);
    let output = std::fs::read_to_string(&apply_request.output_path).unwrap();
    assert!(output.contains("\"sourceLanguage\" : \"en\""));
    let value: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["strings"]["a"]["comment"], "current");
    assert_eq!(value["strings"]["b"]["comment"], "incoming");
    assert_eq!(value["futureRoot"], json!({"preserved": true}));
}

#[test]
fn conflict_never_writes_without_resolution_and_stable_id_can_choose_current() {
    let dir = TempDir::new().unwrap();
    write_inputs(
        &dir,
        json!({"key": {"localizations": {"en": {"stringUnit": {"state": "translated", "value": "base"}}}}}),
        json!({"key": {"localizations": {"en": {"stringUnit": {"state": "translated", "value": "current"}}}}}),
        json!({"key": {"localizations": {"en": {"stringUnit": {"state": "translated", "value": "incoming"}}}}}),
    );
    let store = FsFileStore::new();
    let dry = execute_merge(&store, &request(&dir, true)).unwrap();
    assert_eq!(dry.report.unresolved_conflict_total, 1);
    let conflict_id = dry.report.conflicts[0].id.clone();

    let mut unresolved = request(&dir, false);
    unresolved.expected_fingerprints = dry.report.expected_fingerprints.clone();
    let error = execute_merge(&store, &unresolved).unwrap_err();
    assert!(matches!(error, XcStringsError::MergeConflicts { count: 1 }));
    assert!(!unresolved.output_path.exists());

    unresolved.resolutions.push(ConflictResolution {
        conflict_id,
        choice: ConflictChoice::Current,
    });
    let applied = execute_merge(&store, &unresolved).unwrap();
    assert_eq!(applied.report.resolutions_applied, 1);
    let output: Value =
        serde_json::from_slice(&std::fs::read(&unresolved.output_path).unwrap()).unwrap();
    assert_eq!(
        output["strings"]["key"]["localizations"]["en"]["stringUnit"]["value"],
        "current"
    );
}

#[test]
fn stale_input_or_output_refuses_and_preserves_external_bytes() {
    let dir = TempDir::new().unwrap();
    write_inputs(
        &dir,
        json!({"a": {}}),
        json!({"a": {}, "current": {}}),
        json!({"a": {}, "incoming": {}}),
    );
    let store = FsFileStore::new();
    let dry = execute_merge(&store, &request(&dir, true)).unwrap();

    std::fs::write(
        dir.path().join("incoming.xcstrings"),
        catalog(json!({"changed": {}})),
    )
    .unwrap();
    let mut apply = request(&dir, false);
    apply.expected_fingerprints = dry.report.expected_fingerprints.clone();
    let stale_input = execute_merge(&store, &apply).unwrap_err();
    assert!(
        matches!(stale_input, XcStringsError::StaleMergeFingerprint { ref input } if input == "incoming")
    );
    assert!(!apply.output_path.exists());

    write_inputs(
        &dir,
        json!({"a": {}}),
        json!({"a": {}, "current": {}}),
        json!({"a": {}, "incoming": {}}),
    );
    let dry = execute_merge(&store, &request(&dir, true)).unwrap();
    let external = b"external output must survive";
    std::fs::write(&apply.output_path, external).unwrap();
    apply.expected_fingerprints = dry.report.expected_fingerprints;
    let stale_output = execute_merge(&store, &apply).unwrap_err();
    assert!(
        matches!(stale_output, XcStringsError::StaleMergeFingerprint { ref input } if input == "output")
    );
    assert_eq!(std::fs::read(&apply.output_path).unwrap(), external);
}

#[test]
fn apply_rejects_new_validation_error_without_writing() {
    let dir = TempDir::new().unwrap();
    let base_entry = json!({"key": {"localizations": {
        "en": {"stringUnit": {"state": "translated", "value": "Hello %@"}},
        "de": {"stringUnit": {"state": "translated", "value": "Hallo %@"}}
    }}});
    let incoming_entry = json!({"key": {"localizations": {
        "en": {"stringUnit": {"state": "translated", "value": "Hello %@"}},
        "de": {"stringUnit": {"state": "translated", "value": "Hallo"}}
    }}});
    write_inputs(&dir, base_entry.clone(), base_entry, incoming_entry);
    let store = FsFileStore::new();
    let dry = execute_merge(&store, &request(&dir, true)).unwrap();
    assert!(dry.report.introduced_validation_issues.iter().any(|issue| {
        issue.severity == "error" && issue.issue_type == "format_specifier_count_mismatch"
    }));

    let mut apply = request(&dir, false);
    apply.expected_fingerprints = dry.report.expected_fingerprints;
    let error = execute_merge(&store, &apply).unwrap_err();
    assert!(matches!(error, XcStringsError::MergeIntroducedValidation { count } if count >= 1));
    assert!(!apply.output_path.exists());
}

#[test]
fn validation_delta_omits_current_error_fixed_by_merge_result() {
    let dir = TempDir::new().unwrap();
    let invalid = json!({"key": {"localizations": {
        "en": {"stringUnit": {"state": "translated", "value": "Hello %@"}},
        "de": {"stringUnit": {"state": "translated", "value": "Hallo"}}
    }}});
    let fixed = json!({"key": {"localizations": {
        "en": {"stringUnit": {"state": "translated", "value": "Hello %@"}},
        "de": {"stringUnit": {"state": "translated", "value": "Hallo %@"}}
    }}});
    write_inputs(&dir, invalid.clone(), invalid, fixed);

    let report = execute_merge(&FsFileStore::new(), &request(&dir, true))
        .unwrap()
        .report;
    assert!(report.existing_validation_issues.is_empty());
    assert!(report.introduced_validation_issues.is_empty());
}

#[test]
fn validation_delta_omits_current_error_deleted_by_merge_result() {
    let dir = TempDir::new().unwrap();
    let invalid = json!({"key": {"localizations": {
        "en": {"stringUnit": {"state": "translated", "value": "Hello %@"}},
        "de": {"stringUnit": {"state": "translated", "value": "Hallo"}}
    }}});
    write_inputs(&dir, invalid.clone(), invalid, json!({}));

    let report = execute_merge(&FsFileStore::new(), &request(&dir, true))
        .unwrap()
        .report;
    assert!(report.existing_validation_issues.is_empty());
    assert!(report.introduced_validation_issues.is_empty());
}

#[test]
fn validation_delta_retains_issue_still_present_in_merge_result() {
    let dir = TempDir::new().unwrap();
    let invalid = json!({"key": {"localizations": {
        "en": {"stringUnit": {"state": "translated", "value": "Hello %@"}},
        "de": {"stringUnit": {"state": "translated", "value": "Hallo"}}
    }}});
    write_inputs(&dir, invalid.clone(), invalid.clone(), invalid);

    let report = execute_merge(&FsFileStore::new(), &request(&dir, true))
        .unwrap()
        .report;
    assert_eq!(report.existing_validation_issues.len(), 1);
    assert_eq!(
        report.existing_validation_issues[0].issue_type,
        "format_specifier_count_mismatch"
    );
    assert!(report.introduced_validation_issues.is_empty());
}

#[test]
fn apply_reports_but_does_not_block_a_new_validation_warning() {
    let dir = TempDir::new().unwrap();
    let base_entry = json!({"key": {"localizations": {
        "en": {"stringUnit": {"state": "translated", "value": "Settings"}},
        "de": {"stringUnit": {"state": "translated", "value": "Einstellungen"}}
    }}});
    let warning_entry = json!({"key": {"localizations": {
        "en": {"stringUnit": {"state": "translated", "value": "Settings"}},
        "de": {"stringUnit": {"state": "translated", "value": "Settings"}}
    }}});
    write_inputs(&dir, base_entry.clone(), base_entry, warning_entry);
    let store = FsFileStore::new();
    let dry = execute_merge(&store, &request(&dir, true)).unwrap();
    assert!(
        dry.report.introduced_validation_issues.iter().any(|issue| {
            issue.severity == "warning" && issue.issue_type == "identical_to_source"
        })
    );
    assert!(
        !dry.report
            .introduced_validation_issues
            .iter()
            .any(|issue| issue.severity == "error")
    );

    let mut apply = request(&dir, false);
    apply.expected_fingerprints = dry.report.expected_fingerprints;
    let applied = execute_merge(&store, &apply).unwrap();
    assert!(applied.report.written);
    assert!(apply.output_path.exists());
}

#[test]
fn existing_output_can_be_replaced_only_with_its_exact_dry_run_fingerprint() {
    let dir = TempDir::new().unwrap();
    write_inputs(
        &dir,
        json!({"a": {}}),
        json!({"a": {}, "current": {}}),
        json!({"a": {}, "incoming": {}}),
    );
    let store = FsFileStore::new();
    let output = request(&dir, true).output_path;
    let old_output = catalog(json!({"old": {}}));
    std::fs::write(&output, old_output.as_bytes()).unwrap();
    let dry = execute_merge(&store, &request(&dir, true)).unwrap();
    assert_eq!(
        dry.report
            .fingerprints
            .output_before
            .as_ref()
            .unwrap()
            .key_count,
        1
    );
    assert!(
        dry.report
            .expected_fingerprints
            .as_ref()
            .unwrap()
            .output
            .is_some()
    );

    let mut apply = request(&dir, false);
    apply.expected_fingerprints = dry.report.expected_fingerprints;
    let applied = execute_merge(&store, &apply).unwrap();
    assert!(applied.report.written);
    let value: Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    assert_eq!(value["strings"].as_object().unwrap().len(), 3);
    assert!(value["strings"].get("old").is_none());
}

#[test]
fn apply_rejects_existing_output_deleted_after_dry_run_without_recreating_it() {
    let dir = TempDir::new().unwrap();
    write_inputs(
        &dir,
        json!({"a": {}}),
        json!({"a": {}, "current": {}}),
        json!({"a": {}, "incoming": {}}),
    );
    let store = FsFileStore::new();
    let output = request(&dir, true).output_path;
    std::fs::write(&output, catalog(json!({"old": {}}))).unwrap();
    let dry = execute_merge(&store, &request(&dir, true)).unwrap();
    assert!(
        dry.report
            .expected_fingerprints
            .as_ref()
            .unwrap()
            .output
            .is_some()
    );
    std::fs::remove_file(&output).unwrap();

    let mut apply = request(&dir, false);
    apply.expected_fingerprints = dry.report.expected_fingerprints;
    let error = execute_merge(&store, &apply).unwrap_err();
    assert!(
        matches!(error, XcStringsError::StaleMergeFingerprint { ref input } if input == "output")
    );
    assert!(!output.exists());
}

#[test]
fn output_may_be_the_current_path_and_is_compared_before_replacement() {
    let dir = TempDir::new().unwrap();
    write_inputs(
        &dir,
        json!({"a": {}}),
        json!({"a": {}, "current": {}}),
        json!({"a": {}, "incoming": {}}),
    );
    let store = FsFileStore::new();
    let mut dry_request = request(&dir, true);
    dry_request.output_path = dry_request.current_path.clone();
    let dry = execute_merge(&store, &dry_request).unwrap();
    assert_eq!(
        dry.report
            .fingerprints
            .output_before
            .as_ref()
            .unwrap()
            .sha256,
        dry.report.fingerprints.current.sha256
    );

    let mut apply_request = dry_request;
    apply_request.dry_run = false;
    apply_request.expected_fingerprints = dry.report.expected_fingerprints;
    let applied = execute_merge(&store, &apply_request).unwrap();
    assert!(applied.report.written);
    let value: Value =
        serde_json::from_slice(&std::fs::read(&apply_request.current_path).unwrap()).unwrap();
    assert_eq!(value["strings"].as_object().unwrap().len(), 3);
    assert!(value["strings"].get("current").is_some());
    assert!(value["strings"].get("incoming").is_some());
}

#[test]
fn existing_bom_output_uses_bom_aware_key_count_and_exact_raw_fingerprint() {
    let dir = TempDir::new().unwrap();
    write_inputs(&dir, json!({"a": {}}), json!({"a": {}}), json!({"a": {}}));
    let output = request(&dir, true).output_path;
    let raw = [
        b"\xef\xbb\xbf".as_slice(),
        catalog(json!({"old": {}})).as_bytes(),
    ]
    .concat();
    std::fs::write(&output, &raw).unwrap();

    let report = execute_merge(&FsFileStore::new(), &request(&dir, true))
        .unwrap()
        .report;
    let before = report.fingerprints.output_before.unwrap();
    assert_eq!(before.sha256, fingerprint(&raw));
    assert_eq!(before.key_count, 1);
}

#[cfg(unix)]
#[test]
fn output_path_matrix_supports_absent_regular_and_live_symlink_dry_apply() {
    use std::os::unix::fs::symlink;

    for output_kind in ["absent", "regular", "live_symlink"] {
        let dir = TempDir::new().unwrap();
        write_inputs(
            &dir,
            json!({"a": {}}),
            json!({"a": {}, "current": {}}),
            json!({"a": {}, "incoming": {}}),
        );
        let output = request(&dir, true).output_path;
        let symlink_target = dir.path().join("symlink-target.xcstrings");
        match output_kind {
            "absent" => {}
            "regular" => std::fs::write(&output, catalog(json!({"old": {}}))).unwrap(),
            "live_symlink" => {
                std::fs::write(&symlink_target, catalog(json!({"old": {}}))).unwrap();
                symlink(&symlink_target, &output).unwrap();
            }
            _ => unreachable!(),
        }

        let dry = execute_merge(&FsFileStore::new(), &request(&dir, true)).unwrap();
        assert_eq!(
            dry.report.fingerprints.output_before.is_some(),
            output_kind != "absent",
            "{output_kind}"
        );
        let mut apply = request(&dir, false);
        apply.expected_fingerprints = dry.report.expected_fingerprints;
        execute_merge(&FsFileStore::new(), &apply).unwrap();

        let merged_path = if output_kind == "live_symlink" {
            assert!(
                std::fs::symlink_metadata(&output)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            &symlink_target
        } else {
            &output
        };
        let merged: Value = serde_json::from_slice(&std::fs::read(merged_path).unwrap()).unwrap();
        assert_eq!(
            merged["strings"].as_object().unwrap().len(),
            3,
            "{output_kind}"
        );
    }
}

#[cfg(unix)]
#[test]
fn dangling_output_is_reported_on_dry_run_and_stale_on_apply_without_replacement() {
    use std::os::unix::fs::symlink;

    let dir = TempDir::new().unwrap();
    write_inputs(&dir, json!({"a": {}}), json!({"a": {}}), json!({"a": {}}));
    let output = request(&dir, true).output_path;
    let missing_target = dir.path().join("missing-target.xcstrings");
    symlink(&missing_target, &output).unwrap();

    let dry_error = execute_merge(&FsFileStore::new(), &request(&dir, true)).unwrap_err();
    assert!(matches!(dry_error, XcStringsError::FileNotFound { .. }));
    assert_eq!(std::fs::read_link(&output).unwrap(), missing_target);

    std::fs::remove_file(&output).unwrap();
    let dry = execute_merge(&FsFileStore::new(), &request(&dir, true)).unwrap();
    symlink(&missing_target, &output).unwrap();
    let mut apply = request(&dir, false);
    apply.expected_fingerprints = dry.report.expected_fingerprints;
    let stale = execute_merge(&FsFileStore::new(), &apply).unwrap_err();
    assert!(
        matches!(stale, XcStringsError::StaleMergeFingerprint { ref input } if input == "output")
    );
    assert!(
        std::fs::symlink_metadata(&output)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_link(&output).unwrap(), missing_target);
}
