use serde_json::json;
use xcstrings_mcp::model::xcstrings::XcStringsFile;
use xcstrings_mcp::service::{coverage, extractor, parser, plural_extractor};

fn catalog(localizations: serde_json::Value) -> XcStringsFile {
    parser::parse(&json!({"sourceLanguage":"en","strings":{"birds":{"localizations":localizations}},"version":"1.0"}).to_string()).unwrap()
}

#[test]
fn coverage_requires_every_nested_leaf_and_cldr_destination() {
    let file = catalog(
        json!({"en":{"variations":{"device":{"iphone":{"variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"One bird"}},"other":{"stringUnit":{"state":"translated","value":"%lld birds"}}}}}}}},"fr":{"variations":{"device":{"iphone":{"variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"Un oiseau"}},"other":{"stringUnit":{"state":"translated","value":"%lld oiseaux"}}}}}}}}}),
    );

    let report = coverage::get_coverage(&file);
    let (units, total) = extractor::get_untranslated(&file, "fr", 10, 0).unwrap();

    assert_eq!(
        report
            .locales
            .iter()
            .find(|l| l.locale == "fr")
            .unwrap()
            .translated,
        0
    );
    assert_eq!(total, 1);
    assert_eq!(units[0].key, "birds");
    let value = serde_json::to_value(&units[0]).unwrap();
    assert_eq!(
        value["leaves"][2]["path"],
        json!([{"device":"iphone"},{"plural":"many"}])
    );
    assert_eq!(value["leaves"][2]["source_text"], "%lld birds");
    assert_eq!(value["leaves"][2]["complete"], false);
}

#[test]
fn explicit_simple_target_can_complete_a_varied_source_and_blank_is_ready() {
    let file = catalog(
        json!({"en":{"variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"One bird"}},"other":{"stringUnit":{"state":"translated","value":"%lld birds"}}}}},"ja":{"stringUnit":{"state":"machine_translated","value":""}}}),
    );

    let report = coverage::get_coverage(&file);
    let (_, total) = extractor::get_untranslated(&file, "ja", 10, 0).unwrap();

    assert_eq!(
        report
            .locales
            .iter()
            .find(|l| l.locale == "ja")
            .unwrap()
            .translated,
        1
    );
    assert_eq!(total, 0);
}

#[test]
fn empty_variations_and_unknown_locale_never_count_complete() {
    let file = catalog(
        json!({"en":{"stringUnit":{"state":"translated","value":"Birds"}},"de":{"variations":{"plural":{}}},"xx":{"stringUnit":{"state":"translated","value":"Future"}}}),
    );

    let report = coverage::get_coverage(&file);

    assert_eq!(
        report
            .locales
            .iter()
            .find(|l| l.locale == "de")
            .unwrap()
            .translated,
        0
    );
    assert_eq!(
        report
            .locales
            .iter()
            .find(|l| l.locale == "xx")
            .unwrap()
            .translated,
        0
    );
    assert_eq!(
        parser::summarize(&file).keys_by_state.get("translated"),
        Some(&1)
    );
}

#[test]
fn plural_extraction_includes_every_substitution_and_draft_leaf() {
    let file = catalog(
        json!({"en":{"stringUnit":{"state":"translated","value":"%1$#@BIRDS@ %2$#@NESTS@"},"substitutions":{"BIRDS":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"%arg bird"}},"other":{"stringUnit":{"state":"translated","value":"%arg birds"}}}}},"NESTS":{"argNum":2,"formatSpecifier":"lld","variations":{"plural":{"other":{"stringUnit":{"state":"translated","value":"%arg nests"}}}}}}},"ja":{"stringUnit":{"state":"translated","value":"%1$#@BIRDS@ %2$#@NESTS@"},"substitutions":{"BIRDS":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"other":{"stringUnit":{"state":"translated","value":"%arg 鳥"}}}}},"NESTS":{"argNum":2,"formatSpecifier":"lld","variations":{"plural":{"other":{"stringUnit":{"state":"needs_review","value":"%arg 巣"}}}}}}}}),
    );

    let (units, total) = plural_extractor::get_untranslated_plurals(&file, "ja", 10, 0).unwrap();

    assert_eq!(total, 1);
    let value = serde_json::to_value(&units[0]).unwrap();
    assert_eq!(
        value["leaves"][2]["path"],
        json!([{"substitution":"NESTS"},{"plural":"other"}])
    );
    assert_eq!(value["leaves"][2]["state"], "needs_review");
    assert_eq!(value["leaves"][2]["complete"], false);
    assert_eq!(
        value["leaves"][2]["substitutions"],
        json!([{"name":"NESTS","arg_num":2,"format_specifier":"lld"}])
    );
}

