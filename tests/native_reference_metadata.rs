use serde_json::json;
use xcstrings_mcp::{
    model::translation::CompletedTranslation,
    service::{file_validator, parser, validator},
};

fn catalog(
    source: serde_json::Value,
    target: serde_json::Value,
) -> xcstrings_mcp::model::xcstrings::XcStringsFile {
    parser::parse(&json!({"sourceLanguage":"en","strings":{"k":{"localizations":{"en":source,"de":target}}},"version":"1.0"}).to_string()).unwrap()
}

fn substitution(position: u32, specifier: &str) -> serde_json::Value {
    json!({"argNum":position,"formatSpecifier":specifier,"variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"%arg item"}},"other":{"stringUnit":{"state":"translated","value":"%arg items"}}}}})
}

#[test]
fn existing_target_metadata_cannot_hide_format_type_mismatch() {
    let file = catalog(
        json!({"stringUnit":{"state":"translated","value":"%#@N@"},"substitutions":{"N":substitution(1,"lld")}}),
        json!({"stringUnit":{"state":"translated","value":"%#@N@"},"substitutions":{"N":substitution(1,"@")}}),
    );
    let request: CompletedTranslation =
        serde_json::from_value(json!({"key":"k","locale":"de","path":[],"value":"%#@N@ neu"}))
            .unwrap();

    let rejected = validator::validate_translations(&file, &[request]);
    let reports = file_validator::validate_file(&file, Some("de"));

    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0].code.as_deref(), Some("invalid_translation"));
    assert_eq!(reports[0].errors.len(), 1);
    assert_eq!(
        reports[0].errors[0].issue_type,
        "format_specifier_type_mismatch"
    );
}

#[test]
fn target_reference_positions_use_target_metadata_independently() {
    let file = catalog(
        json!({"stringUnit":{"state":"translated","value":"%1$#@N@ %2$#@M@"},"substitutions":{"N":substitution(1,"lld"),"M":substitution(2,"lld")}}),
        json!({"stringUnit":{"state":"translated","value":"%1$#@M@ %2$#@N@"},"substitutions":{"N":substitution(2,"lld"),"M":substitution(1,"lld")}}),
    );
    let request: CompletedTranslation = serde_json::from_value(
        json!({"key":"k","locale":"de","path":[],"value":"%1$#@M@ mit %2$#@N@"}),
    )
    .unwrap();

    let rejected = validator::validate_translations(&file, &[request]);
    let reports = file_validator::validate_file(&file, Some("de"));

    assert_eq!(serde_json::to_value(rejected).unwrap(), json!([]));
    assert_eq!(serde_json::to_value(&reports[0].errors).unwrap(), json!([]));
}
