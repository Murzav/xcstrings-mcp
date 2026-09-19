use super::{assessment, validator};
use crate::model::translation::{CompletedTranslation, ValidationIssue, ValidationReport};
use crate::model::xcstrings::{XcStringsFile, paths::LeafStep};
use std::collections::BTreeSet;

/// Validate all known leaves, retaining precise scope in every diagnostic.
pub fn validate_file(file: &XcStringsFile, locale: Option<&str>) -> Vec<ValidationReport> {
    let locales: BTreeSet<String> = locale
        .map(|locale| std::iter::once(locale.into()).collect())
        .unwrap_or_else(|| {
            file.strings
                .values()
                .filter_map(|entry| entry.localizations.as_ref())
                .flat_map(|locs| locs.keys())
                .filter(|locale| *locale != &file.source_language)
                .cloned()
                .collect()
        });
    locales.into_iter().map(|locale| {
        let mut report = ValidationReport { locale: locale.clone(), errors: Vec::new(), warnings: Vec::new() };
        for (key, entry) in &file.strings {
            if !entry.should_translate || !entry.localizations.as_ref().is_some_and(|locs| locs.contains_key(&locale)) { continue; }
            let assessment = assessment::assess(key, entry, &file.source_language, &locale);
            for diagnostic in assessment.diagnostics {
                let code = serde_json::to_value(diagnostic.code).ok().and_then(|value| value.as_str().map(str::to_owned)).unwrap_or_else(|| "invalid_shape".into());
                report.errors.push(ValidationIssue { key: key.clone(), issue_type: code, message: format!("{} (path: {})", diagnostic.detail, serde_json::to_string(&diagnostic.path).unwrap_or_default()) });
            }
            for leaf in assessment.leaves {
                let path = serde_json::to_string(&leaf.path).unwrap_or_default();
                let Some(value) = leaf.value else {
                    let (code, message) = if let Some(LeafStep::Plural(category)) = leaf.path.last() { ("missing_plural_form", format!("missing required plural form: {category}")) } else { ("missing_destination", "missing required translation destination".into()) };
                    report.errors.push(ValidationIssue { key: key.clone(), issue_type: code.into(), message: format!("{message} (path: {path})") });
                    continue;
                };
                let request = CompletedTranslation { key: key.clone(), locale: locale.clone(), value: value.clone(), path: Some(leaf.path), ..Default::default() };
                let formats = validator::validate_translation_formats(file, &request);
                report.errors.extend(formats.format_errors);
                report.warnings.extend(formats.warnings);
                if value.is_empty() { continue; }
                if value == leaf.source_text && locale != file.source_language {
                    report.warnings.push(ValidationIssue { key: key.clone(), issue_type: "identical_to_source".into(), message: format!("translation is identical to source text (path: {path})") });
                }
                let source_len = leaf.source_text.chars().count();
                let target_len = value.chars().count();
                if source_len > 5 && (target_len > source_len * 3 || target_len < ((source_len as f64 * 0.3).max(1.0) as usize)) {
                    report.warnings.push(ValidationIssue { key: key.clone(), issue_type: "suspicious_length".into(), message: format!("translation length {target_len} chars is suspicious (source length {source_len} chars, path: {path})") });
                }
            }
        }
        report
    }).collect()
}

#[cfg(test)]
mod tests {
    use crate::model::xcstrings::TranslationState;
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
            ..Default::default()
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
                    ..Default::default()
                }),
                variations: None,
                substitutions: None,
                ..Default::default()
            },
        );
        localizations.insert(
            locale.to_string(),
            Localization {
                string_unit: Some(StringUnit {
                    state: TranslationState::Translated,
                    value: translation.to_string(),
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
                    ..Default::default()
                }),
                variations: None,
                substitutions: None,
                ..Default::default()
            },
        );
        localizations.insert(
            "de".to_string(),
            Localization {
                string_unit: Some(StringUnit {
                    state: TranslationState::Translated,
                    value: "Hallo".to_string(),
                    ..Default::default()
                }),
                variations: None,
                substitutions: None,
                ..Default::default()
            },
        );
        localizations.insert(
            "uk".to_string(),
            Localization {
                string_unit: Some(StringUnit {
                    state: TranslationState::Translated,
                    value: "Привіт".to_string(),
                    ..Default::default()
                }),
                variations: None,
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
        let file = make_file(vec![("greeting", entry)]);

        let reports = validate_file(&file, None);
        assert_eq!(reports.len(), 2); // de and uk, not en (source)
        let locales: Vec<&str> = reports.iter().map(|r| r.locale.as_str()).collect();
        assert!(locales.contains(&"de"));
        assert!(locales.contains(&"uk"));
    }
}
