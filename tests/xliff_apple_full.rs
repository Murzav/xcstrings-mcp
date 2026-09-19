use serde_json::json;
use xcstrings_mcp::service::{parser, xliff};

fn catalog(value: serde_json::Value) -> xcstrings_mcp::XcStringsFile {
    parser::parse(&value.to_string()).unwrap()
}
fn document(units: &str) -> String {
    format!(
        r#"<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2"><file original="Localizable.xcstrings" source-language="en" target-language="fr"><body>{units}</body></file></xliff>"#
    )
}
#[test]
fn imports_new_target_plural_without_flattening_source() {
    let input = catalog(
        json!({"sourceLanguage":"en","version":"1.0","strings":{"count":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"%lld count"}}}}}}),
    );
    let xml = document(
        r#"<trans-unit id="count|==|plural.one"><source>%lld count</source><target state="translated">%lld article</target></trans-unit>"#,
    );
    let plan = xliff::plan_import(&input, &xliff::parse_document(&xml).unwrap(), None).unwrap();
    assert_eq!(plan.report.accepted, 1);
    let actual = serde_json::to_value(plan.candidate.unwrap()).unwrap();
    assert_eq!(
        actual["strings"]["count"]["localizations"]["fr"],
        json!({"variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"%lld article"}}}}})
    );
    assert_eq!(
        actual["strings"]["count"]["localizations"]["en"],
        json!({"stringUnit":{"state":"translated","value":"%lld count"}})
    );
}
#[test]
fn preserves_drafts_machine_state_and_explicit_blank() {
    let input = catalog(
        json!({"sourceLanguage":"en","version":"1.0","strings":{"draft":{},"machine":{},"blank":{},"missing":{}}}),
    );
    let xml = document(
        r#"<trans-unit id="draft"><source>draft</source><target state="needs-review-l10n">brouillon</target></trans-unit><trans-unit id="machine"><source>machine</source><target state="translated" state-qualifier="leveraged-mt">machine cible</target></trans-unit><trans-unit id="blank"><source>blank</source><target state="translated"/></trans-unit><trans-unit id="missing"><source>missing</source></trans-unit>"#,
    );
    let plan = xliff::plan_import(&input, &xliff::parse_document(&xml).unwrap(), None).unwrap();
    assert_eq!((plan.report.accepted, plan.report.missing_targets), (3, 1));
    let v = serde_json::to_value(plan.candidate.unwrap()).unwrap();
    assert_eq!(
        v["strings"]["draft"]["localizations"]["fr"]["stringUnit"],
        json!({"state":"needs_review","value":"brouillon"})
    );
    assert_eq!(
        v["strings"]["machine"]["localizations"]["fr"]["stringUnit"],
        json!({"state":"machine_translated","value":"machine cible"})
    );
    assert_eq!(
        v["strings"]["blank"]["localizations"]["fr"]["stringUnit"],
        json!({"state":"translated","value":""})
    );
    assert_eq!(v["strings"]["missing"], json!({}));
}
#[test]
fn rejects_ambiguous_literal_key_and_new_variation_atomically() {
    let input = catalog(
        json!({"sourceLanguage":"en","version":"1.0","strings":{"count":{},"count|==|plural.one":{},"valid":{}}}),
    );
    let xml = document(
        r#"<trans-unit id="valid"><source>valid</source><target>bon</target></trans-unit><trans-unit id="count|==|plural.one"><source>count</source><target>un</target></trans-unit>"#,
    );
    let plan = xliff::plan_import(&input, &xliff::parse_document(&xml).unwrap(), None).unwrap();
    assert!(plan.candidate.is_none());
    assert_eq!(plan.report.accepted, 0);
    assert_eq!(plan.report.rejected[0].code, "ambiguous_destination");
}
#[test]
fn scopes_duplicate_ids_to_explicit_original() {
    let xml = include_str!("fixtures/apple_xcode27/positive/multiple-catalogs/exported.xliff");
    let input = parser::parse(include_str!(
        "fixtures/apple_xcode27/positive/multiple-catalogs/source.xcstrings"
    ))
    .unwrap();
    let doc = xliff::parse_document(xml).unwrap();
    let plan = xliff::plan_import(&input, &doc, Some("Custom.xcstrings")).unwrap();
    assert_eq!(plan.report.skipped_scopes, vec!["Localizable.xcstrings"]);
    assert_eq!(plan.report.accepted, 1);
    assert_eq!(
        serde_json::to_value(plan.candidate.unwrap()).unwrap()["strings"]["shared"]["localizations"]
            ["fr"]["stringUnit"]["value"],
        "CUSTOM cible"
    );
    assert!(xliff::plan_import(&input, &doc, None).is_err());
}
#[test]
fn exports_real_apple_variation_ids_and_control_attributes() {
    let mut input = parser::parse(include_str!(
        "fixtures/apple_xcode27/positive/catalog-matrix/source.xcstrings"
    ))
    .unwrap();
    input.strings.shift_remove("ambiguous|==|plural.one");
    let (xml, count) = xliff::export_xliff(&input, "fr", "Localizable.xcstrings", false).unwrap();
    let doc = xliff::parse_document(&xml).unwrap();
    assert_eq!(count, doc.files[0].units.len());
    let units = &doc.files[0].units;
    assert!(
        units
            .iter()
            .any(|u| u.id == "chained|==|device.iphone.plural.one")
    );
    assert!(
        units
            .iter()
            .any(|u| u.id == "multi|==|substitutions.YARDS.plural.other"
                && u.target.as_deref() == Some("FR yards %2$lld many"))
    );
    assert!(xml.contains("key&#10;LF"));
    assert!(
        units
            .iter()
            .any(|u| u.id == "sub_DOT.NAME|==|substitutions.@DOT.NAME@.plural.one")
    );
}

#[test]
fn imports_complete_oracle_without_losing_any_native_metadata() {
    let input = parser::parse(include_str!(
        "fixtures/apple_xcode27/positive/catalog-matrix/source.xcstrings"
    ))
    .unwrap();
    let xml = include_str!("fixtures/apple_xcode27/positive/catalog-matrix/exported.xliff");
    let doc = xliff::parse_document(xml).unwrap();
    let plan = xliff::plan_import(&input, &doc, None).unwrap();
    assert_eq!(plan.report.rejected.len(), 0, "{:?}", plan.report.rejected);
    let candidate = plan.candidate.unwrap();
    let value = serde_json::to_value(&candidate).unwrap();
    assert_eq!(value["strings"]["auto"]["isCommentAutoGenerated"], true);
    assert_eq!(value["strings"]["auto"]["commentGenerationVersion"], "1");
    assert_eq!(
        value["strings"]["multi"]["localizations"]["fr"]["substitutions"]["YARDS"]["argNum"],
        2
    );
    assert_eq!(
        value["strings"]["multi"]["localizations"]["fr"]["substitutions"]["BIRDS"]["argNum"],
        1
    );
    let again = xliff::plan_import(&candidate, &doc, None).unwrap();
    assert_eq!(
        serde_json::to_value(again.candidate.unwrap()).unwrap(),
        value
    );
}

#[test]
fn constructs_new_substitution_using_positional_evidence() {
    let input=parser::parse(include_str!("fixtures/apple_xcode27/negative/new-target-substitution-loss/destination-before-import.xcstrings")).unwrap();
    let doc = xliff::parse_document(include_str!(
        "fixtures/apple_xcode27/negative/new-target-substitution-loss/exported.xliff"
    ))
    .unwrap();
    let plan = xliff::plan_import(&input, &doc, None).unwrap();
    assert_eq!(plan.report.rejected.len(), 0, "{:?}", plan.report.rejected);
    let value = serde_json::to_value(plan.candidate.unwrap()).unwrap();
    assert_eq!(
        value["strings"]["target_only_substitution"]["localizations"]["fr"],
        json!({"stringUnit":{"state":"translated","value":"Total %#@COUNT@"},"substitutions":{"COUNT":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"fr %arg single"}},"other":{"stringUnit":{"state":"translated","value":"fr %arg multiple"}}}}}}})
    );
}

