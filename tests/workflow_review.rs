mod workflow_support;
use std::borrow::Cow;
use workflow_support::*;
use xcstrings_mcp::model::workflow::*;
use xcstrings_mcp::model::xcstrings::TranslationState;
use xcstrings_mcp::service::workflow;

#[test]
fn uninitialized_view_preserves_native_readiness_without_claiming_tracking() {
    let file = catalog();
    let document = WorkflowDocument::default();
    let view = workflow::inspect(ID, &file, &document).unwrap();
    assert!(matches!(view.effective_catalog, Cow::Borrowed(_)));
    assert_eq!(view.tracking, TrackingStatus::Uninitialized);
    assert_eq!(view.keys["delete"].freshness, SourceFreshness::Untracked);
}

#[test]
fn stale_view_overlays_ready_leaves_without_mutating_native_catalog_or_drafts() {
    let mut file = catalog();
    let document = tracked(&file);
    set_source(&mut file, "Delete permanently");
    let before = json_file(&file);
    let view = workflow::inspect(ID, &file, &document).unwrap();
    assert!(matches!(view.effective_catalog, Cow::Owned(_)));
    assert_eq!(
        view.keys["delete"].freshness,
        SourceFreshness::SourceChanged
    );
    let effective = json_file(&view.effective_catalog);
    assert_eq!(
        effective["strings"]["delete"]["localizations"]["de"]["stringUnit"]["state"],
        "needs_review"
    );
    assert_eq!(
        effective["strings"]["save"]["localizations"]["de"]["stringUnit"]["state"],
        "translated"
    );
    assert_eq!(json_file(&file), before);
    let queue = workflow::review_queue(ID, &file, &document, &query()).unwrap();
    assert_eq!(queue.total, 1);
    assert_eq!(queue.items[0].native_state, TranslationState::Translated);
    assert_eq!(
        queue.items[0].effective_state,
        TranslationState::NeedsReview
    );
    assert_eq!(queue.items[0].reasons, vec![ReviewReason::SourceChanged]);
}

#[test]
fn approval_changes_only_state_and_old_token_retry_rejects() {
    let file = catalog();
    let document = tracked(&file);
    let request = approve_request(&file, &document, "delete", "fr");
    let plan =
        workflow::plan_approval(ID, &file, &document, std::slice::from_ref(&request)).unwrap();
    assert_eq!(plan.report.accepted, 1);
    assert_eq!(plan.report.rejected, vec![]);
    let approved = plan.candidate.unwrap();
    let mut expected = json_file(&file);
    expected["strings"]["delete"]["localizations"]["fr"]["stringUnit"]["state"] =
        serde_json::json!("translated");
    assert_eq!(json_file(&approved), expected);
    let retry = workflow::plan_approval(ID, &approved, &document, &[request]).unwrap();
    assert!(retry.candidate.is_none());
    assert_eq!(retry.report.accepted, 0);
    assert_eq!(
        retry.report.rejected[0].code,
        WorkflowCode::TargetVersionMismatch
    );
    let fresh = approve_request(&approved, &document, "delete", "fr");
    let again = workflow::plan_approval(ID, &approved, &document, &[fresh]).unwrap();
    assert_eq!(again.report.rejected[0].code, WorkflowCode::NotDraft);
}

#[test]
fn changed_source_rejects_approval_before_any_promotion() {
    let mut file = catalog();
    let document = tracked(&file);
    let request = approve_request(&file, &document, "delete", "fr");
    set_source(&mut file, "Delete permanently");
    let plan = workflow::plan_approval(ID, &file, &document, &[request]).unwrap();
    assert!(plan.candidate.is_none());
    assert_eq!(plan.report.accepted, 0);
    assert_eq!(
        plan.report.rejected[0].code,
        WorkflowCode::SourceVersionMismatch
    );
}

#[test]
fn changed_target_rejects_approval_even_if_source_still_matches() {
    let mut file = catalog();
    let document = tracked(&file);
    let request = approve_request(&file, &document, "delete", "fr");
    set_target(&mut file, "fr", "Effacer définitivement");
    let plan = workflow::plan_approval(ID, &file, &document, &[request]).unwrap();
    assert!(plan.candidate.is_none());
    assert_eq!(
        plan.report.rejected[0].code,
        WorkflowCode::TargetVersionMismatch
    );
}

