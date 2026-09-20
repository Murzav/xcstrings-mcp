mod workflow_support;
use serde_json::json;
use workflow_support::*;
use xcstrings_mcp::{
    model::{
        workflow::*,
        xcstrings::{TranslationState, paths::LeafStep},
    },
    service::{parser, workflow},
};

fn rejects(
    file: &xcstrings_mcp::model::xcstrings::XcStringsFile,
    document: &WorkflowDocument,
    request: ApprovalRequest,
    code: WorkflowCode,
) {
    let before = json_file(file);
    let plan = workflow::plan_approval(ID, file, document, &[request]).unwrap();
    assert!(plan.candidate.is_none());
    assert_eq!(plan.report.accepted, 0);
    assert_eq!(plan.report.accepted_destinations, vec![]);
    assert_eq!(plan.report.rejected.len(), 1);
    assert_eq!(plan.report.rejected[0].code, code);
    assert_eq!(json_file(file), before);
}
#[test]
fn approval_rejects_unknown_key() {
    let file = catalog();
    let document = tracked(&file);
    let mut request = approve_request(&file, &document, "delete", "fr");
    request.key = "missing".into();
    rejects(&file, &document, request, WorkflowCode::UnknownKey);
}
#[test]
fn approval_rejects_source_locale() {
    let file = catalog();
    let document = tracked(&file);
    let request = approve_request(&file, &document, "delete", "en");
    rejects(&file, &document, request, WorkflowCode::SourceLocale);
}
#[test]
fn approval_rejects_nontranslatable_key() {
    let mut file = catalog();
    file.strings.get_mut("delete").unwrap().should_translate = false;
    let document = tracked(&file);
    let request = approve_request(&file, &document, "delete", "fr");
    rejects(&file, &document, request, WorkflowCode::NotTranslatable);
}
#[test]
fn approval_rejects_unknown_locale() {
    let file = catalog();
    let document = tracked(&file);
    let mut request = approve_request(&file, &document, "delete", "fr");
    request.locale = "zz".into();
    rejects(&file, &document, request, WorkflowCode::UnknownLocale);
}
#[test]
fn approval_rejects_absent_physical_target() {
    let file = catalog();
    let document = tracked(&file);
    let mut request = approve_request(&file, &document, "delete", "fr");
    request.locale = "es".into();
    rejects(&file, &document, request, WorkflowCode::MissingTarget);
}
#[test]
fn approval_rejects_unsupported_typed_path() {
    let file = catalog();
    let document = tracked(&file);
    let mut request = approve_request(&file, &document, "delete", "fr");
    request.path = vec![LeafStep::Plural("future".into())];
    rejects(&file, &document, request, WorkflowCode::UnsupportedShape);
}
#[test]
fn approval_rejects_mismatched_format_without_editing_value() {
    let mut file = catalog();
    set_source(&mut file, "Delete %@");
    let document = tracked(&file);
    let request = approve_request(&file, &document, "delete", "fr");
    rejects(&file, &document, request, WorkflowCode::FormatMismatch);
}
#[test]
fn approval_rejects_stale_authored_variable_binding() {
    let file = catalog();
    let document = WorkflowDocument {
        contexts: serde_json::from_value(json!({"delete":{"context":{"variables":[{"reference":{"argument":2},"meaning":"Unused input"}]}}})).unwrap(),
        ..Default::default()
    };
    let sync = workflow::plan_source_sync(ID, &file, &document, SyncMode::AdoptExisting).unwrap();
    let document = workflow::plan_checkpoint(&file, &document, &sync).unwrap();
    let request = approve_request(&file, &document, "delete", "fr");
    rejects(&file, &document, request, WorkflowCode::UnsupportedShape);
}
#[test]
fn approval_rejects_unknown_native_state_and_queue_explains_it() {
    let mut file = catalog();
    file.strings
        .get_mut("delete")
        .unwrap()
        .localizations
        .as_mut()
        .unwrap()
        .get_mut("fr")
        .unwrap()
        .string_unit
        .as_mut()
        .unwrap()
        .state = TranslationState::Unknown("future_ready".into());
    let document = tracked(&file);
    let request = approve_request(&file, &document, "delete", "fr");
    rejects(&file, &document, request, WorkflowCode::NotDraft);
    let page = workflow::review_queue(
        ID,
        &file,
        &document,
        &ReviewQuery {
            locale: Some("fr".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(
        page.items[0].native_state,
        TranslationState::Unknown("future_ready".into())
    );
    assert!(
        page.items[0]
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == WorkflowCode::NotDraft)
    );
}
#[test]
fn empty_approval_batch_has_no_candidate_or_writes() {
    let file = catalog();
    let document = tracked(&file);
    let plan = workflow::plan_approval(ID, &file, &document, &[]).unwrap();
    assert!(plan.candidate.is_none());
    assert_eq!(plan.report.accepted, 0);
    assert_eq!(plan.report.accepted_destinations, vec![]);
    assert_eq!(plan.report.rejected, vec![]);
}
#[test]
fn initialized_new_key_is_untracked_and_effectively_incomplete() {
    let file = catalog();
    let document = tracked(&file);
    let mut raw = json_file(&file);
    raw["strings"]["new"] =
        json!({"localizations":{"de":{"stringUnit":{"state":"translated","value":"Neu"}}}});
    let added = parser::parse(&raw.to_string()).unwrap();
    let view = workflow::inspect(ID, &added, &document).unwrap();
    assert_eq!(view.keys["new"].freshness, SourceFreshness::Untracked);
    assert_eq!(
        json_file(&view.effective_catalog)["strings"]["new"]["localizations"]["de"]["stringUnit"]["state"],
        "needs_review"
    );
    assert_eq!(
        json_file(&added)["strings"]["new"]["localizations"]["de"]["stringUnit"]["state"],
        "translated"
    );
}
#[test]
fn queue_boundaries_empty_last_page_and_filter_revision_are_explicit() {
    let file = catalog();
    let document = tracked(&file);
    let mut query = ReviewQuery {
        locale: Some("fr".into()),
        batch_size: 1,
        ..Default::default()
    };
    let first = workflow::review_queue(ID, &file, &document, &query).unwrap();
    assert_eq!(first.total, 1);
    assert_eq!(first.next_offset, None);
    query.offset = 1;
    query.expected_queue_version = Some(first.queue_version);
    let last = workflow::review_queue(ID, &file, &document, &query).unwrap();
    assert_eq!(last.total, 1);
    assert!(last.items.is_empty());
    assert_eq!(last.next_offset, None);
    query.reasons = vec![ReviewReason::Draft];
    assert!(
        workflow::review_queue(ID, &file, &document, &query)
            .unwrap_err()
            .to_string()
            .contains("queue_version_mismatch")
    );
}
#[test]
fn queue_batch_size_rejects_zero_and_one_above_limit() {
    let file = catalog();
    let document = tracked(&file);
    assert!(matches!(
        workflow::review_queue(
            ID,
            &file,
            &document,
            &ReviewQuery {
                batch_size: 0,
                ..Default::default()
            }
        ),
        Err(xcstrings_mcp::error::XcStringsError::InvalidBatchSize(_))
    ));
    assert!(matches!(
        workflow::review_queue(
            ID,
            &file,
            &document,
            &ReviewQuery {
                batch_size: 101,
                ..Default::default()
            }
        ),
        Err(xcstrings_mcp::error::XcStringsError::InvalidBatchSize(_))
    ));
    assert_eq!(
        workflow::review_queue(
            ID,
            &file,
            &document,
            &ReviewQuery {
                batch_size: 100,
                ..Default::default()
            }
        )
        .unwrap()
        .total,
        1
    );
}
