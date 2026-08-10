use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serde_json::{Value, json};
use tempfile::TempDir;

fn write_catalogs(dir: &TempDir) -> (String, String, String, String) {
    let base = dir.path().join("base.xcstrings");
    let current = dir.path().join("current.xcstrings");
    let incoming = dir.path().join("incoming.xcstrings");
    let output = dir.path().join("output.xcstrings");
    let catalog = |mut strings: Value| {
        strings.as_object_mut().unwrap().insert(
            "literal.percent".into(),
            json!({"localizations": {"en": {"stringUnit": {
                "state": "translated",
                "value": "Save up to 50% today"
            }}}}),
        );
        serde_json::to_string(&json!({
            "sourceLanguage": "en",
            "strings": strings,
            "version": "1.0"
        }))
        .unwrap()
    };
    std::fs::write(&base, catalog(json!({"a": {}, "b": {}}))).unwrap();
    std::fs::write(
        &current,
        catalog(json!({"a": {"comment": "current"}, "b": {}})),
    )
    .unwrap();
    std::fs::write(
        &incoming,
        catalog(json!({"a": {}, "b": {"comment": "incoming"}})),
    )
    .unwrap();
    (
        base.display().to_string(),
        current.display().to_string(),
        incoming.display().to_string(),
        output.display().to_string(),
    )
}

fn conflict_catalog(comment: &str) -> String {
    serde_json::to_string(&json!({
        "sourceLanguage": "en",
        "strings": {"key": {"comment": comment}},
        "version": "1.0"
    }))
    .unwrap()
}

