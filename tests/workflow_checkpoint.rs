mod workflow_support;
use serde_json::json;
use workflow_support::*;
use xcstrings_mcp::model::workflow::*;
use xcstrings_mcp::service::workflow;

#[test]
fn strict_initialization_cannot_checkpoint_ready_targets_without_invalidation() {
    let file = catalog();
    let document = WorkflowDocument::default();
    let plan = workflow::plan_source_sync(ID, &file, &document, SyncMode::Review).unwrap();
    assert_eq!(plan.report.invalidated_destinations.len(), 2);
    assert!(
        workflow::plan_checkpoint(&file, &document, &plan)
            .unwrap_err()
            .to_string()
            .contains("ready_targets_require_invalidation")
    );
    let candidate = plan.invalidation.as_ref().unwrap();
    let checkpoint = workflow::plan_checkpoint(candidate, &document, &plan).unwrap();
    assert!(checkpoint.source_baseline.is_some());
    assert_eq!(
        workflow::inspect(ID, candidate, &checkpoint).unwrap().keys["delete"].freshness,
        SourceFreshness::Current
    );
}

#[test]
fn adoption_is_explicit_first_use_only_and_preserves_ready_states() {
    let file = catalog();
    let document = WorkflowDocument::default();
    let plan = workflow::plan_source_sync(ID, &file, &document, SyncMode::AdoptExisting).unwrap();
    assert!(plan.invalidation.is_none());
    assert!(plan.report.historical_freshness_unknown);
    let checkpoint = workflow::plan_checkpoint(&file, &document, &plan).unwrap();
    assert!(
        workflow::plan_source_sync(ID, &file, &checkpoint, SyncMode::AdoptExisting)
            .unwrap_err()
            .to_string()
            .contains("adopt_requires_uninitialized")
    );
}

#[test]
fn checkpoint_retains_earliest_origin_and_latest_source_for_unchanged_draft() {
    let mut file = catalog();
    let first = tracked(&file);
    let old = workflow::source_snapshot(&file, &first, "delete").unwrap();
    set_source(&mut file, "Delete permanently");
    let plan = workflow::plan_source_sync(ID, &file, &first, SyncMode::Review).unwrap();
    let invalidated = plan.invalidation.as_ref().unwrap().clone();
    let second = workflow::plan_checkpoint(&invalidated, &first, &plan).unwrap();
    let restored = workflow::parse_document(&workflow::format_document(&second).unwrap()).unwrap();
    let queue = workflow::review_queue(ID, &invalidated, &restored, &query()).unwrap();
    assert_eq!(queue.items[0].old_source.as_ref(), Some(&old));
    assert_eq!(
        queue.items[0].reasons,
        vec![ReviewReason::Draft, ReviewReason::SourceChanged]
    );
    let mut latest = invalidated;
    set_source(&mut latest, "Delete this account permanently");
    let plan = workflow::plan_source_sync(ID, &latest, &restored, SyncMode::Review).unwrap();
    let third = workflow::plan_checkpoint(&latest, &restored, &plan).unwrap();
    let queue = workflow::review_queue(ID, &latest, &third, &query()).unwrap();
    assert_eq!(queue.items[0].old_source.as_ref(), Some(&old));
    assert_eq!(
        queue.items[0].current_source,
        workflow::source_snapshot(&latest, &third, "delete").unwrap()
    );
}

#[test]
fn changed_draft_content_does_not_inherit_another_texts_origin() {
    let mut file = catalog();
    let document = tracked(&file);
    set_source(&mut file, "Delete permanently");
    let plan = workflow::plan_source_sync(ID, &file, &document, SyncMode::Review).unwrap();
    let mut candidate = plan.invalidation.as_ref().unwrap().clone();
    let checkpoint = workflow::plan_checkpoint(&candidate, &document, &plan).unwrap();
    set_target(&mut candidate, "de", "Dauerhaft löschen");
    let queue = workflow::review_queue(ID, &candidate, &checkpoint, &query()).unwrap();
    assert_eq!(queue.items[0].reasons, vec![ReviewReason::Draft]);
    assert!(queue.items[0].old_source.is_none());
}

