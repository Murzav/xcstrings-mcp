use crate::{
    error::XcStringsError,
    model::translation::CompletedTranslation,
    service::{
        submission,
        validator::{self, TranslationValidationReport},
        workflow,
    },
    workflow_operation::CatalogSnapshot,
};

/// Check caller-captured source inputs before any technical target validation.
pub(super) fn guarded(
    snapshot: &CatalogSnapshot,
    requests: &[CompletedTranslation],
) -> Result<TranslationValidationReport, XcStringsError> {
    let view = workflow::inspect(
        snapshot.identity_text()?,
        &snapshot.catalog,
        &snapshot.workflow,
    )?;
    let mut report = TranslationValidationReport::default();
    let mut current = Vec::new();
    let mut original_indices = Vec::new();
    for (index, request) in requests.iter().enumerate() {
        if view
            .keys
            .get(&request.key)
            .is_some_and(|status| request.expected_source_version != status.source_version)
        {
            report.rejected_indices.push(index);
            report.rejected.push(submission::reject(request,"source_version_mismatch","source or authored context changed; fetch a current source_version before submitting"));
        } else {
            original_indices.push(index);
            current.push(request.clone());
        }
    }
    let validation = validator::validate_translations_detailed(&snapshot.catalog, &current);
    report.warnings = validation.warnings;
    report.format_errors = validation.format_errors;
    for (index, rejection) in validation
        .rejected_indices
        .into_iter()
        .zip(validation.rejected)
    {
        report.rejected_indices.push(original_indices[index]);
        report.rejected.push(rejection);
    }
    Ok(report)
}
