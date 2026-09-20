mod workflow_support;
use serde_json::{Value, json};
use workflow_support::*;
use xcstrings_mcp::{
    XcStringsError,
    model::{translation::TranslationDestination, workflow::*, xcstrings::paths::LeafStep},
    service::{parser, workflow},
};

fn substitution_catalog() -> xcstrings_mcp::XcStringsFile {
    parser::parse(&json!({"sourceLanguage":"en","version":"1.0","strings":{"count":{"localizations":{
        "en":{"stringUnit":{"state":"translated","value":"%#@N@ items"},"substitutions":{"N":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"other":{"stringUnit":{"state":"translated","value":"%arg items"}}}}}}},
        "de":{"stringUnit":{"state":"needs_review","value":"%#@N@ Dinge"},"substitutions":{"N":{"argNum":1,"formatSpecifier":"lld","future":"preserve","variations":{"plural":{"one":{"stringUnit":{"state":"needs_review","value":"%arg Ding"}},"other":{"stringUnit":{"state":"needs_review","value":"%arg Dinge"}}}}}}}
    }}}}).to_string()).unwrap()
}

#[test]
fn root_and_substitution_leaf_approval_preserves_independent_sibling_draft() {
    let file = substitution_catalog();
    let document = tracked(&file);
    let persisted = workflow::format_document(&document).unwrap();
    let restored = workflow::parse_document(&persisted).unwrap();
    assert_eq!(restored, document);
    let parent = approve_request(&file, &restored, "count", "de");
    let destination = TranslationDestination {
        key: "count".into(),
        locale: "de".into(),
        path: vec![
            LeafStep::Substitution("N".into()),
            LeafStep::Plural("other".into()),
        ],
    };
    let mut leaf = parent.clone();
    leaf.path = destination.path.clone();
    leaf.expected_target_version = workflow::target_version(ID, &file, &destination)
        .unwrap()
        .unwrap();

    let result = workflow::plan_approval(ID, &file, &restored, &[parent, leaf]).unwrap();

    assert_eq!(result.report.rejected, vec![]);
    assert_eq!(result.report.accepted, 2);
    let mut expected = json_file(&file);
    expected["strings"]["count"]["localizations"]["de"]["stringUnit"]["state"] =
        json!("translated");
    expected["strings"]["count"]["localizations"]["de"]["substitutions"]["N"]["variations"]["plural"]
        ["other"]["stringUnit"]["state"] = json!("translated");
    assert_eq!(json_file(&result.candidate.unwrap()), expected);
}

#[test]
fn overlapping_root_and_plural_approvals_reject_entire_invalid_tree() {
    let mut raw = json_file(&catalog());
    raw["strings"]["delete"]["localizations"]["fr"]["variations"] =
        json!({"plural":{"other":{"stringUnit":{"state":"needs_review","value":"Supprimer"}}}});
    let file = parser::parse(&raw.to_string()).unwrap();
    // Establish a source checkpoint from the valid catalog; the invalid target never affects source inputs.
    let document = tracked(&catalog());
    let parent = approve_request(&file, &document, "delete", "fr");
    let mut child = parent.clone();
    child.path = vec![LeafStep::Plural("other".into())];
    child.expected_target_version = workflow::target_version(ID, &file, &child.destination())
        .unwrap()
        .unwrap();

    let result = workflow::plan_approval(ID, &file, &document, &[parent, child]).unwrap();

    assert!(result.candidate.is_none());
    assert_eq!(result.report.accepted, 0);
    assert_eq!(result.report.accepted_destinations, vec![]);
    assert_eq!(
        result
            .report
            .rejected
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec![
            WorkflowCode::OverlappingDestination,
            WorkflowCode::UnsupportedShape,
            WorkflowCode::UnsupportedShape
        ]
    );
    assert_eq!(json_file(&file), raw);
}

#[test]
fn substitution_metadata_change_invalidates_leaf_approval_without_sibling_text_dependency() {
    let file = substitution_catalog();
    let destination = TranslationDestination {
        key: "count".into(),
        locale: "de".into(),
        path: vec![
            LeafStep::Substitution("N".into()),
            LeafStep::Plural("other".into()),
        ],
    };
    let before = workflow::target_version(ID, &file, &destination)
        .unwrap()
        .unwrap();
    let mut raw = json_file(&file);
    raw["strings"]["count"]["localizations"]["de"]["substitutions"]["N"]["variations"]["plural"]
        ["one"]["stringUnit"]["value"] = json!("Nur ein Ding");
    let sibling = parser::parse(&raw.to_string()).unwrap();
    assert_eq!(
        workflow::target_version(ID, &sibling, &destination).unwrap(),
        Some(before.clone())
    );
    raw["strings"]["count"]["localizations"]["de"]["substitutions"]["N"]["argNum"] = json!(2);
    let changed = parser::parse(&raw.to_string()).unwrap();
    assert_ne!(
        workflow::target_version(ID, &changed, &destination).unwrap(),
        Some(before)
    );
}

