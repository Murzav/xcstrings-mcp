use crate::error::XcStringsError;
use crate::model::translation::{DetailedSubmitResult, SubmitResult, ValidationIssue};

pub(crate) fn to_value(
    result: SubmitResult,
    warnings: Vec<ValidationIssue>,
) -> Result<serde_json::Value, XcStringsError> {
    Ok(serde_json::to_value(DetailedSubmitResult {
        result,
        warnings,
    })?)
}

pub(crate) fn extend_unique(warnings: &mut Vec<ValidationIssue>, additional: Vec<ValidationIssue>) {
    for warning in additional {
        if !warnings.iter().any(|existing| {
            existing.key == warning.key
                && existing.issue_type == warning.issue_type
                && existing.message == warning.message
        }) {
            warnings.push(warning);
        }
    }
}
