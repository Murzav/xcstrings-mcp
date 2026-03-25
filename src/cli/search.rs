use std::path::PathBuf;
use std::process::ExitCode;

use xcstrings_mcp::service::extractor;

use super::common::{EXIT_ERROR, EXIT_OK, handle_error, load_file};

pub fn run(
    pattern: String,
    file: Option<PathBuf>,
    locale: Option<String>,
    limit: usize,
    json: bool,
) -> ExitCode {
    let (_path, parsed) = match load_file(file) {
        Ok(v) => v,
        Err(e) => return handle_error(e),
    };

    let locale = locale.unwrap_or_else(|| parsed.source_language.clone());

    // Clamp limit to service max of 100
    let batch_size = limit.clamp(1, 100);

    let (results, total) = match extractor::search_keys(&parsed, &pattern, &locale, batch_size, 0) {
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
            println!("No keys matching \"{pattern}\".");
            return ExitCode::from(EXIT_OK);
        }

        println!("Found {total} key(s) matching \"{pattern}\":");
        println!();

        // Find target translations for display
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
            "{:<kw$}   {:<sw$}   Translation",
            "Key",
            "Source Text",
            kw = key_width,
            sw = src_width
        );
        println!("{}", "\u{2500}".repeat(key_width + src_width + 17));

        for unit in &results {
            // Look up translation value for the target locale
            let translation = parsed
                .strings
                .get(&unit.key)
                .and_then(|e| e.localizations.as_ref())
                .and_then(|locs| locs.get(&locale))
                .and_then(|loc| loc.string_unit.as_ref())
                .map(|su| su.value.as_str())
                .unwrap_or("\u{2014}");

            println!(
                "{:<kw$}   {:<sw$}   {translation}",
                unit.key,
                unit.source_text,
                kw = key_width,
                sw = src_width
            );
        }

        if total > results.len() {
            println!();
            println!("Showing {} of {total} results.", results.len());
        }
    }

    ExitCode::from(EXIT_OK)
}

#[cfg(test)]
mod tests {
    use xcstrings_mcp::model::translation::TranslationUnit;

    fn make_units() -> Vec<TranslationUnit> {
        vec![
            TranslationUnit {
                key: "common.cancel".to_string(),
                source_text: "Cancel".to_string(),
                target_locale: "uk".to_string(),
                comment: None,
                format_specifiers: vec![],
                has_plurals: false,
                has_substitutions: false,
            },
            TranslationUnit {
                key: "dialog.cancel_button".to_string(),
                source_text: "Cancel Action".to_string(),
                target_locale: "uk".to_string(),
                comment: None,
                format_specifiers: vec![],
                has_plurals: false,
                has_substitutions: false,
            },
        ]
    }

    #[test]
    fn json_output_is_valid() {
        let units = make_units();
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
    fn unit_fields_correct() {
        let units = make_units();
        assert_eq!(units[0].key, "common.cancel");
        assert_eq!(units[0].source_text, "Cancel");
        assert_eq!(units[1].target_locale, "uk");
    }
}