#[test]
fn stale_physical_ready_leaf_cannot_be_approved_through_effective_overlay() {
    let mut file = catalog();
    let document = tracked(&file);
    set_source(&mut file, "Delete forever");
    let request = approve_request(&file, &document, "delete", "de");
    let plan = workflow::plan_approval(ID, &file, &document, &[request]).unwrap();
    assert!(plan.candidate.is_none());
    assert_eq!(
        plan.report.rejected[0].code,
        WorkflowCode::SourceCheckpointRequired
    );
}

#[test]
fn mixed_valid_and_invalid_approval_batch_is_atomic() {
    let file = catalog();
    let document = tracked(&file);
    let valid = approve_request(&file, &document, "delete", "fr");
    let invalid = approve_request(&file, &document, "delete", "de");
    let plan = workflow::plan_approval(ID, &file, &document, &[valid, invalid]).unwrap();
    assert!(plan.candidate.is_none());
    assert_eq!(plan.report.accepted, 0);
    assert_eq!(plan.report.accepted_destinations, vec![]);
    assert_eq!(plan.report.rejected.len(), 1);
    assert_eq!(plan.report.rejected[0].code, WorkflowCode::NotDraft);
}

#[test]
fn duplicate_approval_destinations_reject_entire_batch() {
    let file = catalog();
    let document = tracked(&file);
    let request = approve_request(&file, &document, "delete", "fr");
    let plan = workflow::plan_approval(ID, &file, &document, &[request.clone(), request]).unwrap();
    assert!(plan.candidate.is_none());
    assert_eq!(plan.report.accepted, 0);
    assert_eq!(
        plan.report.rejected[0].code,
        WorkflowCode::DuplicateDestination
    );
}

#[test]
fn blank_existing_draft_is_explicitly_approvable() {
    let mut file = catalog();
    set_target(&mut file, "fr", "");
    let document = tracked(&file);
    let request = approve_request(&file, &document, "delete", "fr");
    let plan = workflow::plan_approval(ID, &file, &document, &[request]).unwrap();
    assert_eq!(plan.report.accepted, 1);
    assert_eq!(
        json_file(&plan.candidate.unwrap())["strings"]["delete"]["localizations"]["fr"]["stringUnit"],
        serde_json::json!({"state":"translated","value":""})
    );
}

#[test]
fn later_queue_pages_require_full_input_revision_even_for_unrelated_ready_edits() {
    let mut file = catalog();
    let document = tracked(&file);
    let mut q = query();
    q.locale = None;
    q.batch_size = 1;
    let first = workflow::review_queue(ID, &file, &document, &q).unwrap();
    q.offset = 1;
    assert!(
        workflow::review_queue(ID, &file, &document, &q)
            .unwrap_err()
            .to_string()
            .contains("queue_version_required")
    );
    q.expected_queue_version = Some(first.queue_version);
    file.strings
        .get_mut("save")
        .unwrap()
        .localizations
        .as_mut()
        .unwrap()
        .get_mut("de")
        .unwrap()
        .string_unit
        .as_mut()
        .unwrap()
        .value = "Sichern".into();
    assert!(
        workflow::review_queue(ID, &file, &document, &q)
            .unwrap_err()
            .to_string()
            .contains("queue_version_mismatch")
    );
}

#[test]
fn current_tokens_cannot_approve_draft_before_changed_source_is_checkpointed() {
    let mut file = catalog();
    let document = tracked(&file);
    set_source(&mut file, "Delete permanently");
    let request = approve_request(&file, &document, "delete", "fr");
    let before = json_file(&file);

    let plan = workflow::plan_approval(ID, &file, &document, &[request]).unwrap();

    assert!(plan.candidate.is_none());
    assert_eq!(plan.report.accepted, 0);
    assert_eq!(plan.report.accepted_destinations, vec![]);
    assert_eq!(plan.report.rejected.len(), 1);
    assert_eq!(
        plan.report.rejected[0].code,
        WorkflowCode::SourceCheckpointRequired
    );
    assert_eq!(json_file(&file), before);
}
