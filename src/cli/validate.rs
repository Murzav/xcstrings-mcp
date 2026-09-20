use std::path::PathBuf;
use std::process::ExitCode;

use super::common::{EXIT_OK, EXIT_VALIDATION_ISSUES, handle_error, load_snapshot};

pub fn run(
    file: Option<PathBuf>,
    locale: Option<String>,
    json: bool,
    glossary_path: &std::path::Path,
) -> ExitCode {
    let snapshot = match load_snapshot(file) {
        Ok(v) => v,
        Err(e) => return handle_error(e),
    };

    let guidance = xcstrings_mcp::guidance_operation::GuidanceSnapshot::load(
        &xcstrings_mcp::io::fs::FsFileStore::new(),
        glossary_path,
    );
    let report = match xcstrings_mcp::workflow_operation::read::validate_catalog(
        &snapshot,
        &guidance,
        locale.as_deref(),
    ) {
        Ok(report) => report,
        Err(error) => return handle_error(error),
    };
    let reports = &report.reports;

    if json {
        match serde_json::to_string_pretty(&report) {
            Ok(out) => println!("{out}"),
            Err(e) => {
                eprintln!("error: failed to serialize: {e}");
                return ExitCode::from(super::common::EXIT_ERROR);
            }
        }
    } else {
        super::common::print_tracking(
            report.tracking,
            report.untracked_keys.len(),
            report.source_changed_keys.len(),
        );
        if report.tracking == xcstrings_mcp::model::workflow::TrackingStatus::Initialized
            && !report.untracked_keys.is_empty()
        {
            println!(
                "Untracked source keys: {}",
                report.untracked_keys.join(", ")
            );
        }
        let total_errors: usize = reports.iter().map(|r| r.errors.len()).sum();
        let total_warnings: usize = reports.iter().map(|r| r.warnings.len()).sum();

        for issue in &report.terminology.issues {
            println!("  TERMINOLOGY  key {:?}: {}", issue.key, issue.detail);
        }
        if let Some(unavailable) = &report.terminology.unavailable {
            println!("Terminology QA unavailable: {}", unavailable.detail);
        }
        if !report.source_changed_keys.is_empty() {
            println!(
                "Changed source requires review: {}",
                report.source_changed_keys.join(", ")
            );
        }
        if total_errors == 0
            && total_warnings == 0
            && report.terminology.issues.is_empty()
            && report.terminology.unavailable.is_none()
            && report.source_changed_keys.is_empty()
        {
            println!("No validation issues found.");
            return ExitCode::from(EXIT_OK);
        }

        for report in reports {
            if report.errors.is_empty() && report.warnings.is_empty() {
                continue;
            }

            println!("Locale: {}", report.locale);

            for issue in &report.errors {
                println!("  ERROR  key {:?}: {}", issue.key, issue.message);
            }
            for issue in &report.warnings {
                println!("  WARN   key {:?}: {}", issue.key, issue.message);
            }
            println!();
        }

        println!("Found {total_errors} error(s), {total_warnings} warning(s)");
    }

    if reports
        .iter()
        .any(|r| !r.errors.is_empty() || !r.warnings.is_empty())
        || !report.terminology.issues.is_empty()
        || report.terminology.unavailable.is_some()
        || !report.source_changed_keys.is_empty()
    {
        ExitCode::from(EXIT_VALIDATION_ISSUES)
    } else {
        ExitCode::from(EXIT_OK)
    }
}

#[cfg(test)]
mod tests {
    use xcstrings_mcp::model::translation::{ValidationIssue, ValidationReport};

    fn make_reports_with_issues() -> Vec<ValidationReport> {
        vec![
            ValidationReport {
                locale: "uk".to_string(),
                errors: vec![ValidationIssue {
                    key: "items_count".to_string(),
                    issue_type: "format_specifier_mismatch".to_string(),
                    message: "format specifier mismatch".to_string(),
                }],
                warnings: vec![ValidationIssue {
                    key: "empty_key".to_string(),
                    issue_type: "empty_translation".to_string(),
                    message: "empty translation value".to_string(),
                }],
            },
            ValidationReport {
                locale: "de".to_string(),
                errors: vec![ValidationIssue {
                    key: "greeting".to_string(),
                    issue_type: "missing_plural_form".to_string(),
                    message: "missing required plural form \"other\"".to_string(),
                }],
                warnings: vec![],
            },
        ]
    }

    #[test]
    fn json_output_is_valid() {
        let reports = make_reports_with_issues();
        let json = serde_json::to_string_pretty(&reports).expect("serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(parsed.as_array().expect("array").len(), 2);
    }

    #[test]
    fn counts_errors_and_warnings() {
        let reports = make_reports_with_issues();
        let total_errors: usize = reports.iter().map(|r| r.errors.len()).sum();
        let total_warnings: usize = reports.iter().map(|r| r.warnings.len()).sum();
        assert_eq!(total_errors, 2);
        assert_eq!(total_warnings, 1);
    }

    #[test]
    fn clean_report_has_no_issues() {
        let reports = [ValidationReport {
            locale: "de".to_string(),
            errors: vec![],
            warnings: vec![],
        }];
        let has_issues = reports
            .iter()
            .any(|r| !r.errors.is_empty() || !r.warnings.is_empty());
        assert!(!has_issues);
    }

    #[test]
    fn empty_reports_vec() {
        let reports: Vec<ValidationReport> = vec![];
        let json = serde_json::to_string_pretty(&reports).expect("serialize");
        assert_eq!(json, "[]");
    }
}
