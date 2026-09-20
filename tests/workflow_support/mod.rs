#![allow(dead_code)]
use serde_json::{Value, json};
use xcstrings_mcp::model::{workflow::*, xcstrings::XcStringsFile};
use xcstrings_mcp::service::{parser, workflow};

pub const ID: &str = "/project/Localizable.xcstrings";
pub fn catalog() -> XcStringsFile {
    parser::parse(&json!({"sourceLanguage":"en","version":"1.0","futureRoot":{"state":"keep"},"strings":{
        "delete":{"comment":"Destructive button","futureEntry":{"state":"keep"},"localizations":{
            "en":{"stringUnit":{"state":"translated","value":"Delete","futureUnit":{"state":"keep"}}},
            "de":{"futureLocale":true,"stringUnit":{"state":"translated","value":"Löschen","futureUnit":7}},
            "fr":{"stringUnit":{"state":"needs_review","value":"Supprimer"}}
        }},
        "save":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Save"}},"de":{"stringUnit":{"state":"translated","value":"Speichern"}}}}
    }}).to_string()).unwrap()
}
pub fn set_source(file: &mut XcStringsFile, value: &str) {
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
        .value = value.into();
}
pub fn set_target(file: &mut XcStringsFile, locale: &str, value: &str) {
    file.strings
        .get_mut("delete")
        .unwrap()
        .localizations
        .as_mut()
        .unwrap()
        .get_mut(locale)
        .unwrap()
        .string_unit
        .as_mut()
        .unwrap()
        .value = value.into();
}
pub fn tracked(file: &XcStringsFile) -> WorkflowDocument {
    let document = WorkflowDocument::default();
    let plan = workflow::plan_source_sync(ID, file, &document, SyncMode::AdoptExisting).unwrap();
    workflow::plan_checkpoint(file, &document, &plan).unwrap()
}
pub fn query() -> ReviewQuery {
    ReviewQuery {
        locale: Some("de".into()),
        batch_size: 100,
        ..Default::default()
    }
}
pub fn json_file(file: &XcStringsFile) -> Value {
    serde_json::to_value(file).unwrap()
}
pub fn approve_request(
    file: &XcStringsFile,
    document: &WorkflowDocument,
    key: &str,
    locale: &str,
) -> ApprovalRequest {
    let destination = xcstrings_mcp::model::translation::TranslationDestination {
        key: key.into(),
        locale: locale.into(),
        path: vec![],
    };
    let view = workflow::inspect(ID, file, document).unwrap();
    ApprovalRequest {
        key: key.into(),
        locale: locale.into(),
        path: vec![],
        expected_source_version: view.keys[key].source_version.clone(),
        expected_target_version: workflow::target_version(ID, file, &destination)
            .unwrap()
            .unwrap(),
    }
}
