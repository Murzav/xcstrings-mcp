use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use xcstrings_mcp::io::FileStore;
use xcstrings_mcp::io::fs::FsFileStore;
use xcstrings_mcp::model::translation::SubmitResult;
use xcstrings_mcp::service::{formatter, merger, parser, validator, xliff};

use super::common::{EXIT_ERROR, EXIT_OK, EXIT_VALIDATION_ISSUES, handle_error, load_file};

pub fn run(file: Option<PathBuf>, xliff_path: PathBuf, dry_run: bool, json: bool) -> ExitCode {
    let (path, parsed) = match load_file(file) {
        Ok(v) => v,
        Err(e) => return handle_error(e),
    };

    let store = FsFileStore::new();

    let xliff_content = match store.read(&xliff_path) {
        Ok(v) => v,
        Err(e) => return handle_error(e),
    };

    let (_locale, translations) = match xliff::import_xliff(&xliff_content) {
        Ok(v) => v,
        Err(e) => return handle_error(e),
    };

    if translations.is_empty() {
        let result = SubmitResult {
            accepted: 0,
            accepted_keys: vec![],
            rejected: vec![],
            dry_run,
        };
        return print_result(&result, &xliff_path, json);
    }

    // Validate translations
    let rejected = validator::validate_translations(&parsed, &translations);
    let rejected_keys: HashSet<&str> = rejected.iter().map(|r| r.key.as_str()).collect();
    let valid: Vec<_> = translations
        .iter()
        .filter(|t| !rejected_keys.contains(t.key.as_str()))
        .cloned()
        .collect();

    if dry_run || valid.is_empty() {
        let result = SubmitResult {
            accepted: valid.len(),
            accepted_keys: valid.iter().map(|t| t.key.clone()).collect(),
            rejected,
            dry_run,
        };
        return print_result(&result, &xliff_path, json);
    }

    // Re-read file fresh (same pattern as MCP tool), merge, format, write
    let raw = match store.read(&path) {
        Ok(v) => v,
        Err(e) => return handle_error(e),
    };
    let mut fresh_file = match parser::parse(&raw) {
        Ok(v) => v,
        Err(e) => return handle_error(e),
    };

    let merge_result = merger::merge_translations(&mut fresh_file, &valid);

    let formatted = match formatter::format_xcstrings(&fresh_file) {
        Ok(v) => v,
        Err(e) => return handle_error(e),
    };
    if let Err(e) = store.write(&path, &formatted) {
        return handle_error(e);
    }

    let mut all_rejected = rejected;
    all_rejected.extend(merge_result.rejected);

    let result = SubmitResult {
        accepted: merge_result.accepted,
        accepted_keys: merge_result.accepted_keys,
        rejected: all_rejected,
        dry_run: false,
    };

    print_result(&result, &xliff_path, json)
}

fn print_result(result: &SubmitResult, xliff_path: &std::path::Path, json: bool) -> ExitCode {
    let has_rejected = !result.rejected.is_empty();

    if json {
        match serde_json::to_string_pretty(result) {
            Ok(out) => println!("{out}"),
            Err(e) => {
                eprintln!("error: failed to serialize: {e}");
                return ExitCode::from(EXIT_ERROR);
            }
        }
    } else {
        let xliff_display = xliff_path.display();
        if result.dry_run {
            eprintln!("Dry run — import from {xliff_display}");
            eprintln!("Would accept: {} translations", result.accepted);
        } else {
            eprintln!("Imported from {xliff_display}");
            eprintln!("Accepted: {} translations", result.accepted);
        }

        if has_rejected {
            eprintln!("Rejected: {} translations", result.rejected.len());
            for r in &result.rejected {
                eprintln!("  key \"{}\": {}", r.key, r.reason);
            }
        }
    }

    if has_rejected {
        ExitCode::from(EXIT_VALIDATION_ISSUES)
    } else {
        ExitCode::from(EXIT_OK)
    }
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
            locale: "de".to_string(),
            value: "Something".to_string(),
            plural_forms: None,
            substitution_name: None,
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
            locale: "de".to_string(),
            value: "Hallo".to_string(),
            plural_forms: None,
            substitution_name: None,
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