#[test]
fn rejects_malformed_suffix_and_invalid_batch_without_mutation() {
    let input =
        catalog(json!({"sourceLanguage":"en","version":"1.0","strings":{"count":{},"valid":{}}}));
    let before = serde_json::to_value(&input).unwrap();
    let xml = document(
        r#"<trans-unit id="valid"><source>valid</source><target>bon</target></trans-unit><trans-unit id="count|==|plural.one."><source>count</source><target>un</target></trans-unit>"#,
    );
    let plan = xliff::plan_import(&input, &xliff::parse_document(&xml).unwrap(), None).unwrap();
    assert!(plan.candidate.is_none());
    assert_eq!(plan.report.accepted, 0);
    assert!(plan.report.accepted_destinations.is_empty());
    assert_eq!(plan.report.rejected[0].code, "unsupported_destination");
    assert_eq!(serde_json::to_value(input).unwrap(), before);
}

macro_rules! rejects_unit {
    ($name:ident,$entry:expr,$unit:expr,$code:literal)=>{
        #[test]
        fn $name(){
            let input=catalog(json!({"sourceLanguage":"en","version":"1.0","strings":{"item":$entry}}));
            let doc=xliff::parse_document(&document($unit)).unwrap();
            let plan=xliff::plan_import(&input,&doc,None).unwrap();
            assert!(plan.candidate.is_none());
            assert_eq!(plan.report.accepted,0);
            assert!(plan.report.accepted_destinations.is_empty());
            assert_eq!(plan.report.rejected[0].code,$code);
        }
    }
}
rejects_unit!(
    rejects_nontranslatable_key,
    json!({"shouldTranslate":false}),
    r#"<trans-unit id="item"><source>item</source><target>article</target></trans-unit>"#,
    "not_translatable"
);
rejects_unit!(
    rejects_unsupported_state,
    json!({}),
    r#"<trans-unit id="item"><source>item</source><target state="vendor-state">article</target></trans-unit>"#,
    "unsupported_state"
);
rejects_unit!(
    rejects_unknown_key,
    json!({}),
    r#"<trans-unit id="absent"><source>item</source><target>article</target></trans-unit>"#,
    "unknown_key"
);
rejects_unit!(
    rejects_incompatible_parent_and_plural,
    json!({}),
    r#"<trans-unit id="item"><source>item</source><target>article</target></trans-unit><trans-unit id="item|==|plural.one"><source>item</source><target>un</target></trans-unit>"#,
    "overlapping_destinations"
);
rejects_unit!(
    rejects_new_substitution_without_parent_evidence,
    json!({}),
    r#"<trans-unit id="item|==|substitutions.COUNT.plural.one"><source>item</source><target>%1$lld article</target></trans-unit>"#,
    "substitution_metadata"
);
rejects_unit!(
    rejects_lost_format_argument,
    json!({"localizations":{"en":{"stringUnit":{"state":"translated","value":"%lld item"}}}}),
    r#"<trans-unit id="item"><source>%lld item</source><target>article</target></trans-unit>"#,
    "format_mismatch"
);
rejects_unit!(
    rejects_write_over_unknown_axis,
    json!({"localizations":{"fr":{"variations":{"future":{"other":{"stringUnit":{"state":"translated","value":"future"}}}}}}}),
    r#"<trans-unit id="item|==|plural.one"><source>item</source><target>article</target></trans-unit>"#,
    "shape_conflict"
);

