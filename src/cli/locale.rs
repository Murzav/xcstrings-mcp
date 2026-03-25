use std::path::PathBuf;
use std::process::ExitCode;

use serde::Serialize;
use xcstrings_mcp::service::locale;

use super::common::{EXIT_OK, handle_error, load_file, save_file};

#[derive(Serialize)]
struct AddLocaleResult {
    locale: String,
    keys_initialized: usize,
    dry_run: bool,
}

#[derive(Serialize)]
struct RemoveLocaleResult {
    locale: String,
    entries_affected: usize,
    dry_run: bool,
}

pub fn run_add(locale_code: String, file: Option<PathBuf>, dry_run: bool, json: bool) -> ExitCode {
    let (path, mut parsed) = match load_file(file) {
        Ok(v) => v,
        Err(e) => return handle_error(e),
    };

    match locale::add_locale(&mut parsed, &locale_code) {
        Ok(count) => {
            if json {
                let result = AddLocaleResult {
                    locale: locale_code,
                    keys_initialized: count,
                    dry_run,
                };
                match serde_json::to_string_pretty(&result) {
                    Ok(out) => println!("{out}"),
                    Err(e) => {
                        eprintln!("error: failed to serialize: {e}");
                        return ExitCode::from(super::common::EXIT_ERROR);
                    }
                }
            } else if dry_run {
                eprintln!("Would add locale '{locale_code}': {count} keys to initialize");
            } else {
                eprintln!("Added locale '{locale_code}': {count} keys initialized");
            }

            if !dry_run && let Err(e) = save_file(&path, &parsed) {
                return handle_error(e);
            }

            ExitCode::from(EXIT_OK)
        }
        Err(e) => handle_error(e),
    }
}

pub fn run_remove(
    locale_code: String,
    file: Option<PathBuf>,
    dry_run: bool,
    json: bool,
) -> ExitCode {
    let (path, mut parsed) = match load_file(file) {
        Ok(v) => v,
        Err(e) => return handle_error(e),
    };

    let source_language = parsed.source_language.clone();

    match locale::remove_locale(&mut parsed, &locale_code, &source_language) {
        Ok(count) => {
            if json {
                let result = RemoveLocaleResult {
                    locale: locale_code,
                    entries_affected: count,
                    dry_run,
                };
                match serde_json::to_string_pretty(&result) {
                    Ok(out) => println!("{out}"),
                    Err(e) => {
                        eprintln!("error: failed to serialize: {e}");
                        return ExitCode::from(super::common::EXIT_ERROR);
                    }
                }
            } else if dry_run {
                eprintln!("Would remove locale '{locale_code}': {count} entries affected");
            } else {
                eprintln!("Removed locale '{locale_code}': {count} entries affected");
            }

            if !dry_run && let Err(e) = save_file(&path, &parsed) {
                return handle_error(e);
            }

            ExitCode::from(EXIT_OK)
        }
        Err(e) => handle_error(e),
    }
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;
    use xcstrings_mcp::model::xcstrings::{
        Localization, StringEntry, StringUnit, TranslationState, XcStringsFile,
    };
    use xcstrings_mcp::service::locale;

    fn make_file(entries: Vec<(&str, StringEntry)>) -> XcStringsFile {
        XcStringsFile {
            source_language: "en".to_string(),
            strings: entries
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            version: "1.0".to_string(),
        }
    }

    fn translatable_entry(locales: &[(&str, TranslationState)]) -> StringEntry {
        let mut localizations = IndexMap::new();
        for (locale, state) in locales {
            localizations.insert(
                locale.to_string(),
                Localization {
                    string_unit: Some(StringUnit {
                        state: state.clone(),
                        value: format!("value_{locale}"),
                    }),
                    variations: None,
                    substitutions: None,
                },
            );
        }
        StringEntry {
            extraction_state: None,
            should_translate: true,
            comment: None,
            localizations: if localizations.is_empty() {
                None
            } else {
                Some(localizations)
            },
        }
    }

    #[test]
    fn add_locale_initializes_keys() {
        let mut file = make_file(vec![
            (
                "key1",
                translatable_entry(&[("en", TranslationState::Translated)]),
            ),
            (
                "key2",
                translatable_entry(&[("en", TranslationState::Translated)]),
            ),
        ]);

        let count = locale::add_locale(&mut file, "fr").unwrap();
        assert_eq!(count, 2);

        for entry in file.strings.values() {
            let locs = entry.localizations.as_ref().unwrap();
            assert!(locs.contains_key("fr"));
            let fr = locs.get("fr").unwrap();
            assert_eq!(
                fr.string_unit.as_ref().unwrap().state,
                TranslationState::New
            );
        }
    }

    #[test]
    fn remove_locale_source_language_error() {
        let mut file = make_file(vec![(
            "key1",
            translatable_entry(&[("en", TranslationState::Translated)]),
        )]);

        let result = locale::remove_locale(&mut file, "en", "en");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("en"), "error should mention the locale: {err}");
    }

    #[test]
    fn remove_locale_not_found_error() {
        let mut file = make_file(vec![(
            "key1",
            translatable_entry(&[("en", TranslationState::Translated)]),
        )]);

        let result = locale::remove_locale(&mut file, "zz", "en");
        assert!(result.is_err());
    }

    #[test]
    fn remove_locale_succeeds() {
        let mut file = make_file(vec![(
            "key1",
            translatable_entry(&[
                ("en", TranslationState::Translated),
                ("fr", TranslationState::Translated),
            ]),
        )]);

        let count = locale::remove_locale(&mut file, "fr", "en").unwrap();
        assert_eq!(count, 1);

        let locs = file.strings["key1"].localizations.as_ref().unwrap();
        assert!(!locs.contains_key("fr"));
    }
}
