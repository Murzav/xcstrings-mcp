mod workflow_support;
use serde_json::json;
use workflow_support::*;
use xcstrings_mcp::{
    model::{translation::TranslationDestination, workflow::*, xcstrings::paths::LeafStep},
    service::{parser, workflow},
};

#[test]
fn selected_target_branch_metadata_changes_token_but_sibling_text_does_not() {
    let mut raw = json!({"sourceLanguage":"en","version":"1.0","strings":{"k":{"localizations":{"de":{"variations":{"plural":{
        "one":{"futureBranch":"first","stringUnit":{"state":"needs_review","value":"Ein Ding"}},
        "other":{"stringUnit":{"state":"needs_review","value":"Viele Dinge"}}
    }}}}}}});
    let destination = TranslationDestination {
        key: "k".into(),
        locale: "de".into(),
        path: vec![LeafStep::Plural("one".into())],
    };
    let file = parser::parse(&raw.to_string()).unwrap();
    let token = workflow::target_version(ID, &file, &destination).unwrap();
    raw["strings"]["k"]["localizations"]["de"]["variations"]["plural"]["other"]["stringUnit"] =
        json!({"state":"translated","value":"Andere Dinge"});
    let sibling_changed = parser::parse(&raw.to_string()).unwrap();
    assert_eq!(
        workflow::target_version(ID, &sibling_changed, &destination).unwrap(),
        token
    );
    raw["strings"]["k"]["localizations"]["de"]["variations"]["plural"]["one"]["futureBranch"] =
        json!("different");
    let selected_changed = parser::parse(&raw.to_string()).unwrap();
    assert_ne!(
        workflow::target_version(ID, &selected_changed, &destination).unwrap(),
        token
    );
}

#[test]
fn checkpoint_rechecks_actual_target_shape_after_planning() {
    let file = catalog();
    let document = WorkflowDocument::default();
    let plan = workflow::plan_source_sync(ID, &file, &document, SyncMode::AdoptExisting).unwrap();
    let mut changed = json_file(&file);
    changed["strings"]["delete"]["localizations"]["de"]["variations"] =
        json!({"future_axis":{"first":{"stringUnit":{"state":"new","value":""}}}});
    let changed = parser::parse(&changed.to_string()).unwrap();
    assert!(
        workflow::plan_checkpoint(&changed, &document, &plan)
            .unwrap_err()
            .to_string()
            .contains("unsupported_shape")
    );
}

#[test]
fn malformed_snapshot_known_source_fields_are_not_accepted_as_a_baseline() {
    let file = catalog();
    let document = tracked(&file);
    let mut raw = serde_json::to_value(document).unwrap();
    raw["source_baseline"]["sources"]["delete"]["source"]["stringUnit"]["value"] = json!(73);
    assert!(
        workflow::parse_document(&raw.to_string())
            .unwrap_err()
            .to_string()
            .contains("invalid_source_snapshot")
    );
}

#[test]
fn truncated_pending_content_fingerprint_is_not_accepted() {
    let file = catalog();
    let document = WorkflowDocument::default();
    let plan = workflow::plan_source_sync(ID, &file, &document, SyncMode::Review).unwrap();
    let checkpoint =
        workflow::plan_checkpoint(plan.invalidation.as_ref().unwrap(), &document, &plan).unwrap();
    let mut raw = serde_json::to_value(checkpoint).unwrap();
    raw["pending_review"][0]["target_content_version"] = json!("target-content-v1:sha256:abc");
    assert!(
        workflow::parse_document(&raw.to_string())
            .unwrap_err()
            .to_string()
            .contains("invalid_pending_review")
    );
}

#[test]
fn adding_unrelated_plural_sibling_retains_origin_of_unchanged_draft_text() {
    let raw = json!({"sourceLanguage":"en","version":"1.0","strings":{"k":{"localizations":{
        "en":{"variations":{"plural":{"other":{"stringUnit":{"state":"translated","value":"%lld items"}}}}},
        "de":{"variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"%lld Ding"}},"other":{"stringUnit":{"state":"translated","value":"%lld Dinge"}}}}}
    }}}});
    let file = parser::parse(&raw.to_string()).unwrap();
    let document = tracked(&file);
    let old = workflow::source_snapshot(&file, &document, "k").unwrap();
    let mut changed = raw;
    changed["strings"]["k"]["localizations"]["en"]["variations"]["plural"]["other"]["stringUnit"]
        ["value"] = json!("%lld objects");
    let changed = parser::parse(&changed.to_string()).unwrap();
    let plan = workflow::plan_source_sync(ID, &changed, &document, SyncMode::Review).unwrap();
    let invalidated = plan.invalidation.as_ref().unwrap();
    let checkpoint = workflow::plan_checkpoint(invalidated, &document, &plan).unwrap();
    let mut new_sibling = json_file(invalidated);
    new_sibling["strings"]["k"]["localizations"]["de"]["variations"]["plural"]["zero"] =
        json!({"stringUnit":{"state":"new","value":"Keine Dinge"}});
    let new_sibling = parser::parse(&new_sibling.to_string()).unwrap();
    let queue = workflow::review_queue(ID, &new_sibling, &checkpoint, &query()).unwrap();
    let other = queue
        .items
        .iter()
        .find(|item| item.destination.path == vec![LeafStep::Plural("other".into())])
        .unwrap();
    assert_eq!(other.old_source.as_ref(), Some(&old));
    assert_eq!(
        other.reasons,
        vec![ReviewReason::Draft, ReviewReason::SourceChanged]
    );
}
