use super::{inspect, read, target_version};
use crate::{
    error::XcStringsError,
    model::{
        translation::CompletedTranslation,
        workflow::*,
        xcstrings::{
            TranslationState, XcStringsFile,
            paths::{LeafStep, validate_supported_path},
        },
    },
};
use std::collections::HashSet;

#[derive(Debug)]
pub struct ApprovalPlan {
    pub candidate: Option<XcStringsFile>,
    pub report: ApprovalReport,
}

pub fn plan_approval(
    catalog_identity: &str,
    file: &XcStringsFile,
    document: &WorkflowDocument,
    requests: &[ApprovalRequest],
) -> Result<ApprovalPlan, XcStringsError> {
    let view = inspect(catalog_identity, file, document)?;
    let mut report = ApprovalReport::default();
    let mut seen = HashSet::new();
    for request in requests {
        let destination = request.destination();
        if !seen.insert(destination.clone()) {
            report.rejected.push(super::diagnostic(
                WorkflowCode::DuplicateDestination,
                "duplicate approval destination",
                Some(destination),
                None,
            ));
        }
    }
    for (index, left) in requests.iter().enumerate() {
        for right in &requests[index + 1..] {
            if left.key == right.key
                && left.locale == right.locale
                && left.path != right.path
                && overlapping(&left.path, &right.path)
            {
                report.rejected.push(super::diagnostic(
                    WorkflowCode::OverlappingDestination,
                    "overlapping approval destinations",
                    Some(right.destination()),
                    None,
                ));
            }
        }
    }
    for request in requests {
        if let Some(error) = check_request(
            catalog_identity,
            file,
            document,
            &view,
            request,
            &mut report.warnings,
        )? {
            report.rejected.push(error);
        }
    }
    if !report.rejected.is_empty() || requests.is_empty() {
        return Ok(ApprovalPlan {
            candidate: None,
            report,
        });
    }
    // The clone is the atomic batch candidate; no accepted state escapes on rejection.
    let mut candidate = file.clone();
    for request in requests {
        let destination = request.destination();
        let Some(unit) = read::unit_mut(&mut candidate, &destination) else {
            return Err(super::invalid(
                "missing_target",
                "validated approval destination disappeared",
            ));
        };
        unit.state = TranslationState::Translated;
        report.accepted_destinations.push(destination);
    }
    report.accepted = requests.len();
    Ok(ApprovalPlan {
        candidate: Some(candidate),
        report,
    })
}
fn check_request(
    identity: &str,
    file: &XcStringsFile,
    document: &WorkflowDocument,
    view: &super::WorkflowView<'_>,
    request: &ApprovalRequest,
    warnings: &mut Vec<crate::model::translation::ValidationIssue>,
) -> Result<Option<WorkflowDiagnostic>, XcStringsError> {
    let destination = request.destination();
    let reject = |code, message: &str| {
        Some(super::diagnostic(
            code,
            message,
            Some(destination.clone()),
            None,
        ))
    };
    let Some(entry) = file.strings.get(&request.key) else {
        return Ok(reject(WorkflowCode::UnknownKey, "key not found"));
    };
    if request.locale == file.source_language {
        return Ok(reject(
            WorkflowCode::SourceLocale,
            "source locale cannot be approved",
        ));
    }
    if !entry.should_translate {
        return Ok(reject(
            WorkflowCode::NotTranslatable,
            "key is marked shouldTranslate=false",
        ));
    }
    if let Err(error) = crate::model::plural::plural_categories(&request.locale) {
        return Ok(reject(WorkflowCode::UnknownLocale, &error.to_string()));
    }
    if let Err(error) = validate_supported_path(&request.path) {
        return Ok(reject(WorkflowCode::UnsupportedShape, &error));
    }
    let status = &view.keys[&request.key];
    if request.expected_source_version != status.source_version {
        return Ok(reject(
            WorkflowCode::SourceVersionMismatch,
            "source or authored context changed; fetch current review item",
        ));
    }
    let Some(current_target_version) = target_version(identity, file, &destination)? else {
        return Ok(reject(
            WorkflowCode::MissingTarget,
            "approval requires an existing physical target leaf",
        ));
    };
    if request.expected_target_version != current_target_version {
        return Ok(reject(
            WorkflowCode::TargetVersionMismatch,
            "physical target changed; fetch current review item",
        ));
    }
    if status.freshness != SourceFreshness::Current {
        return Ok(reject(
            WorkflowCode::SourceCheckpointRequired,
            "synchronize source changes before approving this leaf",
        ));
    }
    let Some(unit) = read::unit(file, &destination) else {
        return Ok(reject(
            WorkflowCode::MissingTarget,
            "target leaf disappeared",
        ));
    };
    if !matches!(
        unit.state,
        TranslationState::New | TranslationState::NeedsReview
    ) {
        return Ok(reject(
            WorkflowCode::NotDraft,
            "only existing new/needs_review leaves can be approved",
        ));
    }
    if let Some(error) = read::shape_diagnostics(file, &request.key).first() {
        return Ok(reject(WorkflowCode::UnsupportedShape, &error.message));
    }
    let context_issues =
        crate::service::context::validate_context_bindings(file, &document.contexts);
    if let Some(error) = context_issues.iter().find(|error| error.key == request.key) {
        return Ok(reject(WorkflowCode::UnsupportedShape, &error.detail));
    }
    let translation = CompletedTranslation {
        key: request.key.clone(),
        locale: request.locale.clone(),
        value: unit.value.clone(),
        expected_source_version: request.expected_source_version.clone(),
        path: Some(request.path.clone()),
        ..Default::default()
    };
    let validation =
        crate::service::validator::validate_translations_detailed(file, &[translation]);
    warnings.extend(validation.warnings);
    if let Some(error) = validation.rejected.first() {
        return Ok(reject(WorkflowCode::FormatMismatch, &error.reason));
    }
    Ok(None)
}
fn overlapping(left: &[LeafStep], right: &[LeafStep]) -> bool {
    let common = left.iter().zip(right).take_while(|(a, b)| a == b).count();
    match (left.get(common), right.get(common)) {
        (None, Some(step)) | (Some(step), None) => !matches!(step, LeafStep::Substitution(_)),
        (Some(LeafStep::Device(_)), Some(LeafStep::Device(_)))
        | (Some(LeafStep::Plural(_)), Some(LeafStep::Plural(_)))
        | (Some(LeafStep::Substitution(_)), Some(LeafStep::Substitution(_))) => false,
        (Some(LeafStep::Substitution(_)), Some(_)) | (Some(_), Some(LeafStep::Substitution(_))) => {
            false
        }
        (Some(_), Some(_)) => true,
        (None, None) => false,
    }
}
