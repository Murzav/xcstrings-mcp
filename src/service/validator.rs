use super::{assessment, submission};
use crate::model::specifier::{FormatComparison, compare_formats, compare_substitution_fragment};
use crate::model::translation::{CompletedTranslation, RejectedTranslation, ValidationIssue};
use crate::model::xcstrings::{XcStringsFile, paths::LeafStep};

#[derive(Debug, Default)]
pub struct TranslationValidationReport {
    pub rejected: Vec<RejectedTranslation>,
    pub warnings: Vec<ValidationIssue>,
    pub(crate) format_errors: Vec<ValidationIssue>,
    pub(crate) rejected_indices: Vec<usize>,
}

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
    let (plans, rejected) = submission::prepare(file, translations);
    let mut rejected: Vec<Vec<RejectedTranslation>> = rejected
        .into_iter()
        .map(|value| value.into_iter().collect())
        .collect();
    let mut report = TranslationValidationReport::default();
    for plan in plans {
        let request = &translations[plan.index];
        if let Err(reason) = submission::mutation::validate(file, &plan.leaves) {
            let code = if reason.contains("substitution metadata") {
                "missing_substitution_metadata"
            } else {
                "invalid_path"
            };
            rejected[plan.index].push(submission::reject(request, code, reason));
            continue;
        }
        let formats = validate_translation_formats(file, request);
        rejected[plan.index].extend(formats.rejected);
        report.warnings.extend(formats.warnings);
        report.format_errors.extend(formats.format_errors);
    }
    for (index, rejection) in rejected.into_iter().enumerate() {
        for rejection in rejection {
            report.rejected_indices.push(index);
            report.rejected.push(rejection);
        }
    }
    report
}

pub(crate) fn validate_translation_formats(
    file: &XcStringsFile,
    request: &CompletedTranslation,
) -> TranslationValidationReport {
    let mut report = TranslationValidationReport::default();
    let (plans, _) = submission::prepare(file, std::slice::from_ref(request));
    let Some(entry) = file.strings.get(&request.key) else {
        return report;
    };
    let source = entry
        .localizations
        .as_ref()
        .and_then(|locs| locs.get(&file.source_language));
    let target = entry
        .localizations
        .as_ref()
        .and_then(|locs| locs.get(&request.locale));
    for plan in plans {
        for (destination, value) in plan.leaves {
            // An explicit blank is an intentional Apple translation, not an absent target.
            if value.is_empty() {
                continue;
            }
            let source_value = source
                .and_then(|node| assessment::source_unit(node, &destination.path))
                .or_else(|| {
                    target.and_then(|node| assessment::source_unit(node, &destination.path))
                })
                .map_or(request.key.as_str(), |unit| unit.value.as_str());
            let mut comparison = FormatComparison::default();
            submission::formats::check_references(
                &mut comparison,
                source,
                target,
                &destination.path,
                source_value,
                &value,
            );
            if comparison.errors.is_empty() {
                comparison = if destination
                    .path
                    .iter()
                    .any(|step| matches!(step, LeafStep::Substitution(_)))
                {
                    compare_substitution_fragment(source_value, &value)
                } else {
                    let expanded = submission::formats::expand_references(
                        source,
                        target,
                        &destination.path,
                        source_value,
                    )
                    .and_then(|expanded_source| {
                        submission::formats::expand_references(
                            target,
                            source,
                            &destination.path,
                            &value,
                        )
                        .map(|expanded_target| (expanded_source, expanded_target))
                    });
                    match expanded {
                        Ok((source, target)) => compare_formats(&source, &target),
                        Err(message) => FormatComparison {
                            errors: vec![crate::model::specifier::FormatComparisonIssue {
                                code: "substitution_reference_mismatch",
                                message,
                            }],
                            ..Default::default()
                        },
                    }
                };
            }
            append_comparison(&mut report, request, &destination.path, comparison);
        }
    }
    report
}

fn append_comparison(
    report: &mut TranslationValidationReport,
    request: &CompletedTranslation,
    path: &[LeafStep],
    comparison: FormatComparison,
) {
    let context = if path.is_empty() {
        String::new()
    } else {
        format!(
            " (path: {})",
            serde_json::to_string(path).unwrap_or_default()
        )
    };
    for issue in comparison.errors {
        let message = format!("{}{context}", issue.message);
        let mut rejection = submission::reject(request, "invalid_translation", message.clone());
        rejection.path = Some(path.to_vec());
        report.rejected.push(rejection);
        report.format_errors.push(ValidationIssue {
            key: request.key.clone(),
            issue_type: issue.code.into(),
            message,
        });
    }
    report.warnings.extend(
        comparison
            .warnings
            .into_iter()
            .map(|issue| ValidationIssue {
                key: request.key.clone(),
                issue_type: issue.code.into(),
                message: format!("{}{context}", issue.message),
            }),
    );
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
            ..Default::default()
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
            ..Default::default()
        };
        let file = make_file(vec![("api_key", entry)]);
        let translations = vec![simple_translation("api_key", "uk", "ключ")];
        let rejected = validate_translations(&file, &translations);
        assert_eq!(rejected.len(), 1);
        assert!(rejected[0].reason.contains("shouldTranslate=false"));
    }

    #[test]
    fn explicit_empty_value_is_a_valid_translation() {
        let file = make_file(vec![("greeting", simple_entry("Hello"))]);
        let translations = vec![simple_translation("greeting", "uk", "")];
        let rejected = validate_translations(&file, &translations);
        assert!(rejected.is_empty());
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
    fn partial_plural_forms_are_valid_updates() {
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
            ..Default::default()
        }];

        let rejected = validate_translations(&file, &translations);
        assert!(rejected.is_empty());
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
                        let mut plural = crate::model::xcstrings::OrderedMap::new();
                        plural.insert(
                            "one".to_string(),
                            crate::model::xcstrings::PluralVariation {
                                string_unit: Some(StringUnit {
                                    state: TranslationState::Translated,
                                    value: "%lld item".to_string(),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            },
                        );
                        plural.insert(
                            "other".to_string(),
                            crate::model::xcstrings::PluralVariation {
                                string_unit: Some(StringUnit {
                                    state: TranslationState::Translated,
                                    value: "%lld items".to_string(),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            },
                        );
                        plural
                    }),
                    device: None,
                    ..Default::default()
                }),
                substitutions: None,
                ..Default::default()
            },
        );
        let entry = StringEntry {
            extraction_state: None,
            should_translate: true,
            comment: None,
            localizations: Some(localizations),
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
    fn substitution_without_metadata_is_rejected() {
        // A marker alone cannot establish argument identity or type.
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
            ..Default::default()
        }];

        let rejected = validate_translations(&file, &translations);
        assert_eq!(rejected.len(), 1);
        assert_eq!(
            rejected[0].code.as_deref(),
            Some("missing_substitution_metadata")
        );
    }
}