#[test]
fn retains_source_target_notes_and_file_scope_in_xml_ir() {
    let doc=xliff::parse_document(&document(r#"<trans-unit id="item"><source> source &amp; value </source><target state="translated" state-qualifier="leveraged-mt"> cible </target><note from="auto-generated"> context </note></trans-unit>"#)).unwrap();
    assert_eq!(
        serde_json::to_value(doc).unwrap(),
        json!({"files":[{"original":"Localizable.xcstrings","source_language":"en","target_language":"fr","units":[{"id":"item","source":" source & value ","target":" cible ","state":"translated","state_qualifier":"leveraged-mt","notes":[{"text":" context ","from":"auto-generated"}]}]}]})
    );
}

#[test]
fn refuses_xml_placeholder_with_unrepresentable_attribute_semantics() {
    let xml = document(
        r#"<trans-unit id="item"><source>item</source><target>value <x id="argument" equiv-text="%@"/></target></trans-unit>"#,
    );
    let error = xliff::parse_document(&xml).unwrap_err();
    assert_eq!(
        error.to_string(),
        "XLIFF parse error: inline <x> carries unsupported placeholder semantics"
    );
}

#[test]
fn warns_on_ambiguous_percent_without_rejecting_valid_text() {
    let input = catalog(
        json!({"sourceLanguage":"en","version":"1.0","strings":{"item":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"85% of items"}}}}}}),
    );
    let doc=xliff::parse_document(&document(r#"<trans-unit id="item"><source>85% of items</source><target>85 % des articles</target></trans-unit>"#)).unwrap();
    let plan = xliff::plan_import(&input, &doc, None).unwrap();
    assert_eq!(plan.report.accepted, 1);
    assert!(!plan.report.warnings.is_empty());
    assert_eq!(plan.report.warnings[0].key, "item");
}

rejects_unit!(
    rejects_variation_over_existing_simple_translation,
    json!({"localizations":{"fr":{"stringUnit":{"state":"translated","value":"Hallo"}}}}),
    r#"<trans-unit id="item|==|device.iphone"><source>item</source><target>Bonjour</target></trans-unit>"#,
    "shape_conflict"
);

#[test]
fn rejects_changed_argument_position_inside_substitution_fragment() {
    let input = parser::parse(include_str!(
        "fixtures/apple_xcode27/positive/catalog-matrix/source.xcstrings"
    ))
    .unwrap();
    let xml = document(
        r#"<trans-unit id="multi|==|substitutions.YARDS.plural.one"><source>EN yards %2$lld one</source><target>FR yards %3$lld one</target></trans-unit>"#,
    );
    let plan = xliff::plan_import(&input, &xliff::parse_document(&xml).unwrap(), None).unwrap();
    assert!(plan.candidate.is_none());
    assert_eq!(plan.report.accepted, 0);
    assert_eq!(plan.report.rejected[0].code, "format_mismatch");
}

#[test]
fn initializes_source_substitution_parent_as_draft_for_leaf_only_import() {
    let input = catalog(
        json!({"sourceLanguage":"en","version":"1.0","strings":{"count":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Total %#@COUNT@"},"substitutions":{"COUNT":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"%arg item"}},"other":{"stringUnit":{"state":"translated","value":"%arg items"}}}}}}}}}}}),
    );
    let xml = document(
        r#"<trans-unit id="count|==|substitutions.COUNT.plural.one"><source>%1$lld item</source><target>%1$lld article</target></trans-unit>"#,
    );
    let plan = xliff::plan_import(&input, &xliff::parse_document(&xml).unwrap(), None).unwrap();
    assert_eq!(plan.report.rejected.len(), 0, "{:?}", plan.report.rejected);
    assert_eq!(plan.report.accepted, 1);
    let value = serde_json::to_value(plan.candidate.unwrap()).unwrap();
    assert_eq!(
        value["strings"]["count"]["localizations"]["fr"],
        json!({"stringUnit":{"state":"new","value":"Total %#@COUNT@"},"substitutions":{"COUNT":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"%arg article"}},"many":{"stringUnit":{"state":"new","value":""}},"other":{"stringUnit":{"state":"new","value":""}}}}}}})
    );
}

