use std::path::PathBuf;
use std::process::ExitCode;

use xcstrings_mcp::service::file_validator;

use super::common::{EXIT_OK, EXIT_VALIDATION_ISSUES, handle_error, load_file};

pub fn run(file: Option<PathBuf>, locale: Option<String>, json: bool) -> ExitCode {
    let (_path, parsed) = match load_file(file) {
        Ok(v) => v,
        Err(e) => return handle_error(e),
    };

    let reports = file_validator::validate_file(&parsed, locale.as_deref());

    if json {
        match serde_json::to_string_pretty(&reports) {
            Ok(out) => println!("{out}"),
            Err(e) => {
                eprintln!("error: failed to serialize: {e}");
                return ExitCode::from(super::common::EXIT_ERROR);
            }
        }
    } else {
        let total_errors: usize = reports.iter().map(|r| r.errors.len()).sum();
        let total_warnings: usize = reports.iter().map(|r| r.warnings.len()).sum();

        if total_errors == 0 && total_warnings == 0 {
            println!("No validation issues found.");
            return ExitCode::from(EXIT_OK);
        }

        for report in &reports {
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
        let reports = vec![ValidationReport {
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
