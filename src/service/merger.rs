use std::collections::BTreeMap;

use crate::model::translation::{CompletedTranslation, RejectedTranslation, SubmitResult};
use crate::model::xcstrings::{
    Localization, OrderedMap, PluralVariation, StringUnit, TranslationState, Variations,
    XcStringsFile,
};

/// Merge a batch of validated translations into the file model.
/// Returns count of accepted + any rejected during merge.
pub fn merge_translations(
    file: &mut XcStringsFile,
    translations: &[CompletedTranslation],
) -> SubmitResult {
    let mut accepted = 0;
    let mut rejected = Vec::new();

    for translation in translations {
        let entry = match file.strings.get_mut(&translation.key) {
            Some(e) => e,
            None => {
                rejected.push(RejectedTranslation {
                    key: translation.key.clone(),
                    reason: "key not found".into(),
                });
                continue;
            }
        };

        let localizations = entry.localizations.get_or_insert_with(OrderedMap::new);

        if let Some(plural_forms) = &translation.plural_forms {
            let localization = localizations
                .entry(translation.locale.clone())
                .or_insert_with(|| Localization {
                    string_unit: None,
                    variations: None,
                    substitutions: None,
                });

            let variations = localization.variations.get_or_insert(Variations {
                plural: None,
                device: None,
            });

            let plural_map = variations.plural.get_or_insert_with(BTreeMap::new);

            for (form, value) in plural_forms {
                plural_map.insert(
                    form.clone(),
                    PluralVariation {
                        string_unit: StringUnit {
                            state: TranslationState::Translated,
                            value: value.clone(),
                        },
                    },
                );
            }
        } else {
            let localization = localizations
                .entry(translation.locale.clone())
                .or_insert_with(|| Localization {
                    string_unit: None,
                    variations: None,
                    substitutions: None,
                });

            localization.string_unit = Some(StringUnit {
                state: TranslationState::Translated,
                value: translation.value.clone(),
            });
        }

        accepted += 1;
    }

    SubmitResult {
        accepted,
        rejected,
        dry_run: false,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use indexmap::IndexMap;

    use super::*;
    use crate::model::xcstrings::{StringEntry, XcStringsFile};

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

    fn empty_entry() -> StringEntry {
        StringEntry {
            extraction_state: None,
            should_translate: true,
            comment: None,
            localizations: None,
        }
    }

    fn entry_with_locale(locale: &str, value: &str) -> StringEntry {
        let mut localizations = IndexMap::new();
        localizations.insert(
            locale.to_string(),
            Localization {
                string_unit: Some(StringUnit {
                    state: TranslationState::Translated,
                    value: value.to_string(),
                }),
                variations: None,
                substitutions: None,
            },
        );
        StringEntry {
            extraction_state: None,
            should_translate: true,
            comment: None,
            localizations: Some(localizations),
        }
    }

    fn simple_translation(key: &str, locale: &str, value: &str) -> CompletedTranslation {
        CompletedTranslation {
            key: key.to_string(),
            locale: locale.to_string(),
            value: value.to_string(),
            plural_forms: None,
        }
    }

    #[test]
    fn test_merge_into_empty_localizations() {
        let mut file = make_file(vec![("greeting", empty_entry())]);
        let result =
            merge_translations(&mut file, &[simple_translation("greeting", "uk", "Привіт")]);

        assert_eq!(result.accepted, 1);
        assert!(result.rejected.is_empty());

        let locs = file.strings["greeting"].localizations.as_ref().unwrap();
        let uk = locs["uk"].string_unit.as_ref().unwrap();
        assert_eq!(uk.value, "Привіт");
        assert_eq!(uk.state, TranslationState::Translated);
    }

    #[test]
    fn test_merge_update_existing() {
        let mut file = make_file(vec![(
            "greeting",
            entry_with_locale("uk", "Старий переклад"),
        )]);
        let result = merge_translations(
            &mut file,
            &[simple_translation("greeting", "uk", "Новий переклад")],
        );

        assert_eq!(result.accepted, 1);
        let uk = file.strings["greeting"].localizations.as_ref().unwrap()["uk"]
            .string_unit
            .as_ref()
            .unwrap();
        assert_eq!(uk.value, "Новий переклад");
        assert_eq!(uk.state, TranslationState::Translated);
    }

    #[test]
    fn test_merge_unknown_key() {
        let mut file = make_file(vec![("greeting", empty_entry())]);
        let result = merge_translations(
            &mut file,
            &[simple_translation("nonexistent", "uk", "Щось")],
        );

        assert_eq!(result.accepted, 0);
        assert_eq!(result.rejected.len(), 1);
        assert!(result.rejected[0].reason.contains("key not found"));
    }

    #[test]
    fn test_merge_plurals() {
        let mut file = make_file(vec![("items", empty_entry())]);
        let mut plural_forms = BTreeMap::new();
        plural_forms.insert("one".to_string(), "%lld елемент".to_string());
        plural_forms.insert("few".to_string(), "%lld елементи".to_string());
        plural_forms.insert("many".to_string(), "%lld елементів".to_string());
        plural_forms.insert("other".to_string(), "%lld елементів".to_string());

        let translations = vec![CompletedTranslation {
            key: "items".to_string(),
            locale: "uk".to_string(),
            value: String::new(),
            plural_forms: Some(plural_forms),
        }];

        let result = merge_translations(&mut file, &translations);
        assert_eq!(result.accepted, 1);

        let locs = file.strings["items"].localizations.as_ref().unwrap();
        let uk = &locs["uk"];
        let plural = uk.variations.as_ref().unwrap().plural.as_ref().unwrap();
        assert_eq!(plural.len(), 4);
        assert_eq!(plural["one"].string_unit.value, "%lld елемент");
        assert_eq!(
            plural["one"].string_unit.state,
            TranslationState::Translated
        );
    }

    #[test]
    fn test_merge_idempotent() {
        let mut file = make_file(vec![("greeting", empty_entry())]);
        let t = simple_translation("greeting", "uk", "Привіт");

        let r1 = merge_translations(&mut file, &[t.clone()]);
        let r2 = merge_translations(&mut file, &[t]);

        assert_eq!(r1.accepted, 1);
        assert_eq!(r2.accepted, 1);

        let locs = file.strings["greeting"].localizations.as_ref().unwrap();
        assert_eq!(locs.len(), 1); // no duplicates
        assert_eq!(locs["uk"].string_unit.as_ref().unwrap().value, "Привіт");
    }

    #[test]
    fn test_merge_multiple_translations() {
        let mut file = make_file(vec![
            ("greeting", empty_entry()),
            ("farewell", empty_entry()),
            ("thanks", empty_entry()),
        ]);

        let translations = vec![
            simple_translation("greeting", "uk", "Привіт"),
            simple_translation("farewell", "uk", "До побачення"),
            simple_translation("thanks", "uk", "Дякую"),
        ];

        let result = merge_translations(&mut file, &translations);
        assert_eq!(result.accepted, 3);
        assert!(result.rejected.is_empty());

        for (key, expected) in [
            ("greeting", "Привіт"),
            ("farewell", "До побачення"),
            ("thanks", "Дякую"),
        ] {
            let value = &file.strings[key].localizations.as_ref().unwrap()["uk"]
                .string_unit
                .as_ref()
                .unwrap()
                .value;
            assert_eq!(value, expected);
        }
    }
}
