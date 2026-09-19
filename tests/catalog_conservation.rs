use xcstrings_mcp::service::parser::parse;

const SIMPLE: &str = r#"{"futureRoot":{"x":null},"version":"1.0","strings":{"greeting":{"comment":null,"futureEntry":[2,1],"shouldTranslate":true,"localizations":{"de":{"futureLocale":true,"stringUnit":{"value":"Alt","futureUnit":{"flag":7},"state":"translated"},"variations":null}},"extractionState":null}},"sourceLanguage":"en"}"#;

#[test]
fn catalog_roundtrip_preserves_unknown_fields_defaults_nulls_and_object_order() {
    let catalog = parse(SIMPLE).unwrap();

    assert_eq!(serde_json::to_string(&catalog).unwrap(), SIMPLE);
}

#[test]
fn catalog_leaf_mutation_preserves_unrelated_physical_fields() {
    let mut catalog = parse(SIMPLE).unwrap();
    catalog.strings["greeting"].localizations.as_mut().unwrap()["de"]
        .string_unit
        .as_mut()
        .unwrap()
        .value = "Neu".into();

    assert_eq!(
        serde_json::to_string(&catalog).unwrap(),
        SIMPLE.replace("Alt", "Neu")
    );
}

#[test]
fn catalog_roundtrip_preserves_recursive_device_plural_and_substitution_metadata() {
    let input = r#"{"sourceLanguage":"en","strings":{"items":{"localizations":{"de":{"variations":{"futureAxis":{"value":4},"device":{"iphone":{"variations":{"plural":{"other":{"futureBranch":null,"stringUnit":{"state":"machine_translated","value":"%lld Dinge"}},"one":{"stringUnit":{"state":"translated","value":"Ein Ding"}}}},"futureDevice":9},"other":{"stringUnit":{"state":"translated","value":"Dinge"}}}},"substitutions":{"COUNT":{"futureSub":{"x":1},"argNum":2,"formatSpecifier":"lld","variations":{"plural":{"other":{"stringUnit":{"state":"translated","value":"%arg Dinge"}}},"futureSubAxis":false}}}}}}},"version":"1.0"}"#;

    let catalog = parse(input).unwrap();

    assert_eq!(serde_json::to_string(&catalog).unwrap(), input);
}