#[test]
fn device_leaf_token_binds_selected_metadata_without_sibling_text_dependency() {
    let mut raw = json!({"sourceLanguage":"en","version":"1.0","strings":{"screen":{"localizations":{"de":{"variations":{"device":{"iphone":{"futureDevice":1,"stringUnit":{"state":"needs_review","value":"Telefon"}},"other":{"stringUnit":{"state":"needs_review","value":"Gerät"}}}}}}}}});
    let file = parser::parse(&raw.to_string()).unwrap();
    let destination = TranslationDestination {
        key: "screen".into(),
        locale: "de".into(),
        path: vec![LeafStep::Device(
            xcstrings_mcp::model::xcstrings::DeviceCategory::IPhone,
        )],
    };
    let token = workflow::target_version(ID, &file, &destination)
        .unwrap()
        .unwrap();
    raw["strings"]["screen"]["localizations"]["de"]["variations"]["device"]["other"]["stringUnit"]
        ["value"] = json!("Computer");
    assert_eq!(
        workflow::target_version(ID, &parser::parse(&raw.to_string()).unwrap(), &destination)
            .unwrap(),
        Some(token.clone())
    );
    raw["strings"]["screen"]["localizations"]["de"]["variations"]["device"]["iphone"]["futureDevice"] =
        json!(2);
    assert_ne!(
        workflow::target_version(ID, &parser::parse(&raw.to_string()).unwrap(), &destination)
            .unwrap(),
        Some(token)
    );
}

#[test]
fn duplicate_pending_destination_is_rejected_before_provenance_can_be_ambiguous() {
    let file = catalog();
    let document = WorkflowDocument::default();
    let plan = workflow::plan_source_sync(ID, &file, &document, SyncMode::Review).unwrap();
    let checkpoint =
        workflow::plan_checkpoint(plan.invalidation.as_ref().unwrap(), &document, &plan).unwrap();
    let mut raw = serde_json::to_value(checkpoint).unwrap();
    let duplicate = raw["pending_review"][0].clone();
    raw["pending_review"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);

    let error = workflow::parse_document(&raw.to_string()).unwrap_err();

    assert!(
        matches!(error,XcStringsError::InvalidFormat(message) if message == "duplicate_pending_review: duplicate destination")
    );
}

#[test]
fn malformed_baseline_field_types_fail_closed_instead_of_becoming_current() {
    let document = tracked(&catalog());
    let valid = serde_json::to_value(document).unwrap();
    let cases: &[(&str, Value)] = &[
        ("key", json!("different key")),
        ("source_language", json!("")),
        ("should_translate", json!("true")),
        ("comment", json!(false)),
        ("source", json!([])),
        ("context", json!(7)),
        ("root_extra", json!([])),
        ("entry_extra", Value::Null),
    ];
    for (field, invalid) in cases {
        let mut raw = valid.clone();
        raw["source_baseline"]["sources"]["delete"][field] = invalid.clone();
        let error = workflow::parse_document(&raw.to_string()).unwrap_err();
        assert!(
            matches!(error,XcStringsError::InvalidFormat(message) if message == "invalid_source_snapshot: malformed snapshot for key \"delete\""),
            "{field}"
        );
    }
    let mut scalar = valid;
    scalar["source_baseline"]["sources"]["delete"] = json!(23);
    assert!(
        matches!(workflow::parse_document(&scalar.to_string()).unwrap_err(),XcStringsError::InvalidFormat(message) if message == "invalid_source_snapshot: malformed snapshot for key \"delete\"")
    );
}

#[test]
fn uninitialized_queue_filters_untracked_ready_items_and_paginates_same_snapshot() {
    let file = catalog();
    let document = WorkflowDocument::default();
    let mut query = query();
    query.batch_size = 1;
    query.reasons = vec![ReviewReason::Untracked];
    let first = workflow::review_queue(ID, &file, &document, &query).unwrap();
    assert_eq!(first.total, 2);
    assert_eq!(first.next_offset, Some(1));
    assert_eq!(first.items[0].destination.key, "delete");
    assert_eq!(first.items[0].reasons, vec![ReviewReason::Untracked]);
    assert_eq!(
        first.items[0].native_state,
        xcstrings_mcp::model::xcstrings::TranslationState::Translated
    );
    query.offset = 1;
    query.expected_queue_version = Some(first.queue_version);
    let second = workflow::review_queue(ID, &file, &document, &query).unwrap();
    assert_eq!(second.total, 2);
    assert_eq!(second.next_offset, None);
    assert_eq!(second.items[0].destination.key, "save");
    query.offset = 0;
    query.expected_queue_version = None;
    query.reasons = vec![ReviewReason::Draft];
    let excluded = workflow::review_queue(ID, &file, &document, &query).unwrap();
    assert_eq!(excluded.total, 0);
    assert_eq!(excluded.items.len(), 0);
    assert_eq!(excluded.next_offset, None);
}