#[test]
fn source_diff_and_search_observe_nested_leaf_text() {
    let before = catalog(
        json!({"en":{"variations":{"device":{"mac":{"stringUnit":{"state":"translated","value":"Desktop birds"}}}}}}),
    );
    let after = catalog(
        json!({"en":{"variations":{"device":{"mac":{"stringUnit":{"state":"translated","value":"Desktop flocks"}}}}}}),
    );

    let report = xcstrings_mcp::service::diff::compute_diff(&before, &after);
    let (units, total) = extractor::search_keys(&after, "flocks", "de", 10, 0).unwrap();

    assert_eq!(report.modified.len(), 1);
    assert_eq!(report.modified[0].key, "birds");
    assert_eq!(total, 1);
    assert_eq!(units[0].key, "birds");
}

#[test]
fn add_locale_initializes_all_nested_destinations_as_new_without_source_translations() {
    let mut file = catalog(
        json!({"en":{"variations":{"device":{"iphone":{"variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"One bird","future":4}},"other":{"stringUnit":{"state":"translated","value":"%lld birds"}}}}},"other":{"stringUnit":{"state":"translated","value":"Birds"}}}}}}),
    );

    let count = xcstrings_mcp::service::locale::add_locale(&mut file, "ja").unwrap();

    assert_eq!(count, 1);
    let target =
        serde_json::to_value(&file.strings["birds"].localizations.as_ref().unwrap()["ja"]).unwrap();
    assert_eq!(
        target,
        json!({"variations":{"device":{"iphone":{"variations":{"plural":{"other":{"stringUnit":{"state":"new","value":""}}}}},"other":{"stringUnit":{"state":"new","value":""}}}}})
    );
}

#[test]
fn semantic_merge_combines_disjoint_nested_branches_and_keeps_units_atomic() {
    use xcstrings_mcp::service::semantic_merge::{MergeOptions, prepare_merge};
    let base = json!({"sourceLanguage":"en","strings":{"birds":{"localizations":{"en":{"variations":{"device":{"iphone":{"variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"One bird"}},"other":{"stringUnit":{"state":"translated","value":"Birds"}}}}}}}}}}},"version":"1.0"});
    let mut current = base.clone();
    let mut incoming = base.clone();
    current["strings"]["birds"]["localizations"]["en"]["variations"]["device"]["iphone"]["variations"]
        ["plural"]["one"]["stringUnit"]["value"] = json!("Single bird");
    incoming["strings"]["birds"]["localizations"]["en"]["variations"]["device"]["iphone"]["variations"]
        ["plural"]["other"]["stringUnit"]["value"] = json!("Flock");

    let merged = prepare_merge(
        base.to_string().as_bytes(),
        current.to_string().as_bytes(),
        incoming.to_string().as_bytes(),
        &MergeOptions::default(),
    )
    .unwrap();

    assert_eq!(merged.report.conflict_total, 0);
    let result: serde_json::Value = serde_json::from_str(&merged.content).unwrap();
    assert_eq!(
        result["strings"]["birds"]["localizations"]["en"]["variations"]["device"]["iphone"]["variations"]
            ["plural"]["one"]["stringUnit"]["value"],
        "Single bird"
    );
    assert_eq!(
        result["strings"]["birds"]["localizations"]["en"]["variations"]["device"]["iphone"]["variations"]
            ["plural"]["other"]["stringUnit"]["value"],
        "Flock"
    );
}

#[test]
fn unsupported_semantic_branch_is_reported_without_losing_known_leaf() {
    let file = catalog(
        json!({"en":{"stringUnit":{"state":"translated","value":"Birds"}},"de":{"stringUnit":{"state":"translated","value":"Vögel"},"variations":{"futureAxis":{"new":{"stringUnit":{"state":"translated","value":"Future"}}}}}}),
    );

    let (units, total) = extractor::get_untranslated(&file, "de", 10, 0).unwrap();

    assert_eq!(total, 1);
    let value = serde_json::to_value(&units[0]).unwrap();
    assert_eq!(value["leaves"][0]["value"], "Vögel");
    assert_eq!(
        value["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|issue| issue["code"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["invalid_shape", "unknown_axis"]
    );
    assert_eq!(value["diagnostics"][0]["path"], json!([]));
}
