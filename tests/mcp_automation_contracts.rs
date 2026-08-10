use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use regex::Regex;
use serde_json::{Value, json};
use tempfile::TempDir;

fn run_mcp(frames: &[Value]) -> Vec<Value> {
    let dir = TempDir::new().unwrap();
    let binary = assert_cmd::cargo::cargo_bin!("xcstrings-mcp");
    let mut child = Command::new(binary)
        .arg("--glossary-path")
        .arg(dir.path().join("glossary.json"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().unwrap();
        for frame in frames {
            writeln!(stdin, "{}", serde_json::to_string(frame).unwrap()).unwrap();
        }
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "server failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "MCP stderr was not empty: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn initialize_frame() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2026-07-28",
            "capabilities": {},
            "clientInfo": {"name": "automation-contract-test", "version": "1"}
        }
    })
}

fn mcp_workflow_fences(markdown: &str) -> String {
    let mut workflows = String::new();
    let mut in_fence = false;
    let mut include_fence = false;

    for line in markdown.lines() {
        let trimmed = line.trim();
        if let Some(info) = trimmed.strip_prefix("```") {
            if in_fence {
                in_fence = false;
                include_fence = false;
            } else {
                in_fence = true;
                // MCP workflows are deliberately unlabeled (or explicitly `mcp`).
                // Language-labeled examples such as `swift` contain host built-ins
                // (`Read`, `Bash`, `Edit`) and are not MCP invocations.
                include_fence = matches!(info.trim(), "" | "mcp");
            }
        } else if include_fence {
            workflows.push_str(line);
            workflows.push('\n');
        }
    }

    workflows
}

fn live_tool_schemas() -> BTreeMap<String, Value> {
    let responses = run_mcp(&[
        initialize_frame(),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    ]);
    let tools = responses[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 28);

    tools
        .iter()
        .map(|tool| {
            (
                tool["name"].as_str().unwrap().to_string(),
                tool["inputSchema"].clone(),
            )
        })
        .collect()
}

fn validate_skill_examples(skill: &str, schemas: &BTreeMap<String, Value>) -> (usize, Vec<String>) {
    let invocation =
        Regex::new(r"(?ms)^[\t ]*(?:→[\t ]*)?(?P<tool>[a-z][a-z0-9_]*)\((?P<arguments>[^)]*)\)")
            .unwrap();
    let workflows = mcp_workflow_fences(skill);
    let mut count = 0;
    let mut violations = Vec::new();

    for captures in invocation.captures_iter(&workflows) {
        count += 1;
        let tool = captures.name("tool").unwrap().as_str();
        let Some(schema) = schemas.get(tool) else {
            violations.push(format!("unknown MCP tool `{tool}`"));
            continue;
        };
        let arguments = captures["arguments"].trim();
        let Ok(arguments) = serde_json::from_str::<Value>(arguments) else {
            violations.push(format!(
                "{tool} arguments are not one executable JSON object: `{}`",
                arguments
            ));
            continue;
        };
        let validator = jsonschema::options()
            .build(schema)
            .unwrap_or_else(|error| panic!("invalid live schema for {tool}: {error}"));
        for error in validator.iter_errors(&arguments) {
            violations.push(format!("{tool} arguments violate live schema: {error}"));
        }
    }

    (count, violations)
}

