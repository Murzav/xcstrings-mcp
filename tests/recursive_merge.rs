use serde_json::{Value, json};
use xcstrings_mcp::service::semantic_merge::{MergeOptions, prepare_merge};

const UNIT: &str =
    "/strings/count/localizations/fr/variations/device/iphone/variations/plural/one/stringUnit";

fn catalog() -> Value {
    json!({"sourceLanguage":"en","version":"1.0","strings":{"count":{"localizations":{"fr":{"variations":{"device":{"iphone":{"variations":{"plural":{"one":{"stringUnit":{"state":"new","value":"Un objet","futureUnit":17}},"other":{"stringUnit":{"state":"translated","value":"Des objets"}}}}},"other":{"stringUnit":{"state":"translated","value":"Objet"}}}}}}}}})
}

#[test]
fn nested_string_unit_state_and_value_remain_one_atomic_conflict() {
    let base = catalog();
    let mut current = base.clone();
    let mut incoming = base.clone();
    *current.pointer_mut(&format!("{UNIT}/state")).unwrap() = json!("translated");
    *incoming.pointer_mut(&format!("{UNIT}/value")).unwrap() = json!("Un article");

    let merged = prepare_merge(
        base.to_string().as_bytes(),
        current.to_string().as_bytes(),
        incoming.to_string().as_bytes(),
        &MergeOptions::default(),
    )
    .unwrap();

    assert_eq!(merged.report.conflict_total, 1);
    assert_eq!(merged.report.unresolved_conflict_total, 1);
    let conflict = &merged.report.conflicts[0];
    assert_eq!(conflict.pointer, UNIT);
    assert_eq!(conflict.kind, "atomic_divergence");
    assert_eq!(conflict.key.as_deref(), Some("count"));
    assert_eq!(conflict.locale.as_deref(), Some("fr"));
    assert_eq!(
        serde_json::from_str::<Value>(&merged.content).unwrap(),
        base
    );
}

#[test]
fn unknown_nested_metadata_stays_atomic_while_known_siblings_merge() {
    let mut base = catalog();
    let node = "/strings/count/localizations/fr/variations/device/iphone";
    base.pointer_mut(node).unwrap()["futureObject"] = json!({"a":1,"b":2});
    let mut current = base.clone();
    let mut incoming = base.clone();
    current.pointer_mut(node).unwrap()["futureObject"]["a"] = json!(3);
    incoming.pointer_mut(node).unwrap()["futureObject"]["b"] = json!(4);
    *current.pointer_mut(&format!("{UNIT}/value")).unwrap() = json!("Article");

    let merged = prepare_merge(
        base.to_string().as_bytes(),
        current.to_string().as_bytes(),
        incoming.to_string().as_bytes(),
        &MergeOptions::default(),
    )
    .unwrap();

    assert_eq!(merged.report.conflict_total, 1);
    assert_eq!(
        merged.report.conflicts[0].pointer,
        format!("{node}/futureObject")
    );
    assert_eq!(merged.report.conflicts[0].kind, "atomic_divergence");
    let mut expected = base;
    *expected.pointer_mut(&format!("{UNIT}/value")).unwrap() = json!("Article");
    assert_eq!(
        serde_json::from_str::<Value>(&merged.content).unwrap(),
        expected
    );
}
