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
    let mut accepted_keys = Vec::new();
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

        // Pre-extract source substitution template before mutable borrow
        let source_sub_template = translation.substitution_name.as_ref().and_then(|sub_name| {
            entry
                .localizations
                .as_ref()
                .and_then(|locs| locs.get(&file.source_language))
                .and_then(|loc| loc.substitutions.as_ref())
                .and_then(|src_subs| src_subs.get(sub_name))
                .cloned()
        });

        let localizations = entry.localizations.get_or_insert_with(OrderedMap::new);

        if let Some(plural_forms) = &translation.plural_forms {
            if let Some(sub_name) = &translation.substitution_name {
                // Write plural forms into the substitution's variations
                let localization = localizations
                    .entry(translation.locale.clone())
                    .or_insert_with(|| Localization {
                        string_unit: None,
                        variations: None,
                        substitutions: None,
                    });

                let subs = localization.substitutions.get_or_insert_with(BTreeMap::new);

                let template = source_sub_template;
                let sub_value = subs.entry(sub_name.clone()).or_insert_with(|| {
                    // Start from source template (preserves argNum, formatSpecifier)
                    template.unwrap_or_else(|| serde_json::json!({}))
                });

                // Build plural variations JSON
                let mut plural_obj = serde_json::Map::new();
                for (form, value) in plural_forms {
                    plural_obj.insert(
                        form.clone(),
                        serde_json::json!({
                            "stringUnit": {
                                "state": "translated",
                                "value": value
                            }
                        }),
                    );
                }

                // Set variations.plural on the substitution
                if let Some(sub_obj) = sub_value.as_object_mut() {
                    let variations = sub_obj
                        .entry("variations")
                        .or_insert_with(|| serde_json::json!({}));

                    if let Some(vars_obj) = variations.as_object_mut() {
                        vars_obj
                            .insert("plural".to_string(), serde_json::Value::Object(plural_obj));
                    }
                }
            } else {
                // Write plural forms to localization.variations.plural
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

        accepted_keys.push(translation.key.clone());
        accepted += 1;
    }

    SubmitResult {
        accepted,
        rejected,
        dry_run: false,
        accepted_keys,
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
            substitution_name: None,
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
            substitution_name: None,
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

    #[test]
    fn test_merge_substitution_preserves_arg_num() {
        // Create an entry with source locale substitutions (realistic scenario)
        let mut localizations = IndexMap::new();
        localizations.insert(
            "en".to_string(),
            Localization {
                string_unit: Some(StringUnit {
                    state: TranslationState::Translated,
                    value: "I saw %#@BIRDS@ in the park".to_string(),
                }),
                variations: None,
                substitutions: Some({
                    let mut subs = BTreeMap::new();
                    subs.insert(
                        "BIRDS".to_string(),
                        serde_json::json!({
                            "argNum": 1,
                            "formatSpecifier": "lld",
                            "variations": {
                                "plural": {
                                    "one": { "stringUnit": { "state": "translated", "value": "%arg bird" } },
                                    "other": { "stringUnit": { "state": "translated", "value": "%arg birds" } }
                                }
                            }
                        }),
                    );
                    subs
                }),
            },
        );
        let entry = StringEntry {
            extraction_state: None,
            should_translate: true,
            comment: None,
            localizations: Some(localizations),
        };

        let mut file = make_file(vec![("bird_sighting", entry)]);

        let mut plural_forms = BTreeMap::new();
        plural_forms.insert("one".to_string(), "%arg Vogel".to_string());
        plural_forms.insert("other".to_string(), "%arg Vögel".to_string());

        let translations = vec![CompletedTranslation {
            key: "bird_sighting".to_string(),
            locale: "de".to_string(),
            value: String::new(),
            plural_forms: Some(plural_forms),
            substitution_name: Some("BIRDS".to_string()),
        }];

        let result = merge_translations(&mut file, &translations);
        assert_eq!(result.accepted, 1);

        let locs = file.strings["bird_sighting"]
            .localizations
            .as_ref()
            .unwrap();
        let de = &locs["de"];
        let subs = de.substitutions.as_ref().unwrap();
        let birds = &subs["BIRDS"];

        // Verify argNum and formatSpecifier are preserved from source
        assert_eq!(birds["argNum"], 1, "argNum should be preserved from source");
        assert_eq!(
            birds["formatSpecifier"], "lld",
            "formatSpecifier should be preserved from source"
        );

        // Verify translated plural forms are correct
        assert_eq!(
            birds["variations"]["plural"]["one"]["stringUnit"]["value"],
            "%arg Vogel"
        );
        assert_eq!(
            birds["variations"]["plural"]["other"]["stringUnit"]["value"],
            "%arg Vögel"
        );
    }

    #[test]
    fn test_merge_substitution_plurals() {
        let mut file = make_file(vec![("bird_sighting", empty_entry())]);
        let mut plural_forms = BTreeMap::new();
        plural_forms.insert("one".to_string(), "%arg Vogel".to_string());
        plural_forms.insert("other".to_string(), "%arg Vögel".to_string());

        let translations = vec![CompletedTranslation {
            key: "bird_sighting".to_string(),
            locale: "de".to_string(),
            value: String::new(),
            plural_forms: Some(plural_forms),
            substitution_name: Some("BIRDS".to_string()),
        }];

        let result = merge_translations(&mut file, &translations);
        assert_eq!(result.accepted, 1);

        let locs = file.strings["bird_sighting"]
            .localizations
            .as_ref()
            .unwrap();
        let de = &locs["de"];
        let subs = de.substitutions.as_ref().unwrap();
        let birds_sub = &subs["BIRDS"];
        let birds_one = birds_sub["variations"]["plural"]["one"]["stringUnit"]["value"]
            .as_str()
            .unwrap();
        assert_eq!(birds_one, "%arg Vogel");
    }
}
