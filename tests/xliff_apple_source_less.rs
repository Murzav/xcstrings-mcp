use serde_json::json;
use xcstrings_mcp::service::{parser, xliff};

#[test]
fn imports_actual_xcode_reexport_with_source_less_existing_plural_case() {
    let input = parser::parse(include_str!("fixtures/apple_xcode27/behavior/new-target-case-existing-context/expected-after-import.xcstrings")).unwrap();
    let document = xliff::parse_document(include_str!(
        "fixtures/apple_xcode27/behavior/new-target-case-existing-context/reexported.xliff"
    ))
    .unwrap();
    let plan = xliff::plan_import(&input, &document, None).unwrap();
    assert_eq!(plan.report.rejected.len(), 0, "{:?}", plan.report.rejected);
    assert_eq!(plan.report.accepted, 6);
    assert_eq!(
        serde_json::to_value(plan.candidate.unwrap()).unwrap(),
        serde_json::to_value(input).unwrap()
    );
}

#[test]
fn source_less_plural_case_requires_an_existing_catalog_leaf() {
    let input =
        parser::parse(r#"{"sourceLanguage":"en","version":"1.0","strings":{"count":{}}}"#).unwrap();
    let doc = xliff::parse_document(r#"<xliff version="1.2"><file target-language="fr"><body><trans-unit id="count|==|plural.many"><target state="translated">%lld articles</target></trans-unit></body></file></xliff>"#).unwrap();
    let plan = xliff::plan_import(&input, &doc, None).unwrap();
    assert!(plan.candidate.is_none());
    assert_eq!(plan.report.accepted, 0);
    assert_eq!(plan.report.rejected[0].code, "missing_source_context");
}

#[test]
fn source_less_literal_variation_suffix_is_not_a_catalog_plural_case() {
    let input = parser::parse(&json!({"sourceLanguage":"en","version":"1.0","strings":{"literal|==|plural.many":{"localizations":{"fr":{"stringUnit":{"state":"translated","value":"old"}}}}}}).to_string()).unwrap();
    let doc = xliff::parse_document(r#"<xliff version="1.2"><file target-language="fr"><body><trans-unit id="literal|==|plural.many"><target state="translated">new</target></trans-unit></body></file></xliff>"#).unwrap();
    let plan = xliff::plan_import(&input, &doc, None).unwrap();
    assert!(plan.candidate.is_none());
    assert_eq!(plan.report.accepted, 0);
    assert_eq!(plan.report.rejected[0].code, "missing_source_context");
}

#[test]
fn exporter_rejects_existing_orphan_substitution_instead_of_emitting_unimportable_units() {
    let input = parser::parse(&json!({"sourceLanguage":"en","version":"1.0","strings":{"count":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"%lld items"}},"fr":{"stringUnit":{"state":"translated","value":"%lld articles"},"substitutions":{"COUNT":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"other":{"stringUnit":{"state":"translated","value":"%arg articles"}}}}}}}}}}}).to_string()).unwrap();
    let error = xliff::export_xliff(&input, "fr", "Localizable.xcstrings", false).unwrap_err();
    assert_eq!(
        error.to_string(),
        "XLIFF format error: substitution 'COUNT' has no reference in a parent stringUnit"
    );
}
