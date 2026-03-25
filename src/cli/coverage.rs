use std::path::PathBuf;
use std::process::ExitCode;

use xcstrings_mcp::service::coverage as coverage_svc;

use super::common::{EXIT_OK, handle_error, load_file};

pub fn run(file: Option<PathBuf>, locale: Option<String>, json: bool) -> ExitCode {
    let (_path, parsed) = match load_file(file) {
        Ok(v) => v,
        Err(e) => return handle_error(e),
    };

    let mut report = coverage_svc::get_coverage(&parsed);

    if let Some(ref loc) = locale {
        report.locales.retain(|lc| lc.locale == *loc);
    }

    if json {
        match serde_json::to_string_pretty(&report) {
            Ok(out) => println!("{out}"),
            Err(e) => {
                eprintln!("error: failed to serialize: {e}");
                return ExitCode::from(super::common::EXIT_ERROR);
            }
        }
    } else {
        println!(
            "Source: {} | Keys: {} ({} translatable)",
            report.source_language, report.total_keys, report.translatable_keys
        );
        println!();

        if report.locales.is_empty() {
            println!("No locales found.");
        } else {
            // Header
            println!(
                "{:<14} {:>10}   {:>5}   {:>8}",
                "Locale", "Translated", "Total", "Coverage"
            );
            println!("{}", "\u{2500}".repeat(45));

            for lc in &report.locales {
                println!(
                    "{:<14} {:>10}   {:>5}   {:>7.1}%",
                    lc.locale, lc.translated, lc.translatable_keys, lc.percentage
                );
            }
        }
    }

    ExitCode::from(EXIT_OK)
}

#[cfg(test)]
mod tests {
    use xcstrings_mcp::model::translation::{CoverageReport, LocaleCoverage};

    fn make_report() -> CoverageReport {
        CoverageReport {
            source_language: "en".to_string(),
            total_keys: 42,
            translatable_keys: 40,
            locales: vec![
                LocaleCoverage {
                    locale: "en".to_string(),
                    total_keys: 42,
                    translatable_keys: 40,
                    translated: 40,
                    percentage: 100.0,
                },
                LocaleCoverage {
                    locale: "uk".to_string(),
                    total_keys: 42,
                    translatable_keys: 40,
                    translated: 35,
                    percentage: 87.5,
                },
            ],
        }
    }

    #[test]
    fn json_output_is_valid() {
        let report = make_report();
        let json = serde_json::to_string_pretty(&report).expect("serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(parsed["source_language"], "en");
        assert_eq!(parsed["locales"].as_array().expect("array").len(), 2);
    }

    #[test]
    fn empty_locales() {
        let report = CoverageReport {
            source_language: "en".to_string(),
            total_keys: 0,
            translatable_keys: 0,
            locales: vec![],
        };
        assert!(report.locales.is_empty());
        let json = serde_json::to_string_pretty(&report).expect("serialize");
        assert!(json.contains("\"locales\": []"));
    }

    #[test]
    fn coverage_percentage_values() {
        let report = make_report();
        let en = &report.locales[0];
        assert!((en.percentage - 100.0).abs() < f64::EPSILON);
        let uk = &report.locales[1];
        assert!((uk.percentage - 87.5).abs() < f64::EPSILON);
    }
}
