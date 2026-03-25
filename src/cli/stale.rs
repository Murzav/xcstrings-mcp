use std::path::PathBuf;
use std::process::ExitCode;

use xcstrings_mcp::service::extractor;

use super::common::{EXIT_ERROR, EXIT_OK, handle_error, load_file};

pub fn run(file: Option<PathBuf>, locale: Option<String>, limit: usize, json: bool) -> ExitCode {
    let (_path, parsed) = match load_file(file) {
        Ok(v) => v,
        Err(e) => return handle_error(e),
    };

    let locale = locale.unwrap_or_else(|| parsed.source_language.clone());

    let (results, total) = match extractor::get_stale(&parsed, &locale, limit, 0) {
        Ok(v) => v,
        Err(e) => return handle_error(e),
    };

    if json {
        match serde_json::to_string_pretty(&results) {
            Ok(out) => println!("{out}"),
            Err(e) => {
                eprintln!("error: failed to serialize: {e}");
                return ExitCode::from(EXIT_ERROR);
            }
        }
    } else {
        if results.is_empty() {
            println!("No stale keys found.");
            return ExitCode::from(EXIT_OK);
        }

        println!("Found {total} stale key(s):");
        println!();

        let key_width = results
            .iter()
            .map(|u| u.key.len())
            .max()
            .unwrap_or(3)
            .max(3);
        let src_width = results
            .iter()
            .map(|u| u.source_text.len())
            .max()
            .unwrap_or(11)
            .max(11);

        println!(
            "{:<kw$}   {:<sw$}",
            "Key",
            "Source Text",
            kw = key_width,
            sw = src_width
        );
        println!("{}", "\u{2500}".repeat(key_width + src_width + 3));

        for unit in &results {
            println!(
                "{:<kw$}   {:<sw$}",
                unit.key,
                unit.source_text,
                kw = key_width,
                sw = src_width
            );
        }
    }

    ExitCode::from(EXIT_OK)
}

#[cfg(test)]
mod tests {
    use xcstrings_mcp::model::translation::TranslationUnit;

    fn make_stale_units() -> Vec<TranslationUnit> {
        vec![
            TranslationUnit {
                key: "old.feature".to_string(),
                source_text: "Old Feature Text".to_string(),
                target_locale: "en".to_string(),
                comment: None,
                format_specifiers: vec![],
                has_plurals: false,
                has_substitutions: false,
            },
            TranslationUnit {
                key: "deprecated.setting".to_string(),
                source_text: "Deprecated Setting".to_string(),
                target_locale: "en".to_string(),
                comment: None,
                format_specifiers: vec![],
                has_plurals: false,
                has_substitutions: false,
            },
        ]
    }

    #[test]
    fn json_output_is_valid() {
        let units = make_stale_units();
        let json = serde_json::to_string_pretty(&units).expect("serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(parsed.as_array().expect("array").len(), 2);
    }

    #[test]
    fn empty_results() {
        let units: Vec<TranslationUnit> = vec![];
        let json = serde_json::to_string_pretty(&units).expect("serialize");
        assert_eq!(json, "[]");
    }

    #[test]
    fn stale_unit_fields() {
        let units = make_stale_units();
        assert_eq!(units[0].key, "old.feature");
        assert_eq!(units[1].source_text, "Deprecated Setting");
    }
}
