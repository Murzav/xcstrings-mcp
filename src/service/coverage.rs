use std::collections::BTreeSet;

use crate::model::translation::{CoverageReport, LocaleCoverage};
use crate::model::xcstrings::XcStringsFile;
use crate::service::is_translated_for;

/// Calculate per-locale coverage for the entire file.
pub fn get_coverage(file: &XcStringsFile) -> CoverageReport {
    let total_keys = file.strings.len();
    let translatable_keys = file.strings.values().filter(|e| e.should_translate).count();

    let mut all_locales = BTreeSet::new();
    for entry in file.strings.values() {
        if let Some(locs) = &entry.localizations {
            for locale in locs.keys() {
                all_locales.insert(locale.clone());
            }
        }
    }

    let locales = all_locales
        .iter()
        .map(|locale| locale_coverage(file, locale, total_keys, translatable_keys))
        .collect();

    CoverageReport {
        source_language: file.source_language.clone(),
        total_keys,
        translatable_keys,
        locales,
    }
}

fn locale_coverage(
    file: &XcStringsFile,
    locale: &str,
    total_keys: usize,
    translatable_keys: usize,
) -> LocaleCoverage {
    let translated = file
        .strings
        .values()
        .filter(|e| e.should_translate)
        .filter(|e| is_translated_for(e, locale))
        .count();

    let percentage = if translatable_keys == 0 {
        0.0
    } else {
        (translated as f64 / translatable_keys as f64) * 100.0
    };

    LocaleCoverage {
        locale: locale.to_string(),
        total_keys,
        translatable_keys,
        translated,
        percentage,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use indexmap::IndexMap;

    use super::*;
    use crate::model::xcstrings::{
        Localization, StringEntry, StringUnit, TranslationState, Variations, XcStringsFile,
    };

    fn make_file(strings: IndexMap<String, StringEntry>) -> XcStringsFile {
        XcStringsFile {
            source_language: "en".to_string(),
            strings,
            version: "1.0".to_string(),
        }
    }

    fn make_entry(locales: &[(&str, TranslationState)]) -> StringEntry {
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

    fn make_nontranslatable_entry() -> StringEntry {
        StringEntry {
            extraction_state: None,
            should_translate: false,
            comment: None,
            localizations: None,
        }
    }

    #[test]
    fn empty_file_returns_zeroes() {
        let file = make_file(IndexMap::new());
        let report = get_coverage(&file);
        assert_eq!(report.total_keys, 0);
        assert_eq!(report.translatable_keys, 0);
        assert!(report.locales.is_empty());
    }

    #[test]
    fn all_translated_gives_100_percent() {
        let mut strings = IndexMap::new();
        strings.insert(
            "key1".to_string(),
            make_entry(&[
                ("en", TranslationState::Translated),
                ("de", TranslationState::Translated),
            ]),
        );
        strings.insert(
            "key2".to_string(),
            make_entry(&[
                ("en", TranslationState::Translated),
                ("de", TranslationState::Translated),
            ]),
        );
        let file = make_file(strings);
        let report = get_coverage(&file);

        let de = report.locales.iter().find(|l| l.locale == "de").unwrap();
        assert_eq!(de.translated, 2);
        assert!((de.percentage - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn partial_translation_correct_percentage() {
        let mut strings = IndexMap::new();
        strings.insert(
            "key1".to_string(),
            make_entry(&[
                ("en", TranslationState::Translated),
                ("de", TranslationState::Translated),
            ]),
        );
        strings.insert(
            "key2".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        let file = make_file(strings);
        let report = get_coverage(&file);

        let de = report.locales.iter().find(|l| l.locale == "de").unwrap();
        assert_eq!(de.translated, 1);
        assert_eq!(de.translatable_keys, 2);
        assert!((de.percentage - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn should_translate_false_excluded() {
        let mut strings = IndexMap::new();
        strings.insert(
            "key1".to_string(),
            make_entry(&[("de", TranslationState::Translated)]),
        );
        strings.insert("key2".to_string(), make_nontranslatable_entry());
        let file = make_file(strings);
        let report = get_coverage(&file);

        assert_eq!(report.total_keys, 2);
        assert_eq!(report.translatable_keys, 1);
        let de = report.locales.iter().find(|l| l.locale == "de").unwrap();
        assert_eq!(de.translatable_keys, 1);
        assert_eq!(de.translated, 1);
    }

    #[test]
    fn locale_in_some_keys_only() {
        let mut strings = IndexMap::new();
        strings.insert(
            "key1".to_string(),
            make_entry(&[
                ("en", TranslationState::Translated),
                ("de", TranslationState::Translated),
                ("fr", TranslationState::Translated),
            ]),
        );
        strings.insert(
            "key2".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        let file = make_file(strings);
        let report = get_coverage(&file);

        let fr = report.locales.iter().find(|l| l.locale == "fr").unwrap();
        assert_eq!(fr.translated, 1);
        assert_eq!(fr.translatable_keys, 2);
        assert!((fr.percentage - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn multiple_locales_sorted_alphabetically() {
        let mut strings = IndexMap::new();
        strings.insert(
            "key1".to_string(),
            make_entry(&[
                ("fr", TranslationState::Translated),
                ("de", TranslationState::Translated),
                ("ar", TranslationState::New),
            ]),
        );
        let file = make_file(strings);
        let report = get_coverage(&file);

        let locale_names: Vec<&str> = report.locales.iter().map(|l| l.locale.as_str()).collect();
        assert_eq!(locale_names, vec!["ar", "de", "fr"]);
    }

    #[test]
    fn variations_treated_as_translated() {
        let mut strings = IndexMap::new();
        let mut localizations = IndexMap::new();
        localizations.insert(
            "de".to_string(),
            Localization {
                string_unit: None,
                variations: Some(Variations {
                    plural: Some(BTreeMap::new()),
                    device: None,
                }),
                substitutions: None,
            },
        );
        strings.insert(
            "key1".to_string(),
            StringEntry {
                extraction_state: None,
                should_translate: true,
                comment: None,
                localizations: Some(localizations),
            },
        );
        let file = make_file(strings);
        let report = get_coverage(&file);

        let de = report.locales.iter().find(|l| l.locale == "de").unwrap();
        assert_eq!(de.translated, 1);
    }
}
