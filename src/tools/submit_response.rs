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
