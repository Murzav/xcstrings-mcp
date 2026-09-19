use serde_json::json;
use xcstrings_mcp::service::{parser, xliff};

fn catalog(target_specifier: &str) -> xcstrings_mcp::model::xcstrings::XcStringsFile {
    let source = json!({
        "stringUnit":{"state":"translated","value":"%#@N@"},
        "substitutions":{"N":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{
            "one":{"stringUnit":{"state":"translated","value":"%arg item"}},
            "other":{"stringUnit":{"state":"translated","value":"%arg items"}}
        }}}}
    });
    let mut target = source.clone();
    target["substitutions"]["N"]["formatSpecifier"] = json!(target_specifier);
    parser::parse(&json!({"sourceLanguage":"en","version":"1.0","strings":{"k":{"localizations":{"en":source,"de":target}}}}).to_string()).unwrap()
}

const ROOT: &str = r#"<trans-unit id="k"><source>%1$#@N@</source><target state="translated">%1$#@N@ geändert</target></trans-unit>"#;
const LEAF: &str = r#"<trans-unit id="k|==|substitutions.N.plural.other"><source>%1$lld items</source><target state="translated">%1$@ Dinge</target></trans-unit>"#;

fn import(
    input: &xcstrings_mcp::model::xcstrings::XcStringsFile,
    units: &str,
) -> xcstrings_mcp::model::xliff::XliffImportPlan {
    let document = xliff::parse_document(&format!(r#"<xliff version="1.2"><file source-language="en" target-language="de"><body>{units}</body></file></xliff>"#)).unwrap();
    xliff::plan_import(input, &document, None).unwrap()
}

#[test]
fn root_only_import_uses_existing_target_substitution_type() {
    let input = catalog("@");
    let before = serde_json::to_value(&input).unwrap();
    let plan = import(&input, ROOT);

    assert!(plan.candidate.is_none());
    assert_eq!(plan.report.accepted, 0);
    assert_eq!(plan.report.accepted_destinations, vec![]);
    assert_eq!(plan.report.rejected.len(), 1);
    assert_eq!(plan.report.rejected[0].unit_id, "k");
    assert_eq!(plan.report.rejected[0].code, "format_mismatch");
    assert_eq!(serde_json::to_value(input).unwrap(), before);
}

#[test]
fn combined_import_keeps_source_and_target_substitution_types_separate() {
    let input = catalog("@");
    let plan = import(&input, &format!("{ROOT}{LEAF}"));

    assert!(plan.candidate.is_none());
    assert_eq!(plan.report.accepted, 0);
    assert_eq!(plan.report.accepted_destinations, vec![]);
    assert_eq!(plan.report.rejected.len(), 2);
    assert_eq!(plan.report.rejected[0].unit_id, "k");
    assert_eq!(plan.report.rejected[0].code, "format_mismatch");
    assert_eq!(
        plan.report.rejected[1].unit_id,
        "k|==|substitutions.N.plural.other"
    );
    assert_eq!(plan.report.rejected[1].code, "format_mismatch");
}

#[test]
fn matching_metadata_accepts_root_translation_and_preserves_subtree() {
    let input = catalog("lld");
    let plan = import(&input, ROOT);

    assert_eq!(plan.report.accepted, 1);
    assert_eq!(plan.report.rejected.len(), 0);
    let mut expected = serde_json::to_value(input).unwrap();
    expected["strings"]["k"]["localizations"]["de"]["stringUnit"]["value"] =
        json!("%#@N@ geändert");
    assert_eq!(
        serde_json::to_value(plan.candidate.unwrap()).unwrap(),
        expected
    );
}
