use std::path::{Path, PathBuf};
use std::process::ExitCode;

use xcstrings_mcp::io::FileStore;
use xcstrings_mcp::io::fs::FsFileStore;
use xcstrings_mcp::xliff_operation::{ImportOptions, ImportResult, execute_import};

use super::common::{EXIT_ERROR, EXIT_OK, EXIT_VALIDATION_ISSUES, handle_error, load_file};

pub fn run(
    file: Option<PathBuf>,
    xliff_path: PathBuf,
    original: Option<String>,
    source_versions: PathBuf,
    dry_run: bool,
    json: bool,
    glossary_path: &Path,
) -> ExitCode {
    let (path, _) = match load_file(file) {
        Ok(value) => value,
        Err(error) => return handle_error(error),
    };
    let store = FsFileStore::new();
    let xml = match store.read(&xliff_path) {
        Ok(value) => value,
        Err(error) => return handle_error(error),
    };
    let versions = match store.read(&source_versions).and_then(|text| {
        let value = xcstrings_mcp::service::parser::parse_unique_json(&text)?;
        serde_json::from_value::<std::collections::BTreeMap<String, String>>(value)
            .map_err(Into::into)
    }) {
        Ok(value) => value,
        Err(error) => return handle_error(error),
    };
    match execute_import(
        &store,
        &path,
        &xml,
        ImportOptions {
            original: original.as_deref(),
            dry_run,
            expected_source_versions: &versions,
        },
        glossary_path,
    ) {
        Ok(outcome) => print_result(&outcome.result, &xliff_path, json),
        Err(error) => handle_error(error),
    }
}

fn print_result(result: &ImportResult, xliff_path: &Path, json: bool) -> ExitCode {
    if json {
        match serde_json::to_string_pretty(result) {
            Ok(output) => println!("{output}"),
            Err(error) => {
                eprintln!("error: failed to serialize: {error}");
                return ExitCode::from(EXIT_ERROR);
            }
        }
    } else {
        let action = if result.dry_run { "Dry run" } else { "Import" };
        eprintln!("{action} from {}", xliff_path.display());
        eprintln!("Accepted: {} translation leaves", result.report.accepted);
        eprintln!("Rejected: {}", result.report.rejected.len());
        eprintln!("Written: {}", result.written);
        for rejected in &result.report.rejected {
            eprintln!(
                "  unit {:?} [{}]: {}",
                rejected.unit_id, rejected.code, rejected.message
            );
        }
        for warning in &result.report.warnings {
            eprintln!(
                "  key {:?} [{}]: {}",
                warning.key, warning.issue_type, warning.message
            );
        }
        if let Some(unavailable) = &result.guidance.unavailable {
            eprintln!("Terminology QA unavailable: {}", unavailable.detail);
        }
        for issue in &result.guidance.issues {
            eprintln!(
                "Terminology: {}",
                serde_json::to_string(issue).unwrap_or_default()
            );
        }
        for scope in &result.report.skipped_scopes {
            eprintln!("Skipped file scope: {scope}");
        }
    }
    ExitCode::from(if result.report.rejected.is_empty() {
        EXIT_OK
    } else {
        EXIT_VALIDATION_ISSUES
    })
}

#[cfg(test)]
mod tests {
    use xcstrings_mcp::model::translation::CompletedTranslation;
    use xcstrings_mcp::service::{merger, parser, validator, xliff};

    const SIMPLE_FIXTURE: &str = include_str!("../../tests/fixtures/simple.xcstrings");

    #[test]
    fn import_xliff_roundtrip() {
        let file = parser::parse(SIMPLE_FIXTURE).unwrap();

        // Export to XLIFF
        let (xml, _) = xliff::export_xliff(&file, "de", "test.xcstrings", false).unwrap();

        // Parse XLIFF back
        let (locale, translations) = xliff::import_xliff(&xml).unwrap();
        assert_eq!(locale, "de");

        // Validate
        let rejected = validator::validate_translations(&file, &translations);
        assert!(rejected.is_empty(), "unexpected rejections: {rejected:?}");
    }

    #[test]
    fn import_validates_bad_translations() {
        let file = parser::parse(SIMPLE_FIXTURE).unwrap();

        let translations = vec![CompletedTranslation {
            key: "nonexistent_key".to_string(),
            expected_source_version: String::new(),
            locale: "de".to_string(),
            value: "Something".to_string(),
            plural_forms: None,
            substitution_name: None,
            path: None,
        }];

        let rejected = validator::validate_translations(&file, &translations);
        assert_eq!(rejected.len(), 1);
        assert!(rejected[0].reason.contains("key not found"));
    }

    #[test]
    fn dry_run_does_not_merge() {
        let mut file = parser::parse(SIMPLE_FIXTURE).unwrap();
        let original_json = serde_json::to_string(&file).unwrap();

        let translations = vec![CompletedTranslation {
            key: "greeting".to_string(),
            expected_source_version: String::new(),
            locale: "de".to_string(),
            value: "Hallo".to_string(),
            plural_forms: None,
            substitution_name: None,
            path: None,
        }];

        // Validate only (dry run path)
        let rejected = validator::validate_translations(&file, &translations);
        assert!(rejected.is_empty());

        // File unchanged
        let after_json = serde_json::to_string(&file).unwrap();
        assert_eq!(original_json, after_json);

        // Now actually merge
        let result = merger::merge_translations(&mut file, &translations);
        assert_eq!(result.accepted, 1);

        // File changed
        let merged_json = serde_json::to_string(&file).unwrap();
        assert_ne!(original_json, merged_json);
    }
}