#[test]
fn cli_help_and_json_dry_run_expose_the_complete_merge_contract() {
    cargo_bin_cmd!("xcstrings-mcp")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("28 MCP tools"))
        .stdout(predicate::str::contains("merge"));
    cargo_bin_cmd!("xcstrings-mcp")
        .args(["merge", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--base"))
        .stdout(predicate::str::contains("--current"))
        .stdout(predicate::str::contains("--incoming"))
        .stdout(predicate::str::contains("--output"))
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--resolution"))
        .stdout(predicate::str::contains("--expected-fingerprints"))
        .stdout(predicate::str::contains("--conflict-offset"))
        .stdout(predicate::str::contains("--conflict-limit"))
        .stdout(predicate::str::contains("cooperating writers"))
        .stdout(predicate::str::contains("internal lock/temp sidecars"))
        .stdout(predicate::str::contains("not a multi-file atomic snapshot"));

    let dir = TempDir::new().unwrap();
    let (base, current, incoming, output) = write_catalogs(&dir);
    let result = cargo_bin_cmd!("xcstrings-mcp")
        .args([
            "--json",
            "merge",
            "--base",
            &base,
            "--current",
            &current,
            "--incoming",
            &incoming,
            "--output",
            &output,
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["written"], false);
    assert_eq!(report["conflict_total"], 0);
    assert_eq!(report["fingerprints"]["result"]["key_count"], 3);
    assert!(report["expected_fingerprints"].is_object());
    assert!(!std::path::Path::new(&output).exists());
}

#[test]
fn final_binary_applies_exact_dry_run_fingerprints_with_all_paths_bare_relative() {
    let dir = TempDir::new().unwrap();
    write_catalogs(&dir);
    let binary = assert_cmd::cargo::cargo_bin!("xcstrings-mcp");
    let bare_args = [
        "--json",
        "merge",
        "--base",
        "base.xcstrings",
        "--current",
        "current.xcstrings",
        "--incoming",
        "incoming.xcstrings",
        "--output",
        "output.xcstrings",
    ];

    let dry = Command::new(binary)
        .current_dir(dir.path())
        .args(bare_args)
        .output()
        .unwrap();
    assert_eq!(dry.status.code(), Some(0));
    assert!(dry.stderr.is_empty());
    let dry_report: Value = serde_json::from_slice(&dry.stdout).unwrap();
    assert_eq!(dry_report["output_path"], "output.xcstrings");
    assert_eq!(dry_report["dry_run"], true);
    assert_eq!(dry_report["written"], false);
    assert_eq!(dry_report["conflict_total"], 0);
    assert_eq!(dry_report["unresolved_conflict_total"], 0);
    assert!(dry_report["expected_fingerprints"]["base"].is_string());
    assert!(dry_report["expected_fingerprints"]["current"].is_string());
    assert!(dry_report["expected_fingerprints"]["incoming"].is_string());
    assert_eq!(dry_report["expected_fingerprints"]["output"], Value::Null);
    assert!(!dir.path().join("output.xcstrings").exists());
    let exact_expected = serde_json::to_string(&dry_report["expected_fingerprints"]).unwrap();

    let apply = Command::new(binary)
        .current_dir(dir.path())
        .args(bare_args)
        .args([
            "--dry-run",
            "false",
            "--expected-fingerprints",
            &exact_expected,
        ])
        .output()
        .unwrap();
    assert_eq!(
        apply.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&apply.stderr)
    );
    assert!(apply.stderr.is_empty());
    let apply_report: Value = serde_json::from_slice(&apply.stdout).unwrap();
    assert_eq!(apply_report["output_path"], "output.xcstrings");
    assert_eq!(apply_report["dry_run"], false);
    assert_eq!(apply_report["written"], true);
    assert_eq!(
        apply_report["expected_fingerprints"],
        dry_report["expected_fingerprints"]
    );
    assert_eq!(apply_report["fingerprints"], dry_report["fingerprints"]);

    let output = dir.path().join("output.xcstrings");
    let merged: Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    assert_eq!(merged["strings"]["a"]["comment"], "current");
    assert_eq!(merged["strings"]["b"]["comment"], "incoming");
    assert_eq!(
        merged["strings"]["literal.percent"]["localizations"]["en"]["stringUnit"]["value"],
        "Save up to 50% today"
    );
    assert!(
        dir.path()
            .join("output.xcstrings.xcstrings-mcp.lock")
            .is_file()
    );
    assert!(
        !dir.path()
            .join(".output.xcstrings.xcstrings-mcp.tmp")
            .exists()
    );
}

#[test]
fn cli_apply_without_dry_run_fingerprints_is_nonzero_and_protocol_clean() {
    let dir = TempDir::new().unwrap();
    let (base, current, incoming, output) = write_catalogs(&dir);
    cargo_bin_cmd!("xcstrings-mcp")
        .args([
            "--json",
            "merge",
            "--base",
            &base,
            "--current",
            &current,
            "--incoming",
            &incoming,
            "--output",
            &output,
            "--dry-run",
            "false",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("expected_fingerprints"));
    assert!(!std::path::Path::new(&output).exists());
}

#[test]
fn cli_unresolved_conflict_exits_two_without_stdout_or_write() {
    let dir = TempDir::new().unwrap();
    let (base, current, incoming, output) = write_catalogs(&dir);
    std::fs::write(&base, conflict_catalog("base")).unwrap();
    std::fs::write(&current, conflict_catalog("current")).unwrap();
    std::fs::write(&incoming, conflict_catalog("incoming")).unwrap();
    let binary = assert_cmd::cargo::cargo_bin!("xcstrings-mcp");
    let dry = Command::new(binary)
        .args([
            "--json",
            "merge",
            "--base",
            &base,
            "--current",
            &current,
            "--incoming",
            &incoming,
            "--output",
            &output,
        ])
        .output()
        .unwrap();
    assert_eq!(dry.status.code(), Some(2));
    assert!(dry.stderr.is_empty());
    let report: Value = serde_json::from_slice(&dry.stdout).unwrap();
    assert_eq!(report["unresolved_conflict_total"], 1);
    assert_eq!(report["written"], false);
    assert!(!std::path::Path::new(&output).exists());
    let expected = serde_json::to_string(&report["expected_fingerprints"]).unwrap();

    let apply = Command::new(binary)
        .args([
            "--json",
            "merge",
            "--base",
            &base,
            "--current",
            &current,
            "--incoming",
            &incoming,
            "--output",
            &output,
            "--dry-run",
            "false",
            "--expected-fingerprints",
            &expected,
        ])
        .output()
        .unwrap();
    assert_eq!(apply.status.code(), Some(2));
    assert!(apply.stdout.is_empty());
    assert!(String::from_utf8_lossy(&apply.stderr).contains("merge has 1 unresolved conflict"));
    assert!(!std::path::Path::new(&output).exists());
}

#[test]
fn cli_stale_raw_input_exits_one_without_stdout_or_write() {
    let dir = TempDir::new().unwrap();
    let (base, current, incoming, output) = write_catalogs(&dir);
    let binary = assert_cmd::cargo::cargo_bin!("xcstrings-mcp");
    let dry = Command::new(binary)
        .args([
            "--json",
            "merge",
            "--base",
            &base,
            "--current",
            &current,
            "--incoming",
            &incoming,
            "--output",
            &output,
        ])
        .output()
        .unwrap();
    assert!(dry.status.success());
    let report: Value = serde_json::from_slice(&dry.stdout).unwrap();
    let expected = serde_json::to_string(&report["expected_fingerprints"]).unwrap();
    let mut changed_raw = std::fs::read_to_string(&incoming).unwrap();
    changed_raw.push('\n');
    std::fs::write(&incoming, changed_raw).unwrap();

    let apply = Command::new(binary)
        .args([
            "--json",
            "merge",
            "--base",
            &base,
            "--current",
            &current,
            "--incoming",
            &incoming,
            "--output",
            &output,
            "--dry-run",
            "false",
            "--expected-fingerprints",
            &expected,
        ])
        .output()
        .unwrap();
    assert_eq!(apply.status.code(), Some(1));
    assert!(apply.stdout.is_empty());
    assert!(String::from_utf8_lossy(&apply.stderr).contains("stale merge fingerprint: incoming"));
    assert!(!std::path::Path::new(&output).exists());
}

#[test]
fn mcp_router_lists_28_tools_and_merge_returns_structured_content() {
    let dir = TempDir::new().unwrap();
    let (base, current, incoming, output) = write_catalogs(&dir);
    let binary = assert_cmd::cargo::cargo_bin!("xcstrings-mcp");
    let mut child = Command::new(binary)
        .arg("--glossary-path")
        .arg(dir.path().join("glossary.json"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let frames = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"task2-test","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"merge_xcstrings","arguments":{"base_path":base,"current_path":current,"incoming_path":incoming,"output_path":output}}}),
    ];
    {
        let stdin = child.stdin.as_mut().unwrap();
        for frame in frames {
            writeln!(stdin, "{}", serde_json::to_string(&frame).unwrap()).unwrap();
        }
    }
    drop(child.stdin.take());
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    let messages = String::from_utf8(result.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(messages.len(), 3);
    let tools = messages[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 28);
    let merge = tools
        .iter()
        .find(|tool| tool["name"] == "merge_xcstrings")
        .unwrap();
    assert_eq!(
        merge["inputSchema"]["properties"]["dry_run"]["default"],
        true
    );
    assert_eq!(
        merge["inputSchema"]["properties"]["conflict_limit"]["minimum"],
        1
    );
    assert_eq!(
        merge["inputSchema"]["properties"]["conflict_limit"]["maximum"],
        500
    );
    assert!(merge["inputSchema"]["properties"]["expected_fingerprints"].is_object());
    assert!(merge["outputSchema"].is_object());
    assert_eq!(messages[2]["result"]["structuredContent"]["dry_run"], true);
    assert_eq!(messages[2]["result"]["structuredContent"]["written"], false);
    assert!(!std::path::Path::new(&output).exists());
}

#[test]
fn two_cli_processes_applying_expected_absence_have_exactly_one_winner() {
    let dir = TempDir::new().unwrap();
    let (base, current, incoming, output) = write_catalogs(&dir);
    let binary = assert_cmd::cargo::cargo_bin!("xcstrings-mcp");
    let dry = Command::new(binary)
        .args([
            "--json",
            "merge",
            "--base",
            &base,
            "--current",
            &current,
            "--incoming",
            &incoming,
            "--output",
            &output,
        ])
        .output()
        .unwrap();
    assert!(dry.status.success());
    let dry_report: Value = serde_json::from_slice(&dry.stdout).unwrap();
    let expected = serde_json::to_string(&dry_report["expected_fingerprints"]).unwrap();
    let args = [
        "--json",
        "merge",
        "--base",
        &base,
        "--current",
        &current,
        "--incoming",
        &incoming,
        "--output",
        &output,
        "--dry-run",
        "false",
        "--expected-fingerprints",
        &expected,
    ];
    let first = Command::new(binary)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let second = Command::new(binary)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let results = [
        first.wait_with_output().unwrap(),
        second.wait_with_output().unwrap(),
    ];
    assert_eq!(
        results
            .iter()
            .filter(|result| result.status.success())
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| !result.status.success())
            .count(),
        1
    );
    let winner = results
        .iter()
        .find(|result| result.status.success())
        .unwrap();
    let winner_report: Value = serde_json::from_slice(&winner.stdout).unwrap();
    assert_eq!(winner_report["written"], true);
    assert!(winner.stderr.is_empty());
    let loser = results
        .iter()
        .find(|result| !result.status.success())
        .unwrap();
    assert!(loser.stdout.is_empty());
    assert!(String::from_utf8_lossy(&loser.stderr).contains("conditional write conflict"));
    let merged: Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    assert_eq!(merged["strings"]["a"]["comment"], "current");
    assert_eq!(merged["strings"]["b"]["comment"], "incoming");
    assert_eq!(
        merged["strings"]["literal.percent"]["localizations"]["en"]["stringUnit"]["value"],
        "Save up to 50% today"
    );
}

#[test]
fn cli_unused_resolution_error_is_stable_across_processes_and_keeps_stdout_clean() {
    let dir = TempDir::new().unwrap();
    let (base, current, incoming, output) = write_catalogs(&dir);
    let binary = assert_cmd::cargo::cargo_bin!("xcstrings-mcp");

    for _ in 0..20 {
        let result = Command::new(binary)
            .args([
                "--json",
                "merge",
                "--base",
                &base,
                "--current",
                &current,
                "--incoming",
                &incoming,
                "--output",
                &output,
                "--resolution",
                "unknown-first=current",
                "--resolution",
                "unknown-second=incoming",
            ])
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(1));
        assert!(result.stdout.is_empty());
        assert_eq!(
            String::from_utf8(result.stderr).unwrap(),
            "error: invalid format: resolution references unknown conflict unknown-first\n"
        );
        assert!(!std::path::Path::new(&output).exists());
    }
}

#[cfg(unix)]
#[test]
fn final_binary_rejects_lock_alias_apply_and_keeps_same_target_apply_excluded() {
    use std::fs::OpenOptions;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{MetadataExt, symlink};
    use xcstrings_mcp::service::semantic_merge::fingerprint;

    let dir = TempDir::new().unwrap();
    let (base, current, incoming, target) = write_catalogs(&dir);
    let alias = dir.path().join("b.xcstrings");
    let lock = dir.path().join("output.xcstrings.xcstrings-mcp.lock");
    let binary = assert_cmd::cargo::cargo_bin!("xcstrings-mcp");
    let merge = |output: &str, dry_run: bool, expected: Option<&str>| {
        let mut command = Command::new(binary);
        command.args([
            "--json",
            "merge",
            "--base",
            &base,
            "--current",
            &current,
            "--incoming",
            &incoming,
            "--output",
            output,
            "--dry-run",
            if dry_run { "true" } else { "false" },
        ]);
        if let Some(expected) = expected {
            command.args(["--expected-fingerprints", expected]);
        }
        command.output().unwrap()
    };

    let initial_dry = merge(&target, true, None);
    assert!(initial_dry.status.success());
    let initial_report: Value = serde_json::from_slice(&initial_dry.stdout).unwrap();
    let initial_expected = serde_json::to_string(&initial_report["expected_fingerprints"]).unwrap();
    let initial_apply = merge(&target, false, Some(&initial_expected));
    assert!(initial_apply.status.success());
    assert!(initial_apply.stderr.is_empty());

    let repeat_dry = merge(&target, true, None);
    assert!(repeat_dry.status.success());
    let repeat_report: Value = serde_json::from_slice(&repeat_dry.stdout).unwrap();
    let repeat_expected = serde_json::to_string(&repeat_report["expected_fingerprints"]).unwrap();
    let mut alias_expected = repeat_report["expected_fingerprints"].clone();
    alias_expected["output"] = Value::String(fingerprint(b""));
    let alias_expected = serde_json::to_string(&alias_expected).unwrap();

    let held = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock)
        .unwrap();
    assert_eq!(unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX) }, 0);
    let lock_before = std::fs::metadata(&lock).unwrap();
    symlink(&lock, &alias).unwrap();

    let alias_apply = merge(&alias.display().to_string(), false, Some(&alias_expected));
    let mut target_apply = Command::new(binary)
        .args([
            "--json",
            "merge",
            "--base",
            &base,
            "--current",
            &current,
            "--incoming",
            &incoming,
            "--output",
            &target,
            "--dry-run",
            "false",
            "--expected-fingerprints",
            &repeat_expected,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(500));
    let completed_while_held = target_apply.try_wait().unwrap().is_some();
    let target_while_held = std::fs::read(&target).unwrap();
    assert_eq!(unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_UN) }, 0);
    let target_result = target_apply.wait_with_output().unwrap();

    assert_eq!(alias_apply.status.code(), Some(1));
    assert!(alias_apply.stdout.is_empty());
    assert!(
        String::from_utf8(alias_apply.stderr)
            .unwrap()
            .contains("path resolves to a reserved xcstrings-mcp sidecar")
    );
    assert!(!completed_while_held);
    assert_eq!(target_while_held, std::fs::read(&target).unwrap());
    assert!(target_result.status.success());
    assert!(target_result.stderr.is_empty());
    let target_report: Value = serde_json::from_slice(&target_result.stdout).unwrap();
    assert_eq!(target_report["written"], true);
    assert!(
        std::fs::symlink_metadata(&alias)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_link(&alias).unwrap(), lock);
    assert_eq!(std::fs::read(&lock).unwrap(), b"");
    let lock_after = std::fs::metadata(&lock).unwrap();
    assert_eq!(lock_after.dev(), lock_before.dev());
    assert_eq!(lock_after.ino(), lock_before.ino());
}