#[test]
fn checkpoint_refuses_source_or_context_change_after_captured_plan() {
    let mut file = catalog();
    let document = WorkflowDocument::default();
    let plan = workflow::plan_source_sync(ID, &file, &document, SyncMode::AdoptExisting).unwrap();
    set_source(&mut file, "Different source");
    assert!(
        workflow::plan_checkpoint(&file, &document, &plan)
            .unwrap_err()
            .to_string()
            .contains("source_changed_during_sync")
    );
    let file = catalog();
    let mut context_changed = document;
    context_changed.contexts =
        serde_json::from_value(json!({"delete":{"context":{"purpose":"different meaning"}}}))
            .unwrap();
    assert!(
        workflow::plan_checkpoint(&file, &context_changed, &plan)
            .unwrap_err()
            .to_string()
            .contains("source_changed_during_sync")
    );
}

#[test]
fn checkpoint_preserves_contexts_and_future_metadata() {
    let file = catalog();
    let mut document=workflow::parse_document(r#"{"version":1,"contexts":{"delete":{"context":{"purpose":"Confirm removal"},"futureContext":7}},"futureRoot":{"state":"keep"}}"#).unwrap();
    let plan = workflow::plan_source_sync(ID, &file, &document, SyncMode::AdoptExisting).unwrap();
    document = workflow::plan_checkpoint(&file, &document, &plan).unwrap();
    document
        .source_baseline
        .as_mut()
        .unwrap()
        .extra
        .insert("futureBaseline".into(), json!([1, 2]));
    let mut next = file;
    set_source(&mut next, "Delete permanently");
    let plan = workflow::plan_source_sync(ID, &next, &document, SyncMode::Review).unwrap();
    let candidate = plan.invalidation.as_ref().unwrap();
    let updated = workflow::plan_checkpoint(candidate, &document, &plan).unwrap();
    assert_eq!(updated.contexts, document.contexts);
    assert_eq!(updated.extra, document.extra);
    assert_eq!(
        updated.source_baseline.as_ref().unwrap().extra,
        document.source_baseline.as_ref().unwrap().extra
    );
}

#[test]
fn workflow_document_rejects_duplicate_members_unknown_versions_and_malformed_snapshots() {
    assert!(
        workflow::parse_document(r#"{"version":1,"version":1}"#)
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
    assert!(
        workflow::parse_document(r#"{"version":2}"#)
            .unwrap_err()
            .to_string()
            .contains("version")
    );
    assert!(
        workflow::parse_document(r#"{"version":1,"source_baseline":{"sources":{"k":{}}}}"#)
            .unwrap_err()
            .to_string()
            .contains("snapshot")
    );
}

#[test]
fn unknown_initial_origin_remains_unknown_across_later_source_changes() {
    let file = catalog();
    let document = WorkflowDocument::default();
    let first = workflow::plan_source_sync(ID, &file, &document, SyncMode::Review).unwrap();
    let mut invalidated = first.invalidation.as_ref().unwrap().clone();
    let checkpoint = workflow::plan_checkpoint(&invalidated, &document, &first).unwrap();
    set_source(&mut invalidated, "A later source");
    let second =
        workflow::plan_source_sync(ID, &invalidated, &checkpoint, SyncMode::Review).unwrap();
    let checkpoint = workflow::plan_checkpoint(&invalidated, &checkpoint, &second).unwrap();
    let page = workflow::review_queue(ID, &invalidated, &checkpoint, &query()).unwrap();
    assert_eq!(page.items[0].old_source, None);
    assert_eq!(
        page.items[0].reasons,
        vec![ReviewReason::Draft, ReviewReason::Untracked]
    );
}

#[test]
fn repeat_checkpoint_is_idempotent_and_approval_resolves_matching_evidence() {
    let mut file = catalog();
    let document = tracked(&file);
    set_source(&mut file, "Delete permanently");
    let plan = workflow::plan_source_sync(ID, &file, &document, SyncMode::Review).unwrap();
    let file = plan.invalidation.as_ref().unwrap();
    let checkpoint = workflow::plan_checkpoint(file, &document, &plan).unwrap();
    let repeat = workflow::plan_source_sync(ID, file, &checkpoint, SyncMode::Review).unwrap();
    assert!(!repeat.report.checkpoint_needed);
    assert!(repeat.invalidation.is_none());
    assert_eq!(
        workflow::plan_checkpoint(file, &checkpoint, &repeat).unwrap(),
        checkpoint
    );
    let request = approve_request(file, &checkpoint, "delete", "de");
    let approved = workflow::plan_approval(ID, file, &checkpoint, &[request])
        .unwrap()
        .candidate
        .unwrap();
    assert_eq!(
        workflow::review_queue(ID, &approved, &checkpoint, &query())
            .unwrap()
            .total,
        0
    );
    let prune = workflow::plan_source_sync(ID, &approved, &checkpoint, SyncMode::Review).unwrap();
    assert!(prune.report.checkpoint_needed);
    let pruned = workflow::plan_checkpoint(&approved, &checkpoint, &prune).unwrap();
    assert_eq!(pruned.pending_review.len(), 1);
    assert_eq!(pruned.pending_review[0].destination.locale, "fr");
}

#[test]
fn removed_keys_are_reported_and_context_is_never_silently_rebound() {
    let mut file = catalog();
    let mut document = tracked(&file);
    document.contexts =
        serde_json::from_value(json!({"delete":{"context":{"purpose":"Removal"}}})).unwrap();
    file.strings.shift_remove("delete");
    let plan = workflow::plan_source_sync(ID, &file, &document, SyncMode::Review).unwrap();
    assert_eq!(plan.report.removed_keys, vec!["delete"]);
    assert_eq!(
        plan.report
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>(),
        vec![
            WorkflowCode::StaleBaselineKey,
            WorkflowCode::StaleContextKey
        ]
    );
    let checkpoint = workflow::plan_checkpoint(&file, &document, &plan).unwrap();
    assert_eq!(checkpoint.contexts, document.contexts);
    assert_eq!(
        checkpoint
            .source_baseline
            .as_ref()
            .unwrap()
            .sources
            .keys()
            .collect::<Vec<_>>(),
        vec!["save"]
    );
}

#[test]
fn unsupported_target_shape_never_advances_source_checkpoint() {
    let file = catalog();
    let mut raw = json_file(&file);
    raw["strings"]["delete"]["localizations"]["de"]["variations"] =
        json!({"future_axis":{"branch":{"stringUnit":{"state":"translated","value":"Unknown"}}}});
    let invalid = xcstrings_mcp::service::parser::parse(&raw.to_string()).unwrap();
    let document = WorkflowDocument::default();
    let plan = workflow::plan_source_sync(ID, &invalid, &document, SyncMode::Review).unwrap();
    assert!(plan.invalidation.is_none());
    assert_eq!(plan.report.invalidated_destinations, vec![]);
    assert!(
        plan.report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == WorkflowCode::UnsupportedShape)
    );
    assert!(
        workflow::plan_checkpoint(&invalid, &document, &plan)
            .unwrap_err()
            .to_string()
            .contains("unsupported_shape")
    );
}

#[test]
fn unknown_pending_origin_stays_unknown_before_next_source_checkpoint() {
    let file = catalog();
    let document = WorkflowDocument::default();
    let initial = workflow::plan_source_sync(ID, &file, &document, SyncMode::Review).unwrap();
    let mut invalidated = initial.invalidation.as_ref().unwrap().clone();
    let checkpoint = workflow::plan_checkpoint(&invalidated, &document, &initial).unwrap();
    set_source(&mut invalidated, "Later wording before synchronization");

    let page = workflow::review_queue(ID, &invalidated, &checkpoint, &query()).unwrap();

    assert_eq!(page.total, 2);
    assert_eq!(page.items[0].destination.key, "delete");
    assert_eq!(page.items[0].old_source, None);
    assert_eq!(
        page.items[0].reasons,
        vec![
            ReviewReason::Draft,
            ReviewReason::SourceChanged,
            ReviewReason::Untracked
        ]
    );
}
