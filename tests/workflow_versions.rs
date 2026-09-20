mod workflow_support;
use serde_json::json;
use workflow_support::*;
use xcstrings_mcp::model::workflow::WorkflowDocument;
use xcstrings_mcp::service::{parser, workflow};

#[test]
fn source_tokens_bind_catalog_but_portable_snapshots_do_not() {
    let file = catalog();
    let document = tracked(&file);
    let snapshot = workflow::source_snapshot(&file, &document, "delete").unwrap();
    assert_ne!(
        workflow::source_version(ID, "delete", &snapshot),
        workflow::source_version("/other/Localizable.xcstrings", "delete", &snapshot)
    );
    let moved = workflow::inspect("/other/Localizable.xcstrings", &file, &document).unwrap();
    assert_eq!(
        moved.keys["delete"].freshness,
        xcstrings_mcp::model::workflow::SourceFreshness::Current
    );
}

#[test]
fn source_versions_ignore_target_edits_and_editorial_state_but_keep_unknown_state_fields() {
    let mut file = catalog();
    let document = WorkflowDocument::default();
    let original = workflow::source_snapshot(&file, &document, "delete").unwrap();
    set_target(&mut file, "de", "Entfernen");
    file.strings
        .get_mut("delete")
        .unwrap()
        .localizations
        .as_mut()
        .unwrap()
        .get_mut("en")
        .unwrap()
        .string_unit
        .as_mut()
        .unwrap()
        .state = xcstrings_mcp::model::xcstrings::TranslationState::NeedsReview;
    assert_eq!(
        workflow::source_snapshot(&file, &document, "delete").unwrap(),
        original
    );
    file.strings
        .get_mut("delete")
        .unwrap()
        .localizations
        .as_mut()
        .unwrap()
        .get_mut("en")
        .unwrap()
        .string_unit
        .as_mut()
        .unwrap()
        .extra
        .insert("futureUnit".into(), json!({"state":"changed"}));
    assert_ne!(
        workflow::source_snapshot(&file, &document, "delete").unwrap(),
        original
    );
}

#[test]
fn source_versions_ignore_json_order_and_catalog_schema_normalization() {
    let a=parser::parse(r#"{"sourceLanguage":"en","version":"1.0","strings":{"k":{"localizations":{"en":{"stringUnit":{"value":"Open","state":"translated"}}}}}}"#).unwrap();
    let b=parser::parse(r#"{"strings":{"k":{"localizations":{"en":{"stringUnit":{"state":"new","value":"Open"}}}}},"version":"1.1","sourceLanguage":"en"}"#).unwrap();
    assert_eq!(
        workflow::source_snapshot(&a, &WorkflowDocument::default(), "k").unwrap(),
        workflow::source_snapshot(&b, &WorkflowDocument::default(), "k").unwrap()
    );
}

#[test]
fn authored_context_changes_revision_without_neighbor_target_dependencies() {
    let file = catalog();
    let mut document = WorkflowDocument::default();
    let original = workflow::source_snapshot(&file, &document, "delete").unwrap();
    document.contexts =
        serde_json::from_value(json!({"delete":{"context":{"purpose":"Remove account"}}})).unwrap();
    let contextual = workflow::source_snapshot(&file, &document, "delete").unwrap();
    assert_ne!(contextual, original);
    document.contexts.insert(
        "save".into(),
        serde_json::from_value(json!({"context":{"purpose":"Store settings"}})).unwrap(),
    );
    assert_eq!(
        workflow::source_snapshot(&file, &document, "delete").unwrap(),
        contextual
    );
}

#[test]
fn recursive_source_and_global_argument_metadata_are_revision_dependencies() {
    let mut file=parser::parse(r#"{"sourceLanguage":"en","version":"1.0","strings":{"k":{"localizations":{"en":{"variations":{"device":{"iphone":{"stringUnit":{"state":"translated","value":"%#@N@ phone"}},"other":{"stringUnit":{"state":"translated","value":"%lld devices"}}}},"substitutions":{"N":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"other":{"stringUnit":{"state":"translated","value":"%arg items"}}}}}}}}}}}"#).unwrap();
    let document = WorkflowDocument::default();
    let before = workflow::source_snapshot(&file, &document, "k").unwrap();
    file.strings
        .get_mut("k")
        .unwrap()
        .localizations
        .as_mut()
        .unwrap()
        .get_mut("en")
        .unwrap()
        .substitutions
        .as_mut()
        .unwrap()
        .get_mut("N")
        .unwrap()
        .arg_num = Some(2);
    assert_ne!(
        workflow::source_snapshot(&file, &document, "k").unwrap(),
        before
    );
}