#[test]
fn initializes_source_substitution_metadata_for_parent_only_import() {
    let input = catalog(
        json!({"sourceLanguage":"en","version":"1.0","strings":{"count":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Total %#@COUNT@"},"substitutions":{"COUNT":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"other":{"stringUnit":{"state":"translated","value":"%arg items"}}}}}}}}}}}),
    );
    let xml = document(
        r#"<trans-unit id="count"><source>Total %1$#@COUNT@</source><target>Total cible %1$#@COUNT@</target></trans-unit>"#,
    );
    let plan = xliff::plan_import(&input, &xliff::parse_document(&xml).unwrap(), None).unwrap();
    assert_eq!(plan.report.rejected.len(), 0, "{:?}", plan.report.rejected);
    assert_eq!(plan.report.accepted, 1);
    let value = serde_json::to_value(plan.candidate.unwrap()).unwrap();
    assert_eq!(
        value["strings"]["count"]["localizations"]["fr"],
        json!({"stringUnit":{"state":"translated","value":"Total cible %#@COUNT@"},"substitutions":{"COUNT":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"one":{"stringUnit":{"state":"new","value":""}},"many":{"stringUnit":{"state":"new","value":""}},"other":{"stringUnit":{"state":"new","value":""}}}}}}})
    );
}