fn live_merge_dry_run_contract() -> (String, String) {
    let dir = TempDir::new().unwrap();
    let paths = ["base.xcstrings", "current.xcstrings", "incoming.xcstrings"]
        .map(|name| dir.path().join(name));
    for (path, comment) in paths.iter().zip(["base", "current", "incoming"]) {
        let catalog = json!({
            "sourceLanguage":"en",
            "strings":{"button.save":{"comment":comment}},
            "version":"1.0"
        });
        fs::write(path, serde_json::to_vec(&catalog).unwrap()).unwrap();
    }
    let output = dir.path().join("merged.xcstrings");
    let responses = run_mcp(&[
        initialize_frame(),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({
            "jsonrpc":"2.0",
            "id":50,
            "method":"tools/call",
            "params":{
                "name":"merge_xcstrings",
                "arguments":{
                    "base_path":paths[0],
                    "current_path":paths[1],
                    "incoming_path":paths[2],
                    "output_path":output,
                    "dry_run":true
                }
            }
        }),
    ]);
    let report = &responses[1]["result"]["structuredContent"];
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["written"], false);
    let conflict_id = report["conflicts"][0]["id"].as_str().unwrap();
    let fingerprint = report["expected_fingerprints"]["base"].as_str().unwrap();
    assert!(conflict_id.starts_with("merge-v1:"));
    assert!(fingerprint.starts_with("sha256:"));
    assert_eq!(conflict_id.len(), "merge-v1:".len() + 64);
    assert_eq!(fingerprint.len(), "sha256:".len() + 64);
    ("merge-v1:".to_string(), "sha256:".to_string())
}

fn validate_merge_apply_semantics(
    skill: &str,
    conflict_prefix: &str,
    fingerprint_prefix: &str,
) -> Vec<String> {
    let invocation =
        Regex::new(r"(?ms)^[\t ]*(?:→[\t ]*)?merge_xcstrings\((?P<arguments>[^)]*)\)").unwrap();
    let workflows = mcp_workflow_fences(skill);
    let apply = invocation
        .captures_iter(&workflows)
        .filter_map(|captures| serde_json::from_str::<Value>(captures["arguments"].trim()).ok())
        .find(|arguments| arguments["dry_run"] == false)
        .expect("shipped merge apply example");
    let mut violations = Vec::new();
    let conflict_id = apply["resolutions"][0]["conflict_id"]
        .as_str()
        .unwrap_or_default();
    if !conflict_id.starts_with(conflict_prefix) || !conflict_id.contains("copy exact") {
        violations.push("merge apply must copy an exact dry-run conflict ID".to_string());
    }
    for field in ["base", "current", "incoming"] {
        let fingerprint = apply["expected_fingerprints"][field]
            .as_str()
            .unwrap_or_default();
        if !fingerprint.starts_with(fingerprint_prefix) || !fingerprint.contains("copy exact") {
            violations.push(format!(
                "merge apply must copy the exact dry-run {field} fingerprint"
            ));
        }
    }
    if !apply["expected_fingerprints"]["output"].is_null() {
        violations.push("new-output example must copy dry-run output=null".to_string());
    }
    violations
}

#[test]
fn shipped_skill_examples_match_live_tool_schemas() {
    let schemas = live_tool_schemas();
    let skill = include_str!("../skills/xcstrings-mcp/SKILL.md");
    let (count, violations) = validate_skill_examples(skill, &schemas);

    assert_eq!(count, 74, "shipped MCP invocation coverage changed");
    assert!(
        violations.is_empty(),
        "skill examples drifted from live tools/list schemas:\n{}",
        violations.join("\n")
    );
}

#[test]
fn shipped_skill_schema_guard_rejects_value_and_nested_shape_mutations() {
    let schemas = live_tool_schemas();
    let skill = include_str!("../skills/xcstrings-mcp/SKILL.md");
    let valid = r#"submit_translations({"translations":[{"key":"button.save","locale":"uk","value":"Зберегти"}]})"#;
    let mutations = [
        r#"submit_translations({"translations":{"key":"button.save","locale":"uk","value":"Зберегти"}})"#,
        r#"submit_translations({"translations":"invalid"})"#,
        r#"submit_translations({"translations":[{"locale":"uk","value":"Зберегти"}]})"#,
        r#"submit_translations({"translations":[{"key":7,"locale":"uk","value":"Зберегти"}]})"#,
    ];

    assert!(
        skill.contains(valid),
        "mutation anchor missing from shipped skill"
    );
    for mutation in mutations {
        let mutated = skill.replacen(valid, mutation, 1);
        let (_, violations) = validate_skill_examples(&mutated, &schemas);
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("submit_translations")),
            "schema guard accepted mutation: {mutation}"
        );
    }
}

