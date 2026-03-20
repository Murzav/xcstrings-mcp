use std::collections::BTreeSet;

use crate::model::plural::required_plural_forms;
use crate::model::specifier::extract_specifiers;
use crate::model::translation::{ValidationIssue, ValidationReport};
use crate::model::xcstrings::{TranslationState, XcStringsFile};

/// Validate all translations in a file for a specific locale (or all locales).
pub fn validate_file(file: &XcStringsFile, locale: Option<&str>) -> Vec<ValidationReport> {
    let locales_to_validate: Vec<String> = if let Some(l) = locale {
        vec![l.to_string()]
    } else {
        let mut all_locales = BTreeSet::new();
        for entry in file.strings.values() {
            if let Some(locs) = &entry.localizations {
                for loc_key in locs.keys() {
                    if loc_key != &file.source_language {
                        all_locales.insert(loc_key.clone());
                    }
                }
            }
        }
        all_locales.into_iter().collect()
    };

    let mut reports = Vec::with_capacity(locales_to_validate.len());

    for locale in &locales_to_validate {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        for (key, entry) in &file.strings {
            if !entry.should_translate {
                continue;
            }

            let source_text = entry
                .localizations
                .as_ref()
                .and_then(|locs| locs.get(&file.source_language))
                .and_then(|loc| loc.string_unit.as_ref())
                .map(|su| su.value.as_str());

            let source_specs = source_text.map(extract_specifiers);

            let target_loc = entry
                .localizations
                .as_ref()
                .and_then(|locs| locs.get(locale));

            let target_loc = match target_loc {
                Some(loc) => loc,
                None => continue,
            };

            let target_su = target_loc.string_unit.as_ref();
            let target_value = target_su.map(|su| su.value.as_str());

            // Error: empty translation (only for state=Translated, not for New/pending)
            let is_translated_state =
                target_su.is_some_and(|su| su.state == TranslationState::Translated);
            if is_translated_state
                && target_value.is_some_and(|v| v.is_empty())
                && target_loc.variations.is_none()
            {
                errors.push(ValidationIssue {
                    key: key.clone(),
                    issue_type: "empty_translation".into(),
                    message: "empty translation value".into(),
                });
            }

            // Format specifier checks
            if let (Some(src_specs), Some(target_val)) = (&source_specs, target_value) {
                let tgt_specs = extract_specifiers(target_val);
                if src_specs.len() != tgt_specs.len() {
                    errors.push(ValidationIssue {
                        key: key.clone(),
                        issue_type: "format_specifier_count_mismatch".into(),
                        message: format!(
                            "source has {} specifiers, translation has {}",
                            src_specs.len(),
                            tgt_specs.len()
                        ),
                    });
                } else {
                    for (src, tgt) in src_specs.iter().zip(tgt_specs.iter()) {
                        if !src.is_compatible_with(tgt) {
                            errors.push(ValidationIssue {
                                key: key.clone(),
                                issue_type: "format_specifier_type_mismatch".into(),
                                message: format!(
                                    "specifier mismatch: source has {}, translation has {}",
                                    src.raw, tgt.raw
                                ),
                            });
                        }
                    }
                }
            }

            // Missing plural forms
            if let Some(variations) = &target_loc.variations
                && let Some(plural) = &variations.plural
            {
                let required = required_plural_forms(locale);
                for req in &required {
                    let form_name = serde_json::to_string(req)
                        .unwrap_or_else(|_| "\"unknown\"".to_string())
                        .trim_matches('"')
                        .to_string();
                    if !plural.contains_key(&form_name) {
                        errors.push(ValidationIssue {
                            key: key.clone(),
                            issue_type: "missing_plural_form".into(),
                            message: format!("missing required plural form: {form_name}"),
                        });
                    }
                }
            }

            // Warning: identical to source
            if let (Some(src), Some(tgt)) = (source_text, target_value) {
                if tgt == src && locale != &file.source_language {
                    warnings.push(ValidationIssue {
                        key: key.clone(),
                        issue_type: "identical_to_source".into(),
                        message: "translation is identical to source text".into(),
                    });
                }

                // Warning: suspicious length (char-based to handle CJK correctly)
                let src_chars = src.chars().count();
                let tgt_chars = tgt.chars().count();
                if src_chars > 5 {
                    let max_chars = src_chars * 3;
                    let min_chars = ((src_chars as f64) * 0.3).max(1.0) as usize;
                    if tgt_chars > max_chars || tgt_chars < min_chars {
                        warnings.push(ValidationIssue {
                            key: key.clone(),
                            issue_type: "suspicious_length".into(),
                            message: format!(
                                "translation length {} chars is suspicious (source length {} chars)",
                                tgt_chars, src_chars
                            ),
                        });
                    }
                }
            }
        }

        reports.push(ValidationReport {
            locale: locale.clone(),
            errors,
            warnings,
        });
    }

    reports
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::*;
    use crate::model::xcstrings::{Localization, StringEntry, StringUnit, XcStringsFile};

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

    fn entry_with_translation(source: &str, locale: &str, translation: &str) -> StringEntry {
        let mut localizations = IndexMap::new();
        localizations.insert(
            "en".to_string(),
            Localization {
                string_unit: Some(StringUnit {
                    state: TranslationState::Translated,
                    value: source.to_string(),
                }),
                variations: None,
                substitutions: None,
            },
        );
        localizations.insert(
            locale.to_string(),
            Localization {
                string_unit: Some(StringUnit {
                    state: TranslationState::Translated,
                    value: translation.to_string(),
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

    #[test]
    fn test_validate_file_clean() {
        let file = make_file(vec![(
            "greeting",
            entry_with_translation("Hello", "uk", "Привіт"),
        )]);
        let reports = validate_file(&file, Some("uk"));
        assert_eq!(reports.len(), 1);
        assert!(reports[0].errors.is_empty());
        assert!(reports[0].warnings.is_empty());
    }

    #[test]
    fn test_validate_file_specifier_mismatch() {
        let file = make_file(vec![(
            "msg",
            entry_with_translation("%@ has %d items", "uk", "%@ має елементи"),
        )]);
        let reports = validate_file(&file, Some("uk"));
        assert_eq!(reports[0].errors.len(), 1);
        assert_eq!(
            reports[0].errors[0].issue_type,
            "format_specifier_count_mismatch"
        );
    }

    #[test]
    fn test_validate_file_specifier_type_mismatch() {
        let file = make_file(vec![(
            "msg",
            entry_with_translation("Hello %@", "uk", "Привіт %d"),
        )]);
        let reports = validate_file(&file, Some("uk"));
        assert_eq!(reports[0].errors.len(), 1);
        assert_eq!(
            reports[0].errors[0].issue_type,
            "format_specifier_type_mismatch"
        );
    }

    #[test]
    fn test_validate_file_identical_to_source() {
        let file = make_file(vec![(
            "ok_button",
            entry_with_translation("OK", "de", "OK"),
        )]);
        let reports = validate_file(&file, Some("de"));
        assert!(
            reports[0]
                .warnings
                .iter()
                .any(|w| w.issue_type == "identical_to_source")
        );
    }

    #[test]
    fn test_validate_file_suspicious_length() {
        let file = make_file(vec![(
            "long_key",
            entry_with_translation("This is a normal sentence", "de", "X"),
        )]);
        let reports = validate_file(&file, Some("de"));
        assert!(
            reports[0]
                .warnings
                .iter()
                .any(|w| w.issue_type == "suspicious_length")
        );
    }

    #[test]
    fn test_validate_file_specific_locale_filter() {
        let file = make_file(vec![(
            "greeting",
            entry_with_translation("Hello", "uk", "Привіт"),
        )]);
        let reports = validate_file(&file, Some("uk"));
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].locale, "uk");
    }

    #[test]
    fn test_validate_file_all_locales() {
        let mut localizations = IndexMap::new();
        localizations.insert(
            "en".to_string(),
            Localization {
                string_unit: Some(StringUnit {
                    state: TranslationState::Translated,
                    value: "Hello".to_string(),
                }),
                variations: None,
                substitutions: None,
            },
        );
        localizations.insert(
            "de".to_string(),
            Localization {
                string_unit: Some(StringUnit {
                    state: TranslationState::Translated,
                    value: "Hallo".to_string(),
                }),
                variations: None,
                substitutions: None,
            },
        );
        localizations.insert(
            "uk".to_string(),
            Localization {
                string_unit: Some(StringUnit {
                    state: TranslationState::Translated,
                    value: "Привіт".to_string(),
                }),
                variations: None,
                substitutions: None,
            },
        );
        let entry = StringEntry {
            extraction_state: None,
            should_translate: true,
            comment: None,
            localizations: Some(localizations),
        };
        let file = make_file(vec![("greeting", entry)]);

        let reports = validate_file(&file, None);
        assert_eq!(reports.len(), 2); // de and uk, not en (source)
        let locales: Vec<&str> = reports.iter().map(|r| r.locale.as_str()).collect();
        assert!(locales.contains(&"de"));
        assert!(locales.contains(&"uk"));
    }
}