#[test]
fn checkpoint_rejects_a_replaced_baseline_even_when_current_sources_match() {
    let file = catalog();
    let original = tracked(&file);
    let plan = workflow::plan_source_sync(ID, &file, &original, SyncMode::Review).unwrap();
    let mut replaced = original.clone();
    replaced
        .source_baseline
        .as_mut()
        .unwrap()
        .extra
        .insert("externalRevision".into(), json!(4));

    let error = workflow::plan_checkpoint(&file, &replaced, &plan).unwrap_err();

    assert!(
        matches!(error,XcStringsError::InvalidFormat(message) if message == "source_changed_during_sync: source baseline changed after the plan")
    );
    assert_eq!(
        replaced.source_baseline.as_ref().unwrap().extra["externalRevision"],
        json!(4)
    );
}

#[test]
fn source_only_key_without_localizations_checkpoints_without_inventing_targets() {
    let file =
        parser::parse(r#"{"sourceLanguage":"en","version":"1.0","strings":{"Open settings":{}}}"#)
            .unwrap();
    let document = WorkflowDocument::default();
    let plan = workflow::plan_source_sync(ID, &file, &document, SyncMode::Review).unwrap();
    assert!(plan.invalidation.is_none());
    assert_eq!(plan.report.untracked_keys, vec!["Open settings"]);
    assert_eq!(plan.report.diagnostics, vec![]);
    let checkpoint = workflow::plan_checkpoint(&file, &document, &plan).unwrap();
    assert_eq!(
        workflow::inspect(ID, &file, &checkpoint).unwrap().keys["Open settings"].freshness,
        SourceFreshness::Current
    );
    let page = workflow::review_queue(ID, &file, &checkpoint, &query()).unwrap();
    assert_eq!(page.total, 0);
    assert_eq!(page.items.len(), 0);
    assert_eq!(
        json_file(&file),
        json!({"sourceLanguage":"en","version":"1.0","strings":{"Open settings":{}}})
    );
}

#[test]
fn orphan_substitution_metadata_blocks_checkpoint_without_discarding_native_data() {
    let mut raw = json_file(&substitution_catalog());
    raw["strings"]["count"]["localizations"]["de"]["stringUnit"]["value"] = json!("Dinge");
    let file = parser::parse(&raw.to_string()).unwrap();
    let document = WorkflowDocument::default();
    let plan = workflow::plan_source_sync(ID, &file, &document, SyncMode::Review).unwrap();
    assert!(plan.invalidation.is_none());
    assert_eq!(
        plan.report
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec![WorkflowCode::UnsupportedShape]
    );
    assert!(
        matches!(workflow::plan_checkpoint(&file,&document,&plan).unwrap_err(),XcStringsError::InvalidFormat(message) if message == "unsupported_shape: source checkpoint cannot certify an unsupported catalog shape")
    );
    assert_eq!(json_file(&file), raw);
}

#[test]
fn compiler_invalid_nested_device_branch_blocks_checkpoint_without_mutation() {
    let raw = json!({"sourceLanguage":"en","version":"1.0","strings":{"count":{"localizations":{"de":{"variations":{"device":{"other":{"variations":{"plural":{"other":{"stringUnit":{"state":"translated","value":"Dinge"}}}}}}}}}}}});
    let file = parser::parse(&raw.to_string()).unwrap();
    let document = WorkflowDocument::default();

    let plan = workflow::plan_source_sync(ID, &file, &document, SyncMode::Review).unwrap();

    assert!(plan.invalidation.is_none());
    assert_eq!(plan.report.diagnostics.len(), 1);
    assert_eq!(
        plan.report.diagnostics[0].code,
        WorkflowCode::UnsupportedShape
    );
    assert_eq!(
        plan.report.diagnostics[0].message,
        "device variation order is not supported by the Apple compiler"
    );
    assert!(
        matches!(workflow::plan_checkpoint(&file,&document,&plan).unwrap_err(),XcStringsError::InvalidFormat(message) if message == "unsupported_shape: source checkpoint cannot certify an unsupported catalog shape")
    );
    assert_eq!(json_file(&file), raw);
}

#[test]
fn externally_unknown_target_state_remains_visible_without_a_draft_reason() {
    let mut file = catalog();
    let document = tracked(&file);
    file.strings
        .get_mut("delete")
        .unwrap()
        .localizations
        .as_mut()
        .unwrap()
        .get_mut("de")
        .unwrap()
        .string_unit
        .as_mut()
        .unwrap()
        .state = xcstrings_mcp::model::xcstrings::TranslationState::Unknown("future_ready".into());

    let page = workflow::review_queue(ID, &file, &document, &query()).unwrap();

    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].destination.key, "delete");
    assert_eq!(page.items[0].freshness, SourceFreshness::Current);
    assert_eq!(page.items[0].reasons, vec![]);
    assert_eq!(
        page.items[0]
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec![WorkflowCode::NotDraft]
    );
    assert_eq!(
        page.items[0].native_state,
        xcstrings_mcp::model::xcstrings::TranslationState::Unknown("future_ready".into())
    );
}