#[test]
fn shipped_merge_apply_copies_live_dry_run_identifiers_and_fingerprints() {
    let (conflict_prefix, fingerprint_prefix) = live_merge_dry_run_contract();
    let violations = validate_merge_apply_semantics(
        include_str!("../skills/xcstrings-mcp/SKILL.md"),
        &conflict_prefix,
        &fingerprint_prefix,
    );
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn shipped_merge_semantic_guard_rejects_wrong_identifier_prefixes() {
    let (conflict_prefix, fingerprint_prefix) = live_merge_dry_run_contract();
    let skill = include_str!("../skills/xcstrings-mcp/SKILL.md");
    assert!(
        validate_merge_apply_semantics(skill, &conflict_prefix, &fingerprint_prefix).is_empty(),
        "mutation baseline must be valid"
    );
    let cases = [("merge-v1:", "translation:"), ("sha256:", "")];
    for (valid, invalid) in cases {
        let mutated = skill.replacen(valid, invalid, 1);
        let violations =
            validate_merge_apply_semantics(&mutated, &conflict_prefix, &fingerprint_prefix);
        assert!(!violations.is_empty(), "semantic guard accepted {invalid}");
    }
}

#[test]
fn readme_cli_command_table_has_no_interleaved_prose() {
    let readme = include_str!("../README.md");
    let section = readme.split("## CLI Commands").nth(1).unwrap();
    let table = section.split("### CLI Options").next().unwrap();
    let normalization = table.find("XLIFF unit IDs are compared").unwrap();
    let first_row = table.find("| Command | Description |").unwrap();
    let final_row = table.find("| `completions <shell>`").unwrap();
    let final_row_end = table[final_row..].find('\n').unwrap() + final_row;
    assert!(
        normalization > final_row,
        "CLI command table contains prose before all rows are complete"
    );
    assert!(
        table[first_row..final_row_end]
            .lines()
            .all(|line| line.starts_with('|')),
        "CLI command rows are not one contiguous Markdown table"
    );
}

#[test]
fn all_eight_prompts_accept_mcp_string_arguments_over_stdio() {
    let cases = [
        (
            "add_language",
            json!({"locale":"uk","file_path":"/tmp/Localizable.xcstrings"}),
        ),
        (
            "cleanup_stale",
            json!({"file_path":"/tmp/Localizable.xcstrings"}),
        ),
        (
            "extract_strings",
            json!({"source_language":"en","file_path":"/tmp/Localizable.xcstrings"}),
        ),
        ("fix_validation_errors", json!({"locale":"de"})),
        (
            "full_translate",
            json!({"locale":"ja","file_path":"/tmp/Localizable.xcstrings"}),
        ),
        ("localization_audit", json!({"locale":"fr"})),
        ("review_translations", json!({"locale":"es"})),
        ("translate_batch", json!({"locale":"uk","count":"37"})),
    ];
    let mut frames = vec![
        initialize_frame(),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"prompts/list","params":{}}),
    ];
    frames.extend(cases.iter().enumerate().map(|(index, (name, arguments))| {
        json!({
            "jsonrpc": "2.0",
            "id": index + 10,
            "method": "prompts/get",
            "params": {"name": name, "arguments": arguments}
        })
    }));

    let responses = run_mcp(&frames);
    let listed = responses[1]["result"]["prompts"].as_array().unwrap();
    assert_eq!(listed.len(), 8);
    assert_eq!(responses.len(), 10);

    for (index, (name, _)) in cases.iter().enumerate() {
        let response = &responses[index + 2];
        assert_eq!(response["id"], index + 10, "wrong response for {name}");
        assert!(
            response.get("error").is_none(),
            "{name} rejected protocol arguments: {}",
            response["error"]
        );
        assert_eq!(response["result"]["messages"].as_array().unwrap().len(), 1);
    }

    let review_text = responses[8]["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap();
    assert!(review_text.contains("Call get_coverage to see overall progress"));
    assert!(!review_text.contains("get_coverage with locale="));

    let translate_text = responses[9]["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap();
    assert!(translate_text.contains("batch_size=37"));

    let cleanup_text = responses[3]["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap();
    assert!(cleanup_text.contains("Call list_locales and choose the locale"));
    assert!(cleanup_text.contains("get_stale(locale=\"<locale>\", batch_size=100)"));
    assert!(!cleanup_text.contains("get_stale(batch_size=100)"));
}

#[test]
fn cleanup_stale_prompt_call_replays_over_live_stdio_without_schema_error() {
    let fixture = format!(
        "{}/tests/fixtures/with_stale.xcstrings",
        env!("CARGO_MANIFEST_DIR")
    );
    let responses = run_mcp(&[
        initialize_frame(),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({
            "jsonrpc":"2.0",
            "id":40,
            "method":"prompts/get",
            "params":{
                "name":"cleanup_stale",
                "arguments":{"file_path":"/tmp/Localizable.xcstrings"}
            }
        }),
        json!({
            "jsonrpc":"2.0",
            "id":41,
            "method":"tools/call",
            "params":{"name":"parse_xcstrings","arguments":{"file_path":fixture}}
        }),
        json!({
            "jsonrpc":"2.0",
            "id":42,
            "method":"tools/call",
            "params":{
                "name":"get_stale",
                "arguments":{"locale":"uk","batch_size":100}
            }
        }),
    ]);

    let prompt = responses[1]["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap();
    assert!(prompt.contains("get_stale(locale=\"<locale>\", batch_size=100)"));
    assert_eq!(responses[2]["result"]["isError"], false);
    assert_eq!(responses[3]["result"]["isError"], false);
    let replay = responses[3].to_string();
    assert!(
        !replay.contains("failed to deserialize parameters"),
        "{replay}"
    );
    assert!(!replay.contains("missing field `locale`"), "{replay}");
}

#[test]
fn translate_batch_rejects_invalid_string_counts_deterministically() {
    let cases = [
        ("many", "count must be an integer in 1..=100, got \"many\""),
        ("0", "count must be in 1..=100, got 0"),
        ("101", "count must be in 1..=100, got 101"),
    ];
    let mut frames = vec![
        initialize_frame(),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    ];
    frames.extend(cases.iter().enumerate().map(|(index, (count, _))| {
        json!({
            "jsonrpc": "2.0",
            "id": index + 20,
            "method": "prompts/get",
            "params": {
                "name": "translate_batch",
                "arguments": {"locale":"uk","count":count}
            }
        })
    }));

    let responses = run_mcp(&frames);
    assert_eq!(responses.len(), 4);
    for (index, (_, expected_message)) in cases.iter().enumerate() {
        let error = &responses[index + 1]["error"];
        assert_eq!(error["code"], -32602);
        assert_eq!(error["message"], *expected_message);
        assert!(error.get("data").is_none());
    }
}

#[test]
fn translate_batch_accepts_string_count_boundaries() {
    let frames = [
        initialize_frame(),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({
            "jsonrpc":"2.0",
            "id":30,
            "method":"prompts/get",
            "params":{"name":"translate_batch","arguments":{"locale":"uk","count":"1"}}
        }),
        json!({
            "jsonrpc":"2.0",
            "id":31,
            "method":"prompts/get",
            "params":{"name":"translate_batch","arguments":{"locale":"uk","count":"100"}}
        }),
    ];

    let responses = run_mcp(&frames);
    assert_eq!(responses.len(), 3);
    for (response, expected_count) in responses[1..].iter().zip([1, 100]) {
        assert!(response.get("error").is_none(), "{}", response["error"]);
        let text = response["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap();
        assert!(text.contains(&format!("batch_size={expected_count}")));
    }
}
