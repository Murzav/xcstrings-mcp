#[path = "support/workflow_fixture.rs"]
mod workflow_fixture;
use serde_json::json;
use xcstrings_mcp::{
    model::{translation::CompletedTranslation, xcstrings::XcStringsFile},
    service::{assessment, coverage, merger, parser, validator},
};

fn global_source() -> XcStringsFile {
    parser::parse(&json!({"sourceLanguage":"en","strings":{"k":{"localizations":{"en":{"substitutions":{"N":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"%arg item"}},"other":{"stringUnit":{"state":"translated","value":"%arg items"}}}}}},"variations":{"device":{"iphone":{"stringUnit":{"state":"translated","value":"%#@N@ phone"}},"other":{"stringUnit":{"state":"translated","value":"%lld other"}}}}}}}},"version":"1.0"}).to_string()).unwrap()
}
fn requests(
    file: &xcstrings_mcp::model::xcstrings::XcStringsFile,
    value: serde_json::Value,
) -> Vec<CompletedTranslation> {
    serde_json::from_value(workflow_fixture::capture(file, value)).unwrap()
}

#[test]
fn root_substitution_can_be_referenced_from_device_leaf() {
    let mut file = global_source();
    let source = file.strings["k"].localizations.as_ref().unwrap()["en"].clone();
    file.strings["k"]
        .localizations
        .as_mut()
        .unwrap()
        .insert("fr".into(), source);
    let translations = requests(
        &file,
        json!([{"key":"k","locale":"fr","path":[{"device":"iphone"}],"value":"%#@N@ téléphone"},{"key":"k","locale":"fr","path":[{"substitution":"N"},{"plural":"other"}],"value":"%arg éléments"}]),
    );

    let rejected = validator::validate_translations(&file, &translations);

    assert_eq!(serde_json::to_value(rejected).unwrap(), json!([]));
}

#[test]
fn missing_target_substitution_initializes_device_parent_references_as_new() {
    let mut file = global_source();
    let translations = requests(
        &file,
        json!([{"key":"k","locale":"fr","path":[{"substitution":"N"},{"plural":"other"}],"value":"%arg éléments"}]),
    );

    let result = merger::merge_translations(&mut file, &translations);

    assert_eq!(result.accepted, 1);
    let target =
        serde_json::to_value(&file.strings["k"].localizations.as_ref().unwrap()["fr"]).unwrap();
    assert_eq!(
        target["variations"]["device"]["iphone"]["stringUnit"],
        json!({"state":"new","value":"%#@N@ phone"})
    );
    assert_eq!(
        target["substitutions"]["N"]["variations"]["plural"]["many"]["stringUnit"],
        json!({"state":"new","value":""})
    );
}

fn invalid_metadata(arg_num: u32, specifier: &str) -> XcStringsFile {
    parser::parse(&json!({"sourceLanguage":"en","strings":{"k":{"localizations":{"de":{"stringUnit":{"state":"translated","value":"%#@N@"},"substitutions":{"N":{"argNum":arg_num,"formatSpecifier":specifier,"variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"%arg item"}},"other":{"stringUnit":{"state":"translated","value":"%arg items"}}}}}}}}}},"version":"1.0"}).to_string()).unwrap()
}
#[test]
fn zero_substitution_argument_is_incomplete_with_diagnostic() {
    let file = invalid_metadata(0, "lld");
    let report = assessment::assess("k", &file.strings["k"], "en", "de");
    assert_eq!(coverage::get_coverage(&file).locales[0].translated, 0);
    assert_eq!(
        serde_json::to_value(&report.diagnostics).unwrap()[0]["code"],
        "invalid_substitution_metadata"
    );
}
#[test]
fn invalid_substitution_specifier_is_incomplete_with_diagnostic() {
    let file = invalid_metadata(1, "not-a-specifier");
    let report = assessment::assess("k", &file.strings["k"], "en", "de");
    assert_eq!(coverage::get_coverage(&file).locales[0].translated, 0);
    assert_eq!(
        serde_json::to_value(&report.diagnostics).unwrap()[0]["code"],
        "invalid_substitution_metadata"
    );
}
#[test]
fn blank_parent_cannot_orphan_existing_substitution_metadata() {
    let file = invalid_metadata(1, "lld");
    let translations = requests(
        &file,
        json!([{"key":"k","locale":"de","path":[],"value":""}]),
    );
    let rejected = validator::validate_translations(&file, &translations);
    assert_eq!(rejected.len(), 1);
    assert!(rejected[0].reason.contains("no reference"));
}

#[test]
fn unreferenced_substitution_is_incomplete_with_diagnostic() {
    let mut file = invalid_metadata(1, "lld");
    file.strings["k"].localizations.as_mut().unwrap()["de"]
        .string_unit
        .as_mut()
        .unwrap()
        .value = "%lld items".into();

    let report = assessment::assess("k", &file.strings["k"], "en", "de");

    assert_eq!(coverage::get_coverage(&file).locales[0].translated, 0);
    assert_eq!(
        serde_json::to_value(&report.diagnostics).unwrap()[0]["code"],
        "invalid_shape"
    );
    assert!(!report.leaves[0].complete);
}
