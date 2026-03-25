use std::collections::BTreeMap;

use serde::Serialize;

use crate::error::XcStringsError;
use crate::model::xcstrings::{
    ExtractionState, Localization, OrderedMap, PluralVariation, StringEntry, StringUnit,
    TranslationState, Variations, XcStringsFile,
};
use crate::service::strings_parser::StringsEntry;
use crate::service::stringsdict_parser::StringsdictEntry;

/// A parsed file ready for conversion, grouped by locale.
pub struct ParsedLocaleData {
    pub strings: Vec<StringsEntry>,
    pub stringsdict: Vec<StringsdictEntry>,
}

#[derive(Debug, Serialize)]
pub struct MigrateResult {
    pub file: XcStringsFile,
    pub total_keys: usize,
    pub locales_imported: Vec<LocaleImportStats>,
    pub plural_keys: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct LocaleImportStats {
    pub locale: String,
    pub keys_count: usize,
}

/// Check if a stringsdict format key is a simple single-variable plural
/// (exactly `%#@VARNAME@` with no surrounding text).
pub(crate) fn is_simple_plural(format_key: &str) -> bool {
    let trimmed = format_key.trim();
    if !trimmed.starts_with("%#@") || !trimmed.ends_with('@') {
        return false;
    }
    let inner = &trimmed[3..trimmed.len() - 1];
    !inner.is_empty() && !inner.contains('@') && !inner.contains('%')
}

/// Replace format specifiers like %lld, %d, %@, %f with %arg in plural values.
/// Handles both non-positional (`%lld`) and positional (`%1$lld`) forms.
pub(crate) fn replace_specifier_with_arg(value: &str, format_specifier: &str) -> String {
    let mut result = value.to_string();
    // Replace positional form first: %1$lld, %2$lld, etc.
    for n in 1..=9 {
        let positional = format!("%{n}${format_specifier}");
        result = result.replace(&positional, "%arg");
    }
    // Then replace non-positional form: %lld
    let plain = format!("%{format_specifier}");
    result.replace(&plain, "%arg")
}

/// Build substitutions map for complex plurals.
fn build_substitutions(entry: &StringsdictEntry) -> BTreeMap<String, serde_json::Value> {
    let mut subs = BTreeMap::new();
    for (idx, (var_name, var)) in entry.variables.iter().enumerate() {
        let mut plural_forms = serde_json::Map::new();
        for (form, value) in &var.forms {
            let replaced = replace_specifier_with_arg(value, &var.format_specifier);
            plural_forms.insert(
                form.clone(),
                serde_json::json!({
                    "stringUnit": {
                        "state": "translated",
                        "value": replaced
                    }
                }),
            );
        }
        subs.insert(
            var_name.clone(),
            serde_json::json!({
                "argNum": idx + 1,
                "formatSpecifier": var.format_specifier,
                "variations": {
                    "plural": plural_forms
                }
            }),
        );
    }
    subs
}

/// Build a Localization from a stringsdict entry for the source locale.
fn build_stringsdict_localization(entry: &StringsdictEntry) -> Localization {
    if is_simple_plural(&entry.format_key)
        && entry.variables.len() == 1
        && let Some(var) = entry.variables.values().next()
    {
        let mut plural = BTreeMap::new();
        for (form, value) in &var.forms {
            plural.insert(
                form.clone(),
                PluralVariation {
                    string_unit: StringUnit {
                        state: TranslationState::Translated,
                        value: value.clone(),
                    },
                },
            );
        }
        Localization {
            string_unit: None,
            variations: Some(Variations {
                plural: Some(plural),
                device: None,
            }),
            substitutions: None,
        }
    } else {
        // Complex plural: stringUnit + substitutions
        Localization {
            string_unit: Some(StringUnit {
                state: TranslationState::Translated,
                value: entry.format_key.clone(),
            }),
            variations: None,
            substitutions: Some(build_substitutions(entry)),
        }
    }
}

/// Build XcStringsFile from pre-parsed legacy locale data.
///
/// `locale_data` is keyed by locale code (e.g., "en", "es").
/// `existing` enables merge mode — new keys are added, existing keys are skipped.
pub fn build_xcstrings_from_legacy(
    source_language: &str,
    locale_data: &OrderedMap<String, ParsedLocaleData>,
    existing: Option<XcStringsFile>,
) -> Result<MigrateResult, XcStringsError> {
    let mut warnings: Vec<String> = Vec::new();

    // Step 4: Validate source_language exists in locale_data
    if !locale_data.contains_key(source_language) {
        return Err(XcStringsError::InvalidFormat(format!(
            "source language '{}' not found in imported files (available: {})",
            source_language,
            locale_data.keys().cloned().collect::<Vec<_>>().join(", ")
        )));
    }

    // Step 5: Build XcStringsFile from source locale first
    let mut strings: OrderedMap<String, StringEntry> = OrderedMap::new();
    let source_data = &locale_data[source_language];

    // Process source .strings entries
    for entry in &source_data.strings {
        let mut localizations = OrderedMap::new();
        let state = if entry.value.is_empty() {
            TranslationState::New
        } else {
            TranslationState::Translated
        };
        localizations.insert(
            source_language.to_owned(),
            Localization {
                string_unit: Some(StringUnit {
                    state,
                    value: entry.value.clone(),
                }),
                variations: None,
                substitutions: None,
            },
        );

        if let Some(existing) = strings.get(&entry.key)
            && existing.localizations.is_some()
        {
            warnings.push(format!("duplicate key '{}': last value wins", entry.key));
        }

        strings.insert(
            entry.key.clone(),
            StringEntry {
                extraction_state: Some(ExtractionState::Migrated),
                should_translate: true,
                comment: entry.comment.clone(),
                localizations: Some(localizations),
            },
        );
    }

    // Process source .stringsdict entries (override .strings for same key)
    let mut plural_keys: usize = 0;
    for entry in &source_data.stringsdict {
        let mut localizations = OrderedMap::new();
        localizations.insert(
            source_language.to_owned(),
            build_stringsdict_localization(entry),
        );

        if strings.contains_key(&entry.key) {
            warnings.push(format!(
                "key '{}': .stringsdict overrides .strings",
                entry.key
            ));
        }

        strings.insert(
            entry.key.clone(),
            StringEntry {
                extraction_state: Some(ExtractionState::Migrated),
                should_translate: true,
                comment: None,
                localizations: Some(localizations),
            },
        );
        plural_keys += 1;
    }

    // Step 6: Add non-source locale translations
    let mut locales_imported = Vec::new();
    for (locale, data) in locale_data {
        if locale == source_language {
            locales_imported.push(LocaleImportStats {
                locale: locale.clone(),
                keys_count: data.strings.len() + data.stringsdict.len(),
            });
            continue;
        }

        let mut keys_count = 0;

        // .strings translations
        for entry in &data.strings {
            if !strings.contains_key(&entry.key) {
                // Key in non-source but missing from source → add with warning
                warnings.push(format!(
                    "key '{}' found in locale '{}' but not in source — adding",
                    entry.key, locale
                ));
                let mut localizations = OrderedMap::new();
                localizations.insert(
                    source_language.to_owned(),
                    Localization {
                        string_unit: Some(StringUnit {
                            state: TranslationState::New,
                            value: String::new(),
                        }),
                        variations: None,
                        substitutions: None,
                    },
                );
                strings.insert(
                    entry.key.clone(),
                    StringEntry {
                        extraction_state: Some(ExtractionState::Migrated),
                        should_translate: true,
                        comment: None,
                        localizations: Some(localizations),
                    },
                );
            }

            let string_entry = strings.get_mut(&entry.key).ok_or_else(|| {
                XcStringsError::InvalidFormat("internal: missing key after insert".into())
            })?;
            let localizations = string_entry
                .localizations
                .get_or_insert_with(OrderedMap::new);

            let state = if entry.value.is_empty() {
                TranslationState::New
            } else {
                TranslationState::Translated
            };
            localizations.insert(
                locale.clone(),
                Localization {
                    string_unit: Some(StringUnit {
                        state,
                        value: entry.value.clone(),
                    }),
                    variations: None,
                    substitutions: None,
                },
            );
            keys_count += 1;
        }

        // .stringsdict translations
        for entry in &data.stringsdict {
            if !strings.contains_key(&entry.key) {
                warnings.push(format!(
                    "key '{}' found in locale '{}' but not in source — adding",
                    entry.key, locale
                ));
                let mut localizations = OrderedMap::new();
                localizations.insert(
                    source_language.to_owned(),
                    Localization {
                        string_unit: Some(StringUnit {
                            state: TranslationState::New,
                            value: String::new(),
                        }),
                        variations: None,
                        substitutions: None,
                    },
                );
                strings.insert(
                    entry.key.clone(),
                    StringEntry {
                        extraction_state: Some(ExtractionState::Migrated),
                        should_translate: true,
                        comment: None,
                        localizations: Some(localizations),
                    },
                );
            }

            let string_entry = strings.get_mut(&entry.key).ok_or_else(|| {
                XcStringsError::InvalidFormat("internal: missing key after insert".into())
            })?;
            let localizations = string_entry
                .localizations
                .get_or_insert_with(OrderedMap::new);
            localizations.insert(locale.clone(), build_stringsdict_localization(entry));
            keys_count += 1;
        }

        locales_imported.push(LocaleImportStats {
            locale: locale.clone(),
            keys_count,
        });
    }

    let new_file = XcStringsFile {
        source_language: source_language.to_owned(),
        strings,
        version: "1.0".to_owned(),
    };

    // Step 7: Merge mode: if existing is Some, add only new keys
    let xcstrings_file = if let Some(mut existing_file) = existing {
        let mut skipped_count = 0;

        for (key, entry) in &new_file.strings {
            if existing_file.strings.contains_key(key) {
                skipped_count += 1;
            } else {
                existing_file.strings.insert(key.clone(), entry.clone());
            }
        }

        if skipped_count > 0 {
            warnings.push(format!(
                "{skipped_count} keys already exist in output, skipped"
            ));
        }

        existing_file
    } else {
        new_file
    };

    let total_keys = xcstrings_file.strings.len();

    Ok(MigrateResult {
        file: xcstrings_file,
        total_keys,
        locales_imported,
        plural_keys,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::stringsdict_parser::PluralVariable;
    use indexmap::IndexMap;

    #[test]
    fn test_is_simple_plural_basic() {
        assert!(is_simple_plural("%#@items@"));
    }

    #[test]
    fn test_is_simple_plural_complex() {
        assert!(!is_simple_plural("%1$#@photos@ in %2$#@albums@"));
    }

    #[test]
    fn test_is_simple_plural_edge_cases() {
        assert!(!is_simple_plural(""));
        assert!(!is_simple_plural("%#@@"));
        assert!(!is_simple_plural("%#@a@b"));
        assert!(!is_simple_plural("%#@items"));
        assert!(!is_simple_plural("items@"));
    }

    #[test]
    fn test_replace_specifier_basic() {
        assert_eq!(
            replace_specifier_with_arg("%lld items", "lld"),
            "%arg items"
        );
    }

    #[test]
    fn test_replace_specifier_at_sign() {
        assert_eq!(replace_specifier_with_arg("%@ things", "@"), "%arg things");
    }

    #[test]
    fn test_replace_specifier_positional() {
        assert_eq!(
            replace_specifier_with_arg("%1$lld photos in %2$lld albums", "lld"),
            "%arg photos in %arg albums"
        );
    }

    #[test]
    fn test_build_substitutions_single_var() {
        let mut forms = BTreeMap::new();
        forms.insert("one".to_string(), "%lld item".to_string());
        forms.insert("other".to_string(), "%lld items".to_string());

        let mut variables = IndexMap::new();
        variables.insert(
            "items".to_string(),
            PluralVariable {
                format_specifier: "lld".to_string(),
                forms,
            },
        );

        let entry = StringsdictEntry {
            key: "items_count".to_string(),
            format_key: "%#@items@".to_string(),
            variables,
        };

        let subs = build_substitutions(&entry);
        assert_eq!(subs.len(), 1);

        let items_sub = &subs["items"];
        assert_eq!(items_sub["argNum"], 1);
        assert_eq!(items_sub["formatSpecifier"], "lld");
        assert!(items_sub["variations"]["plural"]["one"].is_object());
        assert_eq!(
            items_sub["variations"]["plural"]["one"]["stringUnit"]["value"],
            "%arg item"
        );
        assert_eq!(
            items_sub["variations"]["plural"]["other"]["stringUnit"]["value"],
            "%arg items"
        );
    }

    #[test]
    fn replace_specifier_with_arg_plain() {
        assert_eq!(
            replace_specifier_with_arg("%lld items", "lld"),
            "%arg items"
        );
    }

    #[test]
    fn replace_specifier_with_arg_positional() {
        assert_eq!(
            replace_specifier_with_arg("%1$lld photo in %2$lld albums", "lld"),
            "%arg photo in %arg albums"
        );
    }

    #[test]
    fn replace_specifier_with_arg_mixed() {
        assert_eq!(
            replace_specifier_with_arg("%1$d and %d items", "d"),
            "%arg and %arg items"
        );
    }

    #[test]
    fn source_language_not_in_locale_data_errors() {
        let locale_data: OrderedMap<String, ParsedLocaleData> = OrderedMap::new();
        let result = build_xcstrings_from_legacy("en", &locale_data, None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not found"),
            "should mention language not found: {err}"
        );
    }

    fn make_strings_entry(key: &str, value: &str) -> StringsEntry {
        StringsEntry {
            key: key.to_string(),
            value: value.to_string(),
            comment: None,
        }
    }

    fn make_simple_stringsdict_entry(key: &str) -> StringsdictEntry {
        let mut forms = BTreeMap::new();
        forms.insert("one".to_string(), "%lld item".to_string());
        forms.insert("other".to_string(), "%lld items".to_string());

        let mut variables = IndexMap::new();
        variables.insert(
            "count".to_string(),
            PluralVariable {
                format_specifier: "lld".to_string(),
                forms,
            },
        );

        StringsdictEntry {
            key: key.to_string(),
            format_key: "%#@count@".to_string(),
            variables,
        }
    }

    #[test]
    fn build_with_empty_strings_value_gets_new_state() {
        let mut locale_data: OrderedMap<String, ParsedLocaleData> = OrderedMap::new();
        locale_data.insert(
            "en".to_string(),
            ParsedLocaleData {
                strings: vec![make_strings_entry("empty_key", "")],
                stringsdict: vec![],
            },
        );

        let result = build_xcstrings_from_legacy("en", &locale_data, None).unwrap();
        let entry = &result.file.strings["empty_key"];
        let locs = entry.localizations.as_ref().unwrap();
        let en_loc = &locs["en"];
        assert_eq!(
            en_loc.string_unit.as_ref().unwrap().state,
            TranslationState::New
        );
    }

    #[test]
    fn build_warns_on_duplicate_key() {
        let mut locale_data: OrderedMap<String, ParsedLocaleData> = OrderedMap::new();
        locale_data.insert(
            "en".to_string(),
            ParsedLocaleData {
                strings: vec![
                    make_strings_entry("dup", "first"),
                    make_strings_entry("dup", "second"),
                ],
                stringsdict: vec![],
            },
        );

        let result = build_xcstrings_from_legacy("en", &locale_data, None).unwrap();
        assert!(
            result.warnings.iter().any(|w| w.contains("duplicate")),
            "should warn about duplicate key"
        );
    }

    #[test]
    fn stringsdict_overrides_strings_with_warning() {
        let mut locale_data: OrderedMap<String, ParsedLocaleData> = OrderedMap::new();
        locale_data.insert(
            "en".to_string(),
            ParsedLocaleData {
                strings: vec![make_strings_entry("items_count", "plain value")],
                stringsdict: vec![make_simple_stringsdict_entry("items_count")],
            },
        );

        let result = build_xcstrings_from_legacy("en", &locale_data, None).unwrap();
        assert!(
            result.warnings.iter().any(|w| w.contains("overrides")),
            "should warn about stringsdict override"
        );
        assert_eq!(result.plural_keys, 1);
    }

    #[test]
    fn non_source_locale_adds_translations() {
        let mut locale_data: OrderedMap<String, ParsedLocaleData> = OrderedMap::new();
        locale_data.insert(
            "en".to_string(),
            ParsedLocaleData {
                strings: vec![make_strings_entry("greeting", "Hello")],
                stringsdict: vec![],
            },
        );
        locale_data.insert(
            "es".to_string(),
            ParsedLocaleData {
                strings: vec![make_strings_entry("greeting", "Hola")],
                stringsdict: vec![],
            },
        );

        let result = build_xcstrings_from_legacy("en", &locale_data, None).unwrap();
        let locs = result.file.strings["greeting"]
            .localizations
            .as_ref()
            .unwrap();
        assert!(locs.contains_key("es"));
        assert_eq!(
            locs["es"].string_unit.as_ref().unwrap().state,
            TranslationState::Translated
        );
    }

    #[test]
    fn non_source_key_not_in_source_warns_and_adds() {
        let mut locale_data: OrderedMap<String, ParsedLocaleData> = OrderedMap::new();
        locale_data.insert(
            "en".to_string(),
            ParsedLocaleData {
                strings: vec![make_strings_entry("greeting", "Hello")],
                stringsdict: vec![],
            },
        );
        locale_data.insert(
            "es".to_string(),
            ParsedLocaleData {
                strings: vec![
                    make_strings_entry("greeting", "Hola"),
                    make_strings_entry("extra_key", "Extra"),
                ],
                stringsdict: vec![],
            },
        );

        let result = build_xcstrings_from_legacy("en", &locale_data, None).unwrap();
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("extra_key") && w.contains("not in source")),
            "should warn about key not in source"
        );
        assert!(result.file.strings.contains_key("extra_key"));
    }

    #[test]
    fn non_source_stringsdict_key_not_in_source() {
        let mut locale_data: OrderedMap<String, ParsedLocaleData> = OrderedMap::new();
        locale_data.insert(
            "en".to_string(),
            ParsedLocaleData {
                strings: vec![make_strings_entry("greeting", "Hello")],
                stringsdict: vec![],
            },
        );
        locale_data.insert(
            "es".to_string(),
            ParsedLocaleData {
                strings: vec![],
                stringsdict: vec![make_simple_stringsdict_entry("plural_only_es")],
            },
        );

        let result = build_xcstrings_from_legacy("en", &locale_data, None).unwrap();
        assert!(
            result.warnings.iter().any(|w| w.contains("plural_only_es")),
            "should warn about stringsdict key not in source"
        );
        assert!(result.file.strings.contains_key("plural_only_es"));
    }

    #[test]
    fn non_source_empty_value_gets_new_state() {
        let mut locale_data: OrderedMap<String, ParsedLocaleData> = OrderedMap::new();
        locale_data.insert(
            "en".to_string(),
            ParsedLocaleData {
                strings: vec![make_strings_entry("key1", "Value")],
                stringsdict: vec![],
            },
        );
        locale_data.insert(
            "fr".to_string(),
            ParsedLocaleData {
                strings: vec![make_strings_entry("key1", "")],
                stringsdict: vec![],
            },
        );

        let result = build_xcstrings_from_legacy("en", &locale_data, None).unwrap();
        let locs = result.file.strings["key1"].localizations.as_ref().unwrap();
        assert_eq!(
            locs["fr"].string_unit.as_ref().unwrap().state,
            TranslationState::New
        );
    }

    #[test]
    fn merge_mode_skips_existing_keys() {
        // First build a base file
        let mut locale_data: OrderedMap<String, ParsedLocaleData> = OrderedMap::new();
        locale_data.insert(
            "en".to_string(),
            ParsedLocaleData {
                strings: vec![
                    make_strings_entry("existing", "Old"),
                    make_strings_entry("new_key", "New"),
                ],
                stringsdict: vec![],
            },
        );

        // Create an existing file with only "existing" key
        let mut existing_strings: OrderedMap<String, crate::model::xcstrings::StringEntry> =
            OrderedMap::new();
        let mut existing_locs = OrderedMap::new();
        existing_locs.insert(
            "en".to_string(),
            Localization {
                string_unit: Some(StringUnit {
                    state: TranslationState::Translated,
                    value: "Original".to_string(),
                }),
                variations: None,
                substitutions: None,
            },
        );
        existing_strings.insert(
            "existing".to_string(),
            crate::model::xcstrings::StringEntry {
                extraction_state: None,
                should_translate: true,
                comment: None,
                localizations: Some(existing_locs),
            },
        );
        let existing_file = XcStringsFile {
            source_language: "en".to_string(),
            strings: existing_strings,
            version: "1.0".to_string(),
        };

        let result = build_xcstrings_from_legacy("en", &locale_data, Some(existing_file)).unwrap();

        // "existing" key should keep original value
        let locs = result.file.strings["existing"]
            .localizations
            .as_ref()
            .unwrap();
        assert_eq!(locs["en"].string_unit.as_ref().unwrap().value, "Original");
        // "new_key" should be added
        assert!(result.file.strings.contains_key("new_key"));
        // Should warn about skipped keys
        assert!(
            result.warnings.iter().any(|w| w.contains("skipped")),
            "should warn about skipped existing keys"
        );
    }

    #[test]
    fn complex_plural_uses_substitutions() {
        let mut forms = BTreeMap::new();
        forms.insert("one".to_string(), "%lld photo".to_string());
        forms.insert("other".to_string(), "%lld photos".to_string());

        let mut album_forms = BTreeMap::new();
        album_forms.insert("one".to_string(), "%lld album".to_string());
        album_forms.insert("other".to_string(), "%lld albums".to_string());

        let mut variables = IndexMap::new();
        variables.insert(
            "photos".to_string(),
            PluralVariable {
                format_specifier: "lld".to_string(),
                forms,
            },
        );
        variables.insert(
            "albums".to_string(),
            PluralVariable {
                format_specifier: "lld".to_string(),
                forms: album_forms,
            },
        );

        let entry = StringsdictEntry {
            key: "photos_in_albums".to_string(),
            format_key: "%1$#@photos@ in %2$#@albums@".to_string(),
            variables,
        };

        let loc = build_stringsdict_localization(&entry);
        // Complex plural should use substitutions, not variations
        assert!(loc.substitutions.is_some());
        assert!(loc.variations.is_none());
        assert!(loc.string_unit.is_some());
    }

    #[test]
    fn locale_import_stats_counted_correctly() {
        let mut locale_data: OrderedMap<String, ParsedLocaleData> = OrderedMap::new();
        locale_data.insert(
            "en".to_string(),
            ParsedLocaleData {
                strings: vec![
                    make_strings_entry("k1", "V1"),
                    make_strings_entry("k2", "V2"),
                ],
                stringsdict: vec![],
            },
        );
        locale_data.insert(
            "de".to_string(),
            ParsedLocaleData {
                strings: vec![make_strings_entry("k1", "W1")],
                stringsdict: vec![],
            },
        );

        let result = build_xcstrings_from_legacy("en", &locale_data, None).unwrap();
        assert_eq!(result.locales_imported.len(), 2);

        let en_stats = result
            .locales_imported
            .iter()
            .find(|s| s.locale == "en")
            .unwrap();
        assert_eq!(en_stats.keys_count, 2);

        let de_stats = result
            .locales_imported
            .iter()
            .find(|s| s.locale == "de")
            .unwrap();
        assert_eq!(de_stats.keys_count, 1);
    }
}
