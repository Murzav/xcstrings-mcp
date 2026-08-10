use crate::model::plural::required_plural_forms;
use crate::model::specifier::{FormatComparison, compare_formats, compare_substitution_formats};

use crate::model::translation::{CompletedTranslation, RejectedTranslation, ValidationIssue};
use crate::model::xcstrings::{Localization, StringEntry, XcStringsFile};

#[derive(Debug, Default)]
pub struct TranslationValidationReport {
    pub rejected: Vec<RejectedTranslation>,
    pub warnings: Vec<ValidationIssue>,
    pub(crate) format_errors: Vec<ValidationIssue>,
}

/// Validate a batch of translations against the source file.
/// Returns a list of rejected translations with reasons.
pub fn validate_translations(
    file: &XcStringsFile,
    translations: &[CompletedTranslation],
) -> Vec<RejectedTranslation> {
    validate_translations_detailed(file, translations).rejected
}

pub fn validate_translations_detailed(
    file: &XcStringsFile,
    translations: &[CompletedTranslation],
) -> TranslationValidationReport {
    let mut report = TranslationValidationReport::default();

    for translation in translations {
        let entry = match file.strings.get(&translation.key) {
            Some(e) => e,
            None => {
                report.rejected.push(RejectedTranslation {
                    key: translation.key.clone(),
                    reason: "key not found in file".into(),
                });
                continue;
            }
        };

        if !entry.should_translate {
            report.rejected.push(RejectedTranslation {
                key: translation.key.clone(),
                reason: "key is marked as shouldTranslate=false".into(),
            });
            continue;
        }

        if translation.value.is_empty() && translation.plural_forms.is_none() {
            report.rejected.push(RejectedTranslation {
                key: translation.key.clone(),
                reason: "translation value is empty".into(),
            });
            continue;
        }

        if let Some(plural_forms) = &translation.plural_forms {
            let required = required_plural_forms(&translation.locale);
            for req in &required {
                let form_name = req.as_str().to_string();
                if !plural_forms.contains_key(&form_name) {
                    report.rejected.push(RejectedTranslation {
                        key: translation.key.clone(),
                        reason: format!("missing required plural form: {form_name}"),
                    });
                }
            }
        }

        let format_report = validate_translation_formats(file, translation);
        report.rejected.extend(format_report.rejected);
        report.warnings.extend(format_report.warnings);
        report.format_errors.extend(format_report.format_errors);
    }

    report
}

pub(crate) fn validate_translation_formats(
    file: &XcStringsFile,
    translation: &CompletedTranslation,
) -> TranslationValidationReport {
    let mut report = TranslationValidationReport::default();
    let Some(entry) = file.strings.get(&translation.key) else {
        return report;
    };
    let source = source_localization(file, entry);
    if let Some(plural_forms) = &translation.plural_forms {
        for (form, target) in plural_forms {
            let Some(source_value) = resolve_plural_source(
                source,
                &translation.key,
                translation.substitution_name.as_deref(),
                form,
            ) else {
                continue;
            };
            let comparison = if translation.substitution_name.is_some() {
                compare_substitution_formats(source_value, target)
            } else {
                compare_formats(source_value, target)
            };
            append_comparison(&mut report, &translation.key, Some(form), comparison);
        }
    } else {
        let source_value = source
            .and_then(|localization| localization.string_unit.as_ref())
            .map(|unit| unit.value.as_str())
            .unwrap_or(&translation.key);
        append_comparison(
            &mut report,
            &translation.key,
            None,
            compare_formats(source_value, &translation.value),
        );
    }
    report
}

fn append_comparison(
    report: &mut TranslationValidationReport,
    key: &str,
    plural_form: Option<&str>,
    comparison: FormatComparison,
) {
    let context = plural_form
        .map(|form| format!(" (plural form: {form})"))
        .unwrap_or_default();
    for issue in comparison.errors {
        let message = format!("{}{context}", issue.message);
        report.rejected.push(RejectedTranslation {
            key: key.to_string(),
            reason: message.clone(),
        });
        report.format_errors.push(ValidationIssue {
            key: key.to_string(),
            issue_type: issue.code.to_string(),
            message,
        });
    }
    report.warnings.extend(
        comparison
            .warnings
            .into_iter()
            .map(|issue| ValidationIssue {
                key: key.to_string(),
                issue_type: issue.code.to_string(),
                message: format!("{}{context}", issue.message),
            }),
    );
}

fn source_localization<'a>(
    file: &XcStringsFile,
    entry: &'a StringEntry,
) -> Option<&'a Localization> {
    entry
        .localizations
        .as_ref()
        .and_then(|localizations| localizations.get(&file.source_language))
}

fn resolve_plural_source<'a>(
    source: Option<&'a Localization>,
    key: &'a str,
    substitution_name: Option<&str>,
    form: &str,
) -> Option<&'a str> {
    if let Some(name) = substitution_name {
        return source
            .and_then(|localization| localization.substitutions.as_ref())
            .and_then(|substitutions| substitutions.get(name))
            .and_then(|value| plural_value(value, form));
    }

    if let Some(plural) = source
        .and_then(|localization| localization.variations.as_ref())
        .and_then(|variations| variations.plural.as_ref())
    {
        if let Some(value) = plural.get(form) {
            return Some(&value.string_unit.value);
        }
        if let Some(value) = plural.get("other").or_else(|| plural.values().next()) {
            return Some(&value.string_unit.value);
        }
    }

    Some(
        source
            .and_then(|localization| localization.string_unit.as_ref())
            .map(|unit| unit.value.as_str())
            .unwrap_or(key),
    )
}

fn plural_value<'a>(substitution: &'a serde_json::Value, form: &str) -> Option<&'a str> {
    let plural = substitution.get("variations")?.get("plural")?.as_object()?;
    plural
        .get(form)
        .or_else(|| plural.get("other"))
        .or_else(|| plural.values().next())?
        .get("stringUnit")?
        .get("value")?
        .as_str()
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
            "valid plural translation for plural-only source should not be rejected: {rejected:?}"
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

    #[test]
    fn test_substitution_skips_specifier_validation() {
        // Source has %#@BIRDS@ substitution marker — NOT a format specifier.
        // Substitution plural forms use %arg, which is different.
        // Validator must skip specifier check when substitution_name is set.
        let file = make_file(vec![("bird", simple_entry("I saw %#@BIRDS@ in the park"))]);

        let mut plural_forms = BTreeMap::new();
        plural_forms.insert("one".to_string(), "%arg bird".to_string());
        plural_forms.insert("other".to_string(), "%arg birds".to_string());

        let translations = vec![CompletedTranslation {
            key: "bird".to_string(),
            locale: "de".to_string(),
            value: String::new(),
            plural_forms: Some(plural_forms),
            substitution_name: Some("BIRDS".to_string()),
        }];

        let rejected = validate_translations(&file, &translations);
        assert!(
            rejected.is_empty(),
            "substitution plural forms should not be rejected for specifier mismatch: {rejected:?}"
        );
    }
}