#[test]
fn exports_missing_locale_categories_with_source_other_fallback() {
    let input = catalog(
        json!({"sourceLanguage":"en","version":"1.0","strings":{"count":{"localizations":{"en":{"variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"%lld one"}},"other":{"stringUnit":{"state":"translated","value":"%lld others"}}}}}}}}}),
    );
    let (xml, count) = xliff::export_xliff(&input, "uk", "Localizable.xcstrings", false).unwrap();
    assert_eq!(count, 4);
    let doc = xliff::parse_document(&xml).unwrap();
    assert_eq!(
        doc.files[0]
            .units
            .iter()
            .map(|unit| (
                unit.id.as_str(),
                unit.source.as_deref(),
                unit.target.as_deref()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("count|==|plural.few", Some("%lld others"), None),
            ("count|==|plural.many", Some("%lld others"), None),
            ("count|==|plural.one", Some("%lld one"), None),
            ("count|==|plural.other", Some("%lld others"), None)
        ]
    );
}

#[test]
fn rejects_apple_export_of_literal_variation_suffix_but_keeps_native_import_safe() {
    let input = catalog(
        json!({"sourceLanguage":"en","version":"1.0","strings":{"literal|==|plural.one":{}}}),
    );
    let error = xliff::export_xliff(&input, "fr", "Localizable.xcstrings", false).unwrap_err();
    assert_eq!(
        error.to_string(),
        "XLIFF format error: literal key 'literal|==|plural.one' ends in an Apple variation path; Xcode cannot import its exported updates safely"
    );
    let xml = document(
        r#"<trans-unit id="literal|==|plural.one"><source>literal|==|plural.one</source><target>updated literal</target></trans-unit>"#,
    );
    let plan = xliff::plan_import(&input, &xliff::parse_document(&xml).unwrap(), None).unwrap();
    assert_eq!(plan.report.accepted, 1);
    assert_eq!(
        plan.report.accepted_destinations[0].key,
        "literal|==|plural.one"
    );
    assert_eq!(plan.report.accepted_destinations[0].path, vec![]);
}

#[test]
fn synthesized_target_only_substitution_uses_existing_other_source_identity() {
    let input = parser::parse(include_str!(
        "fixtures/apple_xcode27/negative/new-target-substitution-loss/source.xcstrings"
    ))
    .unwrap();
    let (xml, _) = xliff::export_xliff(&input, "fr", "Localizable.xcstrings", false).unwrap();
    let doc = xliff::parse_document(&xml).unwrap();
    let many = doc.files[0]
        .units
        .iter()
        .find(|u| u.id == "target_only_substitution|==|substitutions.COUNT.plural.many")
        .unwrap();
    assert_eq!(
        many.source.as_deref(),
        Some("target_only_substitution|==|substitutions.COUNT.plural.other")
    );
    assert_eq!(many.target, None);
}

#[test]
fn rejects_nonempty_parent_edit_that_orphans_existing_substitution() {
    let input = parser::parse(include_str!(
        "fixtures/apple_xcode27/negative/new-target-substitution-loss/source.xcstrings"
    ))
    .unwrap();
    let xml = document(
        r#"<trans-unit id="target_only_substitution"><source>%lld total</source><target>%1$lld éléments</target></trans-unit>"#,
    );
    let plan = xliff::plan_import(&input, &xliff::parse_document(&xml).unwrap(), None).unwrap();
    assert!(plan.candidate.is_none());
    assert_eq!(plan.report.accepted, 0);
    assert_eq!(plan.report.rejected[0].code, "shape_conflict");
}

#[test]
fn roundtrips_device_strings_with_root_scoped_substitution_metadata() {
    let input = parser::parse(include_str!(
        "fixtures/apple_xcode27/positive/device-root-substitution/source.xcstrings"
    ))
    .unwrap();
    let (xml, count) = xliff::export_xliff(&input, "de", "Localizable.xcstrings", false).unwrap();
    assert_eq!(count, 4);
    let doc = xliff::parse_document(&xml).unwrap();
    assert_eq!(doc.files[0].units[0].id, "k|==|device.iphone");
    assert_eq!(
        doc.files[0].units[0].source.as_deref(),
        Some("%1$#@COUNT@ phone")
    );
    assert_eq!(
        doc.files[0].units[0].target.as_deref(),
        Some("DE %1$#@COUNT@ phone")
    );
    let edited = xliff::parse_document(include_str!(
        "fixtures/apple_xcode27/positive/device-root-substitution/import-edited.xliff"
    ))
    .unwrap();
    let plan = xliff::plan_import(&input, &edited, None).unwrap();
    assert_eq!(plan.report.rejected.len(), 0, "{:?}", plan.report.rejected);
    assert_eq!(plan.report.accepted, 4);
    assert_eq!(serde_json::to_value(plan.candidate.unwrap()).unwrap(),serde_json::from_str::<serde_json::Value>(include_str!("fixtures/apple_xcode27/positive/device-root-substitution/expected-after-import.xcstrings")).unwrap());
}

#[test]
fn rejects_blank_parent_edit_that_orphans_existing_substitution() {
    let input = parser::parse(include_str!(
        "fixtures/apple_xcode27/negative/new-target-substitution-loss/source.xcstrings"
    ))
    .unwrap();
    let xml = document(
        r#"<trans-unit id="target_only_substitution"><source>%lld total</source><target/></trans-unit>"#,
    );
    let plan = xliff::plan_import(&input, &xliff::parse_document(&xml).unwrap(), None).unwrap();
    assert!(plan.candidate.is_none());
    assert_eq!(plan.report.accepted, 0);
    assert_eq!(plan.report.rejected[0].code, "shape_conflict");
}

#[test]
fn reconstructs_new_root_substitution_from_device_reference_in_any_unit_order() {
    let input = catalog(
        json!({"sourceLanguage":"en","version":"1.0","strings":{"count":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"%lld items"}}}}}}),
    );
    let xml = document(
        r#"<trans-unit id="count|==|substitutions.COUNT.plural.one"><source>%lld items</source><target>%1$lld article</target></trans-unit><trans-unit id="count|==|substitutions.COUNT.plural.other"><source>%lld items</source><target>%1$lld articles</target></trans-unit><trans-unit id="count|==|device.iphone"><source>%lld items</source><target>%1$#@COUNT@ téléphone</target></trans-unit><trans-unit id="count|==|device.other"><source>%lld items</source><target>%lld appareils</target></trans-unit>"#,
    );
    let plan = xliff::plan_import(&input, &xliff::parse_document(&xml).unwrap(), None).unwrap();
    assert_eq!(plan.report.rejected.len(), 0, "{:?}", plan.report.rejected);
    assert_eq!(plan.report.accepted, 4);
    let value = serde_json::to_value(plan.candidate.unwrap()).unwrap();
    assert_eq!(
        value["strings"]["count"]["localizations"]["fr"],
        json!({"variations":{"device":{"iphone":{"stringUnit":{"state":"translated","value":"%#@COUNT@ téléphone"}},"other":{"stringUnit":{"state":"translated","value":"%lld appareils"}}}},"substitutions":{"COUNT":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"%arg article"}},"other":{"stringUnit":{"state":"translated","value":"%arg articles"}}}}}}})
    );
}
