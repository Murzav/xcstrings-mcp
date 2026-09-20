#[path = "support/workflow_fixture.rs"]
mod workflow_fixture;
use serde_json::json;
use xcstrings_mcp::service::{merger, parser, validator};

const CATALOG: &str = r#"{"sourceLanguage":"en","strings":{"birds":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"%lld birds"}},"de":{"future":8,"variations":{"device":{"iphone":{"stringUnit":{"state":"needs_review","value":"%lld Vögel","futureUnit":3}},"other":{"stringUnit":{"state":"translated","value":"%lld Tiere"}}}}}}}},"version":"1.0"}"#;

#[test]
fn explicit_leaf_submission_updates_only_its_destination_and_reports_identity() {
    let mut file = parser::parse(CATALOG).unwrap();
    let translations: Vec<xcstrings_mcp::model::translation::CompletedTranslation> = serde_json::from_value(workflow_fixture::capture(&file,json!([{"key":"birds","locale":"de","value":"%lld Vogelarten","path":[{"device":"iphone"}]}]))).unwrap();

    let result = merger::merge_translations(&mut file, &translations);

    assert_eq!(result.accepted, 1);
    assert_eq!(
        serde_json::to_value(&result).unwrap()["accepted_destinations"],
        json!([{"key":"birds","locale":"de","path":[{"device":"iphone"}]}])
    );
    assert_eq!(
        serde_json::to_value(&file.strings["birds"].localizations.as_ref().unwrap()["de"]).unwrap(),
        json!({"future":8,"variations":{"device":{"iphone":{"stringUnit":{"state":"needs_review","value":"%lld Vogelarten","futureUnit":3}},"other":{"stringUnit":{"state":"translated","value":"%lld Tiere"}}}}})
    );
}

#[test]
fn duplicate_destinations_reject_every_overlapping_request_before_mutation() {
    let mut file = parser::parse(CATALOG).unwrap();
    let before = serde_json::to_string(&file).unwrap();
    let translations: Vec<xcstrings_mcp::model::translation::CompletedTranslation> = serde_json::from_value(workflow_fixture::capture(&file,json!([{"key":"birds","locale":"de","value":"A","path":[{"device":"iphone"}]},{"key":"birds","locale":"de","value":"B","path":[{"device":"iphone"}]}]))).unwrap();

    let result = merger::merge_translations(&mut file, &translations);

    assert_eq!(result.accepted, 0);
    assert_eq!(
        serde_json::to_value(&result.rejected).unwrap(),
        json!([{"key":"birds","reason":"duplicate translation destination","locale":"de","path":[{"device":"iphone"}],"code":"duplicate_destination"},{"key":"birds","reason":"duplicate translation destination","locale":"de","path":[{"device":"iphone"}],"code":"duplicate_destination"}])
    );
    assert_eq!(serde_json::to_string(&file).unwrap(), before);
}

#[test]
fn selector_conflict_and_invalid_path_have_specific_codes() {
    let file = parser::parse(CATALOG).unwrap();
    let translations: Vec<xcstrings_mcp::model::translation::CompletedTranslation> = serde_json::from_value(workflow_fixture::capture(&file,json!([{"key":"birds","locale":"de","value":"X","path":[],"plural_forms":{"other":"X"}},{"key":"birds","locale":"de","value":"X","path":[{"device":"toaster"}]}]))).unwrap();

    let rejected = validator::validate_translations(&file, &translations);

    assert_eq!(rejected.len(), 2);
    let value = serde_json::to_value(rejected).unwrap();
    assert_eq!(value[0]["code"], "conflicting_selectors");
    assert_eq!(value[1]["code"], "invalid_path");
}

#[test]
fn partial_plural_update_is_valid_and_explicit_blank_is_intentional() {
    let file = parser::parse(CATALOG).unwrap();
    let translations: Vec<xcstrings_mcp::model::translation::CompletedTranslation> = serde_json::from_value(workflow_fixture::capture(&file,json!([{"key":"birds","locale":"fr","value":"","plural_forms":{"other":"%lld oiseaux"}},{"key":"birds","locale":"de","value":"","path":[{"device":"iphone"}]}]))).unwrap();

    let rejected = validator::validate_translations(&file, &translations);

    assert_eq!(serde_json::to_value(rejected).unwrap(), json!([]));
}

#[test]
fn absent_substitution_target_keeps_parent_references_new_and_only_submitted_leaf_needs_review() {
    let mut file = parser::parse(&json!({"sourceLanguage":"en","strings":{"count":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"%#@N@","metadata":9},"substitutions":{"N":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"%arg item"}},"other":{"stringUnit":{"state":"translated","value":"%arg items"}}}}}}}}}},"version":"1.0"}).to_string()).unwrap();
    let translations: Vec<xcstrings_mcp::model::translation::CompletedTranslation> = serde_json::from_value(workflow_fixture::capture(&file,json!([{"key":"count","locale":"de","value":"%arg Dinge","path":[{"substitution":"N"},{"plural":"other"}]}]))).unwrap();

    let result = merger::merge_translations(&mut file, &translations);

    assert_eq!(result.accepted, 1);
    let target =
        serde_json::to_value(&file.strings["count"].localizations.as_ref().unwrap()["de"]).unwrap();
    assert_eq!(
        target["stringUnit"],
        json!({"state":"new","value":"%#@N@","metadata":9})
    );
    assert_eq!(
        target["substitutions"]["N"]["variations"]["plural"]["one"]["stringUnit"],
        json!({"state":"new","value":""})
    );
    assert_eq!(
        target["substitutions"]["N"]["variations"]["plural"]["other"]["stringUnit"],
        json!({"state":"needs_review","value":"%arg Dinge"})
    );
}

#[test]
fn root_and_device_requests_overlap_before_either_is_applied() {
    let mut file = parser::parse(CATALOG).unwrap();
    let before = serde_json::to_string(&file).unwrap();
    let requests: Vec<xcstrings_mcp::model::translation::CompletedTranslation> = serde_json::from_value(workflow_fixture::capture(&file,json!([{"key":"birds","locale":"ja","value":"Birds","path":[]},{"key":"birds","locale":"ja","value":"Phone birds","path":[{"device":"iphone"}]}]))).unwrap();

    let result = merger::merge_translations(&mut file, &requests);

    assert_eq!(result.accepted, 0);
    let rejected = serde_json::to_value(result.rejected).unwrap();
    assert_eq!(rejected[0]["code"], "duplicate_destination");
    assert_eq!(rejected[1]["code"], "duplicate_destination");
    assert_eq!(serde_json::to_string(&file).unwrap(), before);
}

#[test]
fn variation_write_cannot_remove_substitution_parent_even_when_new_and_empty() {
    let mut file = parser::parse(&json!({"sourceLanguage":"en","strings":{"count":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Count"}},"de":{"stringUnit":{"state":"new","value":""},"substitutions":{"N":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"other":{"stringUnit":{"state":"new","value":""}}}}}}}}}},"version":"1.0"}).to_string()).unwrap();
    let before = serde_json::to_string(&file).unwrap();
    let requests: Vec<xcstrings_mcp::model::translation::CompletedTranslation> =
        serde_json::from_value(workflow_fixture::capture(
            &file,
            json!([{"key":"count","locale":"de","value":"Zahl","path":[{"device":"iphone"}]}]),
        ))
        .unwrap();

    let result = merger::merge_translations(&mut file, &requests);

    assert_eq!(result.accepted, 0);
    assert_eq!(
        serde_json::to_value(result.rejected).unwrap()[0]["code"],
        "invalid_path"
    );
    assert_eq!(serde_json::to_string(&file).unwrap(), before);
}
