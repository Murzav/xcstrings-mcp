use std::collections::HashSet;

use crate::model::translation::ValidationIssue;
use crate::model::xcstrings::XcStringsFile;
use crate::service::file_validator;

use super::MergeValidationIssue;

pub(super) fn validation_delta(
    current: &XcStringsFile,
    result: &XcStringsFile,
) -> (Vec<MergeValidationIssue>, Vec<MergeValidationIssue>) {
    let current_set = flatten(current).into_iter().collect::<HashSet<_>>();
    let (existing, introduced) = flatten(result)
        .into_iter()
        .partition(|issue| current_set.contains(issue));
    (existing, introduced)
}

fn flatten(file: &XcStringsFile) -> Vec<MergeValidationIssue> {
    let mut issues = Vec::new();
    for report in file_validator::validate_file(file, None) {
        issues.extend(
            report
                .errors
                .into_iter()
                .map(|issue| convert("error", &report.locale, issue)),
        );
        issues.extend(
            report
                .warnings
                .into_iter()
                .map(|issue| convert("warning", &report.locale, issue)),
        );
    }
    issues.sort_by(|left, right| {
        (
            &left.severity,
            &left.locale,
            &left.key,
            &left.issue_type,
            &left.message,
        )
            .cmp(&(
                &right.severity,
                &right.locale,
                &right.key,
                &right.issue_type,
                &right.message,
            ))
    });
    issues
}

fn convert(severity: &str, locale: &str, issue: ValidationIssue) -> MergeValidationIssue {
    MergeValidationIssue {
        severity: severity.to_string(),
        locale: locale.to_string(),
        key: issue.key,
        issue_type: issue.issue_type,
        message: issue.message,
    }
}