#[test]
fn catalog_rejects_duplicate_known_field() {
    let error =
        parse(r#"{"sourceLanguage":"en","sourceLanguage":"de","strings":{},"version":"1.0"}"#)
            .unwrap_err();
    assert!(
        matches!(error, xcstrings_mcp::error::XcStringsError::JsonParse(ref message) if message.contains("duplicate JSON member 'sourceLanguage'"))
    );
}

#[test]
fn catalog_rejects_duplicate_catalog_key() {
    let error = parse(r#"{"sourceLanguage":"en","strings":{"same":{},"same":{}},"version":"1.0"}"#)
        .unwrap_err();
    assert!(
        matches!(error, xcstrings_mcp::error::XcStringsError::JsonParse(ref message) if message.contains("duplicate JSON member 'same'"))
    );
}

#[test]
fn catalog_rejects_duplicate_unknown_nested_metadata() {
    let error = parse(
        r#"{"sourceLanguage":"en","strings":{},"version":"1.0","future":{"nested":{"x":1,"x":2}}}"#,
    )
    .unwrap_err();
    assert!(
        matches!(error, xcstrings_mcp::error::XcStringsError::JsonParse(ref message) if message.contains("duplicate JSON member 'x'"))
    );
}

#[test]
fn catalog_deliberate_null_removal_does_not_reappear() {
    let mut catalog = parse(SIMPLE).unwrap();
    let entry = &mut catalog.strings["greeting"];
    entry.comment = None;
    entry.layout.forget("comment");

    assert_eq!(
        serde_json::to_string(&catalog).unwrap(),
        SIMPLE.replace("\"comment\":null,", "")
    );
}

#[test]
fn catalog_translation_update_preserves_unit_metadata_and_order() {
    let mut catalog = parse(SIMPLE).unwrap();
    let node = &mut catalog.strings["greeting"].localizations.as_mut().unwrap()["de"];
    node.set_translation(
        xcstrings_mcp::model::xcstrings::TranslationState::NeedsReview,
        "Entwurf",
    );

    assert_eq!(
        serde_json::to_string(&catalog).unwrap(),
        SIMPLE
            .replace("Alt", "Entwurf")
            .replace("translated", "needs_review")
    );
}

#[test]
fn catalog_unknown_metadata_array_retains_nested_property_order() {
    let input = r#"{"sourceLanguage":"en","future":[{"z":1,"a":null},[false,{"c":2,"b":3}]],"strings":{},"version":"1.0"}"#;
    let catalog = parse(input).unwrap();

    assert_eq!(serde_json::to_string(&catalog).unwrap(), input);
}

#[test]
fn catalog_required_fields_remain_required_despite_default_constructors() {
    let error = parse(r#"{"strings":{},"version":"1.0"}"#).unwrap_err();
    assert!(
        matches!(error, xcstrings_mcp::error::XcStringsError::JsonParse(ref message) if message.contains("missing field `sourceLanguage`"))
    );
}

#[test]
fn catalog_required_unit_state_does_not_default_on_read() {
    let error = parse(r#"{"sourceLanguage":"en","strings":{"k":{"localizations":{"de":{"stringUnit":{"value":"x"}}}}},"version":"1.0"}"#).unwrap_err();
    assert!(
        matches!(error, xcstrings_mcp::error::XcStringsError::JsonParse(ref message) if message.contains("missing field `state`"))
    );
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config {
        cases: 32,
        rng_seed: proptest::test_runner::RngSeed::Fixed(260919),
        ..proptest::test_runner::Config::default()
    })]
    #[test]
    fn arbitrary_future_metadata_survives_typed_roundtrip(values in proptest::collection::vec(proptest::num::i64::ANY, 0..16), text in ".{0,48}") {
        let input = serde_json::json!({"future":{"z":values,"a":text},"sourceLanguage":"en","strings":{"k":{"shouldTranslate":true,"future":null}},"version":"1.0"});
        let original = serde_json::to_string(&input).unwrap();
        let catalog = parse(&original).unwrap();
        proptest::prop_assert_eq!(serde_json::to_string(&catalog).unwrap(), original);
    }
}

#[test]
fn catalog_merge_preserves_existing_unit_metadata() {
    let mut catalog = parse(SIMPLE).unwrap();
    let translation =
        serde_json::from_value(serde_json::json!({"key":"greeting","locale":"de","value":"Neu"}))
            .unwrap();

    let result = xcstrings_mcp::service::merger::merge_translations(&mut catalog, &[translation]);

    assert_eq!(result.accepted, 1);
    assert_eq!(result.rejected.len(), 0);
    assert_eq!(result.accepted_keys, vec!["greeting"]);
    assert_eq!(
        serde_json::to_string(&catalog).unwrap(),
        SIMPLE.replace("Alt", "Neu")
    );
}

#[test]
fn catalog_rejects_metadata_colliding_with_known_field_on_serialization() {
    let mut catalog = parse(SIMPLE).unwrap();
    catalog
        .extra
        .insert("version".into(), serde_json::json!("future"));

    let error = serde_json::to_string(&catalog).unwrap_err();

    assert_eq!(
        error.to_string(),
        "extra field 'version' conflicts with a known field"
    );
}

#[test]
fn catalog_preserves_optional_nulls_at_localization_variation_and_substitution_levels() {
    let input = r#"{"sourceLanguage":"en","strings":{"k":{"localizations":{"de":{"stringUnit":null,"variations":{"plural":null,"device":null},"substitutions":{"N":{"formatSpecifier":null,"argNum":null,"variations":null}}}}}},"version":"1.0"}"#;

    let catalog = parse(input).unwrap();

    assert_eq!(serde_json::to_string(&catalog).unwrap(), input);
}

#[test]
fn catalog_schema_accepts_omitted_should_translate_with_true_default() {
    let schema = serde_json::to_value(schemars::schema_for!(
        xcstrings_mcp::model::xcstrings::XcStringsFile
    ))
    .unwrap();
    let validator = jsonschema::options().build(&schema).unwrap();
    let input = serde_json::json!({"sourceLanguage":"en","strings":{"k":{}},"version":"1.0"});

    assert!(parse(&input.to_string()).unwrap().strings["k"].should_translate);
    assert!(validator.is_valid(&input));
    assert_eq!(
        schema["$defs"]["StringEntry"]["properties"]["shouldTranslate"]["default"],
        serde_json::json!(true)
    );
    assert!(!validator.is_valid(&serde_json::json!({"strings":{},"version":"1.0"})));
}