#[cfg(unix)]
#[test]
fn mcp_reparse_of_retargeted_symlink_lists_only_new_active_identity() {
    use std::os::unix::fs::symlink;

    let dir = TempDir::new().unwrap();
    let first = dir.path().join("first.xcstrings");
    let second = dir.path().join("second.xcstrings");
    let alias = dir.path().join("catalog-alias.xcstrings");
    std::fs::write(
        &first,
        serde_json::to_vec(&json!({
            "sourceLanguage": "en",
            "strings": {"first.a": {}, "first.b": {}},
            "version": "1.0"
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        &second,
        serde_json::to_vec(&json!({
            "sourceLanguage": "en",
            "strings": {"second.a": {}, "second.b": {}, "second.c": {}},
            "version": "1.0"
        }))
        .unwrap(),
    )
    .unwrap();
    symlink(&first, &alias).unwrap();

    let binary = assert_cmd::cargo::cargo_bin!("xcstrings-mcp");
    let mut child = Command::new(binary)
        .arg("--glossary-path")
        .arg(dir.path().join("glossary.json"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let alias_text = alias.display().to_string();
    for frame in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"cache-retarget-test","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"parse_xcstrings","arguments":{"file_path":alias_text}}}),
    ] {
        writeln!(stdin, "{}", serde_json::to_string(&frame).unwrap()).unwrap();
    }
    stdin.flush().unwrap();
    let mut first_lines = Vec::new();
    for _ in 0..2 {
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        first_lines.push(serde_json::from_str::<Value>(&line).unwrap());
    }
    assert_eq!(first_lines[0]["id"], 1);
    assert_eq!(first_lines[1]["id"], 2);

    std::fs::remove_file(&alias).unwrap();
    symlink(&second, &alias).unwrap();
    for frame in [
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"parse_xcstrings","arguments":{"file_path":alias.display().to_string()}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"list_files","arguments":{}}}),
    ] {
        writeln!(stdin, "{}", serde_json::to_string(&frame).unwrap()).unwrap();
    }
    drop(stdin);
    let mut remaining = String::new();
    stdout.read_to_string(&mut remaining).unwrap();
    let messages = remaining
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let status = child.wait().unwrap();
    let mut stderr = Vec::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_end(&mut stderr)
        .unwrap();

    assert!(status.success());
    assert!(stderr.is_empty());
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["id"], 3);
    assert_eq!(messages[1]["id"], 4);
    let list_text = messages[1]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let entries: Value = serde_json::from_str(list_text).unwrap();
    assert_eq!(entries.as_array().unwrap().len(), 1);
    assert_eq!(entries[0]["path"], alias.display().to_string());
    assert_eq!(entries[0]["total_keys"], 3);
    assert_eq!(entries[0]["is_active"], true);
}
