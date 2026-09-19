use serde_json::json;
use xcstrings_mcp::{
    model::translation::CompletedTranslation,
    service::{merger, parser, validator},
};

fn parse_requests(value: serde_json::Value) -> Vec<CompletedTranslation> {
    serde_json::from_value(value).unwrap()
}

#[test]
fn different_device_leaves_are_independent_requests_in_the_same_key() {
    let mut file = parser::parse(r#"{"sourceLanguage":"en","strings":{"k":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"%lld birds"}}}}},"version":"1.0"}"#).unwrap();
    let requests = parse_requests(
        json!([{"key":"k","locale":"de","path":[{"device":"iphone"}],"value":"%lld Handyvögel"},{"key":"k","locale":"de","path":[{"device":"ipad"}],"value":"%lld Tabletvögel"}]),
    );

    let validation = validator::validate_translations(&file, &requests);
    let result = merger::merge_translations(&mut file, &requests);

    assert!(validation.is_empty());
    assert_eq!(result.accepted, 2);
    assert_eq!(
        serde_json::to_value(result.accepted_destinations).unwrap(),
        json!([{"key":"k","locale":"de","path":[{"device":"iphone"}]},{"key":"k","locale":"de","path":[{"device":"ipad"}]}])
    );
}

#[test]
fn explicit_substitution_path_requires_parent_template_even_with_metadata() {
    let mut file = parser::parse(&json!({"sourceLanguage":"en","strings":{"k":{"localizations":{"en":{"substitutions":{"N":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"other":{"stringUnit":{"state":"translated","value":"%arg birds"}}}}}}}}}},"version":"1.0"}).to_string()).unwrap();
    let before = serde_json::to_string(&file).unwrap();
    let requests = parse_requests(
        json!([{"key":"k","locale":"de","path":[{"substitution":"N"},{"plural":"other"}],"value":"%arg Vögel"}]),
    );

    let result = merger::merge_translations(&mut file, &requests);

    assert_eq!(result.accepted, 0);
    assert_eq!(
        result.rejected[0].code.as_deref(),
        Some("unsupported_shape")
    );
    assert_eq!(serde_json::to_string(&file).unwrap(), before);
}

#[test]
fn parent_substitution_reference_keeps_argument_identity_and_occurrences() {
    let file = parser::parse(&json!({"sourceLanguage":"en","strings":{"k":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"%1$#@A@ %2$#@B@"},"substitutions":{"A":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"other":{"stringUnit":{"state":"translated","value":"%arg birds"}}}}},"B":{"argNum":2,"formatSpecifier":"lld","variations":{"plural":{"other":{"stringUnit":{"state":"translated","value":"%arg nests"}}}}}}}}}},"version":"1.0"}).to_string()).unwrap();
    let requests =
        parse_requests(json!([{"key":"k","locale":"de","path":[],"value":"%2$#@A@ %1$#@B@"}]));

    let rejected = validator::validate_translations(&file, &requests);

    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].code.as_deref(), Some("invalid_path"));
    assert_eq!(
        rejected[0].reason,
        "substitution 'A' reference position disagrees with argNum"
    );
}

#[test]
fn target_only_substitution_parent_uses_metadata_for_typed_format_comparison() {
    let file = parser::parse(&json!({"sourceLanguage":"en","strings":{"k":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"%lld total"}},"fr":{"stringUnit":{"state":"translated","value":"Total %#@COUNT@"},"substitutions":{"COUNT":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"other":{"stringUnit":{"state":"translated","value":"%arg au total"}}}}}}}}}},"version":"1.0"}).to_string()).unwrap();
    let requests = parse_requests(
        json!([{"key":"k","locale":"fr","path":[],"value":"Total %#@COUNT@ modifié"}]),
    );

    let rejected = validator::validate_translations(&file, &requests);

    assert_eq!(serde_json::to_value(rejected).unwrap(), json!([]));
}

#[test]
fn nested_source_state_change_is_not_reported_as_source_text_change() {
    let source = json!({"sourceLanguage":"en","strings":{"k":{"localizations":{"en":{"variations":{"device":{"iphone":{"stringUnit":{"state":"new","value":"Birds","metadata":1}}}}}}}},"version":"1.0"});
    let before = parser::parse(&source.to_string()).unwrap();
    let mut changed = source;
    changed["strings"]["k"]["localizations"]["en"]["variations"]["device"]["iphone"]["stringUnit"]
        ["state"] = json!("translated");
    changed["strings"]["k"]["localizations"]["en"]["variations"]["device"]["iphone"]["stringUnit"]
        ["metadata"] = json!(2);
    let after = parser::parse(&changed.to_string()).unwrap();

    let diff = xcstrings_mcp::service::diff::compute_diff(&before, &after);

    assert!(diff.modified.is_empty());
}

#[test]
fn target_only_substitution_missing_category_uses_existing_other_format_template() {
    let file = parser::parse(&json!({"sourceLanguage":"en","strings":{"k":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"%lld total"}},"fr":{"stringUnit":{"state":"translated","value":"Total %#@COUNT@"},"substitutions":{"COUNT":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"other":{"stringUnit":{"state":"translated","value":"%arg au total"}}}}}}}}}},"version":"1.0"}).to_string()).unwrap();
    let requests = parse_requests(
        json!([{"key":"k","locale":"fr","path":[{"substitution":"COUNT"},{"plural":"many"}],"value":"%arg éléments"}]),
    );

    let rejected = validator::validate_translations(&file, &requests);

    assert_eq!(serde_json::to_value(rejected).unwrap(), json!([]));
}

#[test]
fn legacy_substitution_request_cannot_create_an_orphan_subtree() {
    let mut file = parser::parse(&json!({"sourceLanguage":"en","strings":{"k":{"localizations":{"en":{"substitutions":{"N":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"other":{"stringUnit":{"state":"translated","value":"%arg birds"}}}}}}}}}},"version":"1.0"}).to_string()).unwrap();
    let before = serde_json::to_string(&file).unwrap();
    let requests = parse_requests(
        json!([{"key":"k","locale":"de","value":"","substitution_name":"N","plural_forms":{"other":"%arg Vögel"}}]),
    );

    let result = merger::merge_translations(&mut file, &requests);

    assert_eq!(result.accepted, 0);
    assert_eq!(result.rejected[0].code.as_deref(), Some("invalid_path"));
    assert!(result.rejected[0].reason.contains("no reference"));
    assert_eq!(serde_json::to_string(&file).unwrap(), before);
}
