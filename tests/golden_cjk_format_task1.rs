use std::path::PathBuf;

use assert_cmd::Command;

const KNOWN_VALID_CJK_KEYS: [&str; 3] = [
    "%@ • %lld days remaining",
    "%lld days remaining",
    "create_technique.duration.accessibility_value %lld %lld",
];

#[test]
fn golden_known_valid_cjk_entries_have_no_format_errors() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("golden.xcstrings");

    for key in KNOWN_VALID_CJK_KEYS {
        let search = Command::cargo_bin("xcstrings-mcp")
            .expect("binary")
            .args([
                "--json",
                "search",
                key,
                fixture.to_str().expect("fixture path"),
                "--limit",
                "100",
            ])
            .output()
            .expect("search Golden fixture");
        assert!(search.status.success(), "search {key:?}: {search:?}");
        let results: serde_json::Value =
            serde_json::from_slice(&search.stdout).expect("search JSON");
        assert!(
            results
                .as_array()
                .expect("search array")
                .iter()
                .any(|unit| unit["key"] == key),
            "Golden fixture must still contain {key:?}"
        );
    }

    let validation = Command::cargo_bin("xcstrings-mcp")
        .expect("binary")
        .args([
            "--json",
            "validate",
            fixture.to_str().expect("fixture path"),
        ])
        .output()
        .expect("validate Golden fixture");
    let reports: serde_json::Value =
        serde_json::from_slice(&validation.stdout).expect("validation JSON");
    let regressions: Vec<(&str, &str)> = reports
        .as_array()
        .expect("validation reports")
        .iter()
        .flat_map(|report| report["errors"].as_array().expect("errors"))
        .filter_map(|issue| {
            let key = issue["key"].as_str()?;
            let issue_type = issue["issue_type"].as_str()?;
            (KNOWN_VALID_CJK_KEYS.contains(&key) && issue_type.starts_with("format_"))
                .then_some((key, issue_type))
        })
        .collect();

    assert!(
        regressions.is_empty(),
        "CJK format regressions: {regressions:?}"
    );
}
