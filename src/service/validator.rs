use crate::model::plural::required_plural_forms;
use crate::model::specifier::{FormatSpecifier, extract_specifiers};

use crate::model::translation::{CompletedTranslation, RejectedTranslation};
use crate::model::xcstrings::XcStringsFile;

/// Validate a batch of translations against the source file.
/// Returns a list of rejected translations with reasons.
pub fn validate_translations(
    file: &XcStringsFile,
    translations: &[CompletedTranslation],
) -> Vec<RejectedTranslation> {
    let mut rejected = Vec::new();

    for translation in translations {
        // Check 1: Key exists in file
        let entry = match file.strings.get(&translation.key) {
            Some(e) => e,
            None => {
                rejected.push(RejectedTranslation {
                    key: translation.key.clone(),
                    reason: "key not found in file".into(),
                });
                continue;
            }
        };

        // Check 2: shouldTranslate is true
        if !entry.should_translate {
            rejected.push(RejectedTranslation {
                key: translation.key.clone(),
                reason: "key is marked as shouldTranslate=false".into(),
            });
            continue;
        }

        // Check 3: Non-empty value (for simple translations)
        if translation.value.is_empty() && translation.plural_forms.is_none() {
            rejected.push(RejectedTranslation {
                key: translation.key.clone(),
                reason: "translation value is empty".into(),
            });
            continue;
        }

        // Check 4: Format specifier validation
        // Get source localization for specifier extraction
        let source_loc = entry
            .localizations
            .as_ref()
            .and_then(|locs| locs.get(&file.source_language));

        // Get source text — fall back to the key itself if no source localization exists
        let source_text = source_loc
            .and_then(|loc| loc.string_unit.as_ref())
            .map(|su| su.value.as_str())
            .unwrap_or(&translation.key);

        let source_specs = extract_specifiers(source_text);

        if let Some(plural_forms) = &translation.plural_forms {
            // For plural keys where source has no string_unit but has plural variations,
            // extract specifiers from the first source plural form instead.
            let effective_source_specs = if source_specs.is_empty() {
                source_loc
                    .and_then(|loc| loc.variations.as_ref())
                    .and_then(|v| v.plural.as_ref())
                    .and_then(|p| p.values().next())
                    .map(|var| extract_specifiers(&var.string_unit.value))
                    .unwrap_or_default()
            } else {
                source_specs.clone()
            };
            // Validate required plural forms are present
            let required = required_plural_forms(&translation.locale);
            for req in &required {
                let form_name = req.as_str().to_string();
                if !plural_forms.contains_key(&form_name) {
                    rejected.push(RejectedTranslation {
                        key: translation.key.clone(),
                        reason: format!("missing required plural form: {form_name}"),
                    });
                }
            }

            // Validate specifiers in each plural form value
            for (form, value) in plural_forms {
                let target_specs = extract_specifiers(value);
                if let Some(reason) = check_specifier_mismatch(
                    &effective_source_specs,
                    &target_specs,
                    &translation.key,
                    Some(form),
                ) {
                    rejected.push(reason);
                }
            }
        } else {
            // Simple translation — validate specifiers
            let target_specs = extract_specifiers(&translation.value);
            if let Some(reason) =
                check_specifier_mismatch(&source_specs, &target_specs, &translation.key, None)
            {
                rejected.push(reason);
            }
        }
    }

    rejected
}

