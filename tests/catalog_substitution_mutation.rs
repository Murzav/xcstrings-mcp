use serde_json::json;
use xcstrings_mcp::service::{merger::merge_translations, parser::parse};

const SOURCE: &str = r#"{"sourceLanguage":"en","strings":{"birds":{"localizations":{"en":{"substitutions":{"BIRDS":{"argNum":1,"futureSub":9,"formatSpecifier":"lld","variations":{"futureAxis":null,"plural":{"one":{"stringUnit":{"state":"translated","value":"%arg bird"}},"other":{"stringUnit":{"state":"translated","futureUnit":true,"value":"%arg birds"}}}}}}}}}},"version":"1.0"}"#;

#[test]
fn new_target_substitution_never_inherits_unsubmitted_source_translations() {
    let mut file = parse(SOURCE).unwrap();
    let translation = serde_json::from_value(json!({"key":"birds","locale":"ja","value":"","substitution_name":"BIRDS","plural_forms":{"other":"%arg 鳥"}})).unwrap();

    let result = merge_translations(&mut file, &[translation]);

    assert_eq!(result.accepted, 1);
    assert_eq!(result.rejected.len(), 0);
    assert_eq!(
        serde_json::to_value(&file.strings["birds"].localizations.as_ref().unwrap()["ja"]).unwrap(),
        json!({
            "substitutions":{"BIRDS":{"argNum":1,"futureSub":9,"formatSpecifier":"lld","variations":{"futureAxis":null,"plural":{"other":{"stringUnit":{"state":"translated","futureUnit":true,"value":"%arg 鳥"}}}}}}
        })
    );
    assert_eq!(
        file.strings["birds"].localizations.as_ref().unwrap()["en"]
            .substitutions
            .as_ref()
            .unwrap()["BIRDS"]
            .variations
            .as_ref()
            .unwrap()
            .plural
            .as_ref()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn existing_target_substitution_keeps_unsubmitted_siblings_and_metadata() {
    let mut file = parse(SOURCE).unwrap();
    file.strings["birds"].localizations.as_mut().unwrap().insert("de".into(), serde_json::from_value(json!({"substitutions":{"BIRDS":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"one":{"stringUnit":{"state":"needs_review","value":"Ein Vogel"}},"other":{"stringUnit":{"state":"translated","futureUnit":7,"value":"Alt"}}}}}}})).unwrap());
    let translation = serde_json::from_value(json!({"key":"birds","locale":"de","value":"","substitution_name":"BIRDS","plural_forms":{"other":"%arg Vögel"}})).unwrap();

    let result = merge_translations(&mut file, &[translation]);

    assert_eq!(result.accepted, 1);
    assert_eq!(
        serde_json::to_value(&file.strings["birds"].localizations.as_ref().unwrap()["de"]).unwrap(),
        json!({"substitutions":{"BIRDS":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"one":{"stringUnit":{"state":"needs_review","value":"Ein Vogel"}},"other":{"stringUnit":{"state":"translated","futureUnit":7,"value":"%arg Vögel"}}}}}}})
    );
}
