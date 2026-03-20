use std::collections::BTreeSet;

use crate::error::XcStringsError;
use crate::model::translation::LocaleInfo;
use crate::model::xcstrings::{Localization, StringUnit, TranslationState, XcStringsFile};
use crate::service::is_translated_for;

/// List all locales found across all entries with translation statistics.
pub fn list_locales(file: &XcStringsFile) -> Vec<LocaleInfo> {
    let mut all_locales = BTreeSet::new();
    for entry in file.strings.values() {
        if let Some(locs) = &entry.localizations {
            for locale in locs.keys() {
                all_locales.insert(locale.clone());
            }
        }
    }

    let total = file.strings.values().filter(|e| e.should_translate).count();

    all_locales
        .into_iter()
        .map(|locale| {
            let translated = file
                .strings
                .values()
                .filter(|e| e.should_translate)
                .filter(|e| is_translated_for(e, &locale))
                .count();
            let percentage = if total == 0 {
                0.0
            } else {
                (translated as f64 / total as f64) * 100.0
            };
            LocaleInfo {
                locale,
                translated,
                total,
                percentage,
            }
        })
        .collect()
}

/// Add a new locale to all translatable entries.
/// Returns the number of keys initialized.
pub fn add_locale(file: &mut XcStringsFile, locale: &str) -> Result<usize, XcStringsError> {
    if locale.is_empty() {
        return Err(XcStringsError::InvalidFormat("locale is empty".into()));
    }

    // Check if locale already exists in any entry
    for entry in file.strings.values() {
        if let Some(locs) = &entry.localizations
            && locs.contains_key(locale)
        {
            return Err(XcStringsError::LocaleAlreadyExists(locale.into()));
        }
    }

    let mut count = 0;
    for entry in file.strings.values_mut() {
        if !entry.should_translate {
            continue;
        }
        let locs = entry.localizations.get_or_insert_with(Default::default);
        locs.insert(
            locale.to_string(),
            Localization {
                string_unit: Some(StringUnit {
                    state: TranslationState::New,
                    value: String::new(),
                }),
                variations: None,
                substitutions: None,
            },
        );
        count += 1;
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::*;
    use crate::model::xcstrings::StringEntry;

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
    fn list_locales_correct_counts() {
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
        let locales = list_locales(&file);

        let de = locales.iter().find(|l| l.locale == "de").unwrap();
        assert_eq!(de.translated, 1);
        assert_eq!(de.total, 2);
        assert!((de.percentage - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn list_locales_empty_file() {
        let file = make_file(IndexMap::new());
        let locales = list_locales(&file);
        assert!(locales.is_empty());
    }

    #[test]
    fn add_locale_creates_entries() {
        let mut strings = IndexMap::new();
        strings.insert(
            "key1".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        strings.insert(
            "key2".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        let mut file = make_file(strings);

        let count = add_locale(&mut file, "fr").unwrap();
        assert_eq!(count, 2);

        for entry in file.strings.values() {
            let locs = entry.localizations.as_ref().unwrap();
            let fr = locs.get("fr").unwrap();
            let su = fr.string_unit.as_ref().unwrap();
            assert_eq!(su.state, TranslationState::New);
            assert!(su.value.is_empty());
        }
    }

    #[test]
    fn add_locale_excludes_nontranslatable() {
        let mut strings = IndexMap::new();
        strings.insert(
            "key1".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        strings.insert("key2".to_string(), make_nontranslatable_entry());
        let mut file = make_file(strings);

        let count = add_locale(&mut file, "fr").unwrap();
        assert_eq!(count, 1);

        let nontrans = &file.strings["key2"];
        assert!(
            nontrans.localizations.is_none()
                || !nontrans.localizations.as_ref().unwrap().contains_key("fr")
        );
    }

    #[test]
    fn add_locale_duplicate_returns_error() {
        let mut strings = IndexMap::new();
        strings.insert(
            "key1".to_string(),
            make_entry(&[("de", TranslationState::New)]),
        );
        let mut file = make_file(strings);

        let result = add_locale(&mut file, "de");
        assert!(matches!(
            result.unwrap_err(),
            XcStringsError::LocaleAlreadyExists(_)
        ));
    }

    #[test]
    fn add_locale_empty_string_returns_error() {
        let mut file = make_file(IndexMap::new());
        let result = add_locale(&mut file, "");
        assert!(matches!(
            result.unwrap_err(),
            XcStringsError::InvalidFormat(_)
        ));
    }
}