fn check_specifier_mismatch(
    source_specs: &[FormatSpecifier],
    target_specs: &[FormatSpecifier],
    key: &str,
    plural_form: Option<&str>,
) -> Option<RejectedTranslation> {
    if source_specs.len() != target_specs.len() {
        let context = plural_form
            .map(|f| format!(" (plural form: {f})"))
            .unwrap_or_default();
        return Some(RejectedTranslation {
            key: key.to_string(),
            reason: format!(
                "format specifier count mismatch{context}: source has {}, translation has {}",
                source_specs.len(),
                target_specs.len()
            ),
        });
    }

    for (src, tgt) in source_specs.iter().zip(target_specs.iter()) {
        if !src.is_compatible_with(tgt) {
            let context = plural_form
                .map(|f| format!(" (plural form: {f})"))
                .unwrap_or_default();
            return Some(RejectedTranslation {
                key: key.to_string(),
                reason: format!(
                    "format specifier type mismatch{context}: source has {}, translation has {}",
                    src.raw, tgt.raw
                ),
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use indexmap::IndexMap;

    use super::*;
    use crate::model::xcstrings::{
        Localization, StringEntry, StringUnit, TranslationState, XcStringsFile,
    };

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

    fn simple_entry(source_value: &str) -> StringEntry {
        let mut localizations = IndexMap::new();
        localizations.insert(
            "en".to_string(),
            Localization {
                string_unit: Some(StringUnit {
                    state: TranslationState::Translated,
                    value: source_value.to_string(),
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
    fn test_valid_translation() {
        let file = make_file(vec![("greeting", simple_entry("Hello %@"))]);
        let translations = vec![simple_translation("greeting", "uk", "Привіт %@")];
        let rejected = validate_translations(&file, &translations);
        assert!(rejected.is_empty());
    }

    #[test]
    fn test_key_not_found() {
        let file = make_file(vec![("greeting", simple_entry("Hello"))]);
        let translations = vec![simple_translation("missing_key", "uk", "Щось")];
        let rejected = validate_translations(&file, &translations);
        assert_eq!(rejected.len(), 1);
        assert!(rejected[0].reason.contains("key not found"));
    }

    #[test]
    fn test_should_not_translate() {
        let entry = StringEntry {
            extraction_state: None,
            should_translate: false,
            comment: None,
            localizations: None,
        };
        let file = make_file(vec![("api_key", entry)]);
        let translations = vec![simple_translation("api_key", "uk", "ключ")];
        let rejected = validate_translations(&file, &translations);
        assert_eq!(rejected.len(), 1);
        assert!(rejected[0].reason.contains("shouldTranslate=false"));
    }

    #[test]
    fn test_empty_value() {
        let file = make_file(vec![("greeting", simple_entry("Hello"))]);
        let translations = vec![simple_translation("greeting", "uk", "")];
        let rejected = validate_translations(&file, &translations);
        assert_eq!(rejected.len(), 1);
        assert!(rejected[0].reason.contains("empty"));
    }

    #[test]
    fn test_specifier_count_mismatch() {
        let file = make_file(vec![("msg", simple_entry("%@ has %d items"))]);
        let translations = vec![simple_translation("msg", "uk", "%@ має елементи")];
        let rejected = validate_translations(&file, &translations);
        assert_eq!(rejected.len(), 1);
        assert!(rejected[0].reason.contains("count mismatch"));
    }

    #[test]
    fn test_specifier_type_mismatch() {
        let file = make_file(vec![("msg", simple_entry("Hello %@"))]);
        let translations = vec![simple_translation("msg", "uk", "Привіт %d")];
        let rejected = validate_translations(&file, &translations);
        assert_eq!(rejected.len(), 1);
        assert!(rejected[0].reason.contains("type mismatch"));
    }

    #[test]
    fn test_missing_plural_form() {
        let file = make_file(vec![("items", simple_entry("%lld items"))]);
        let mut plural_forms = BTreeMap::new();
        plural_forms.insert("one".to_string(), "%lld елемент".to_string());
        plural_forms.insert("other".to_string(), "%lld елементів".to_string());
        // Missing "few" and "many" for Ukrainian

        let translations = vec![CompletedTranslation {
            key: "items".to_string(),
            locale: "uk".to_string(),
            value: String::new(),
            plural_forms: Some(plural_forms),
            substitution_name: None,
        }];

        let rejected = validate_translations(&file, &translations);
        assert!(rejected.iter().any(|r| r.reason.contains("few")));
        assert!(rejected.iter().any(|r| r.reason.contains("many")));
    }

    #[test]
    fn test_plural_only_key_specifier_validation() {
        // Source key has only plural variations (no string_unit) — specifiers
        // should be extracted from the first plural form value
        let mut localizations = IndexMap::new();
        localizations.insert(
            "en".to_string(),
            Localization {
                string_unit: None,
                variations: Some(crate::model::xcstrings::Variations {
                    plural: Some({
                        let mut plural = std::collections::BTreeMap::new();
                        plural.insert(
                            "one".to_string(),
                            crate::model::xcstrings::PluralVariation {
                                string_unit: StringUnit {
                                    state: TranslationState::Translated,
                                    value: "%lld item".to_string(),
                                },
                            },
                        );
                        plural.insert(
                            "other".to_string(),
                            crate::model::xcstrings::PluralVariation {
                                string_unit: StringUnit {
                                    state: TranslationState::Translated,
                                    value: "%lld items".to_string(),
                                },
                            },
                        );
                        plural
                    }),
                    device: None,
                }),
                substitutions: None,
            },
        );
        let entry = StringEntry {
            extraction_state: None,
            should_translate: true,
            comment: None,
            localizations: Some(localizations),
        };
        let file = make_file(vec![("items", entry)]);

        // Submit plural forms WITH correct specifier (%lld) — should pass
        let mut plural_forms_ok = std::collections::BTreeMap::new();
        plural_forms_ok.insert("one".to_string(), "%lld Artikel".to_string());
        plural_forms_ok.insert("other".to_string(), "%lld Artikel".to_string());

        let translations_ok = vec![CompletedTranslation {
            key: "items".to_string(),
            locale: "de".to_string(),
            value: String::new(),
            plural_forms: Some(plural_forms_ok),
            substitution_name: None,
        }];

        let rejected = validate_translations(&file, &translations_ok);
        assert!(
            rejected.is_empty(),
            "valid plural translation for plural-only source should not be rejected: {:?}",
            rejected
        );

        // Submit plural forms WITHOUT specifier — should be rejected
        let mut plural_forms_bad = std::collections::BTreeMap::new();
        plural_forms_bad.insert("one".to_string(), "Ein Artikel".to_string());
        plural_forms_bad.insert("other".to_string(), "Artikel".to_string());

        let translations_bad = vec![CompletedTranslation {
            key: "items".to_string(),
            locale: "de".to_string(),
            value: String::new(),
            plural_forms: Some(plural_forms_bad),
            substitution_name: None,
        }];

        let rejected = validate_translations(&file, &translations_bad);
        assert!(
            !rejected.is_empty(),
            "missing specifier in plural form should be rejected"
        );
        assert!(
            rejected.iter().any(|r| r.reason.contains("specifier")),
            "rejection should mention specifier mismatch"
        );
    }

    #[test]
    fn test_extra_plural_forms_ok() {
        let file = make_file(vec![("items", simple_entry("%lld items"))]);
        let mut plural_forms = BTreeMap::new();
        plural_forms.insert("one".to_string(), "%lld item".to_string());
        plural_forms.insert("other".to_string(), "%lld items".to_string());
        plural_forms.insert("zero".to_string(), "no items".to_string()); // extra for "en"

        let translations = vec![CompletedTranslation {
            key: "items".to_string(),
            locale: "en".to_string(),
            value: String::new(),
            plural_forms: Some(plural_forms),
            substitution_name: None,
        }];

        let rejected = validate_translations(&file, &translations);
        // "zero" has no specifier but source has %lld — that's a specifier mismatch, not a plural form issue
        // Filter to only plural-form rejections
        let plural_rejections: Vec<_> = rejected
            .iter()
            .filter(|r| r.reason.contains("missing required plural form"))
            .collect();
        assert!(plural_rejections.is_empty());
    }
}
