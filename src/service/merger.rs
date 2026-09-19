use super::submission;
use crate::model::translation::{CompletedTranslation, SubmitResult};
use crate::model::xcstrings::XcStringsFile;

/// Merge prevalidated requests with destination-level overlap checks.
pub fn merge_translations(
    file: &mut XcStringsFile,
    translations: &[CompletedTranslation],
) -> SubmitResult {
    let (plans, rejected) = submission::prepare(file, translations);
    let mut result = SubmitResult {
        rejected: rejected.into_iter().flatten().collect(),
        ..Default::default()
    };
    for plan in plans {
        match submission::mutation::apply(file, &plan.leaves) {
            Ok(()) => {
                result.accepted += 1;
                result
                    .accepted_keys
                    .push(translations[plan.index].key.clone());
                result
                    .accepted_destinations
                    .extend(plan.leaves.into_iter().map(|(destination, _)| destination));
            }
            Err(reason) => result.rejected.push(submission::reject(
                &translations[plan.index],
                "invalid_path",
                reason,
            )),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use crate::model::xcstrings::{Localization, OrderedMap, TranslationState};
    use std::collections::BTreeMap;

    use indexmap::IndexMap;

    use super::*;
    use crate::model::xcstrings::StringUnit;
    use crate::model::xcstrings::{StringEntry, XcStringsFile};

    fn make_file(entries: Vec<(&str, StringEntry)>) -> XcStringsFile {
        XcStringsFile {
            source_language: "en".to_string(),
            strings: entries
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            version: "1.0".to_string(),
            ..Default::default()
        }
    }

    fn empty_entry() -> StringEntry {
        StringEntry {
            extraction_state: None,
            should_translate: true,
            comment: None,
            localizations: None,
            ..Default::default()
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
                    ..Default::default()
                }),
                variations: None,
                substitutions: None,
                ..Default::default()
            },
        );
        StringEntry {
            extraction_state: None,
            should_translate: true,
            comment: None,
            localizations: Some(localizations),
            ..Default::default()
        }
    }

    fn simple_translation(key: &str, locale: &str, value: &str) -> CompletedTranslation {
        CompletedTranslation {
            key: key.to_string(),
            locale: locale.to_string(),
            value: value.to_string(),
            plural_forms: None,
            substitution_name: None,
            ..Default::default()
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
            ..Default::default()
        }];

        let result = merge_translations(&mut file, &translations);
        assert_eq!(result.accepted, 1);

        let locs = file.strings["items"].localizations.as_ref().unwrap();
        let uk = &locs["uk"];
        let plural = uk.variations.as_ref().unwrap().plural.as_ref().unwrap();
        assert_eq!(plural.len(), 4);
        assert_eq!(
            plural["one"].string_unit.as_ref().unwrap().value,
            "%lld елемент"
        );
        assert_eq!(
            plural["one"].string_unit.as_ref().unwrap().state,
            TranslationState::Translated
        );
    }

    #[test]
    fn test_merge_idempotent() {
        let mut file = make_file(vec![("greeting", empty_entry())]);
        let t = simple_translation("greeting", "uk", "Привіт");

        let r1 = merge_translations(&mut file, std::slice::from_ref(&t));
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
                 ..Default::default() }),
                variations: None,
                substitutions: Some({
                    let mut subs = OrderedMap::new();
                    subs.insert(
                        "BIRDS".to_string(),
                        serde_json::from_value(serde_json::json!({
                            "argNum": 1,
                            "formatSpecifier": "lld",
                            "variations": {
                                "plural": {
                                    "one": { "stringUnit": { "state": "translated", "value": "%arg bird" } },
                                    "other": { "stringUnit": { "state": "translated", "value": "%arg birds" } }
                                }
                            }
                        })).unwrap(),
                    );
                    subs
                }),
             ..Default::default() },
        );
        let entry = StringEntry {
            extraction_state: None,
            should_translate: true,
            comment: None,
            localizations: Some(localizations),
            ..Default::default()
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
            ..Default::default()
        }];

        let result = merge_translations(&mut file, &translations);
        assert_eq!(result.accepted, 1);

        let locs = file.strings["bird_sighting"]
            .localizations
            .as_ref()
            .unwrap();
        let de = &locs["de"];
        let subs = de.substitutions.as_ref().unwrap();
        let birds = serde_json::to_value(&subs["BIRDS"]).unwrap();

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
    fn substitution_creation_without_metadata_preserves_catalog() {
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
            ..Default::default()
        }];

        let before = serde_json::to_string(&file).unwrap();
        let result = merge_translations(&mut file, &translations);
        assert_eq!(result.accepted, 0);
        assert_eq!(
            result.rejected[0].reason,
            "missing substitution metadata for 'BIRDS'"
        );
        assert_eq!(serde_json::to_string(&file).unwrap(), before);
    }
}
