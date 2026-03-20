use tracing::warn;

use crate::error::XcStringsError;
use crate::model::specifier::extract_specifiers;
use crate::model::translation::TranslationUnit;
use crate::model::xcstrings::{ExtractionState, TranslationState, XcStringsFile};

/// Extract untranslated strings for a specific locale.
/// Returns `(batch, total_untranslated_count)`.
pub fn get_untranslated(
    file: &XcStringsFile,
    locale: &str,
    batch_size: usize,
    offset: usize,
) -> Result<(Vec<TranslationUnit>, usize), XcStringsError> {
    if locale.is_empty() {
        return Err(XcStringsError::LocaleNotFound("locale is empty".into()));
    }
    if batch_size == 0 || batch_size > 100 {
        return Err(XcStringsError::InvalidBatchSize(format!(
            "batch_size must be 1..=100, got {batch_size}"
        )));
    }

    let mut untranslated = Vec::new();

    // BTreeMap iteration = alphabetical order (deterministic)
    for (key, entry) in &file.strings {
        if !entry.should_translate {
            continue;
        }

        // Skip substitution-only keys (has substitutions but no string_unit).
        // These need plural translation via get_untranslated_plurals, not simple translation.
        // Keys with BOTH string_unit and substitutions are included here for the simple
        // string_unit translation; their substitution plurals are handled separately.
        if let Some(localizations) = &entry.localizations
            && let Some(source_loc) = localizations.get(&file.source_language)
            && source_loc.substitutions.is_some()
            && source_loc.string_unit.is_none()
        {
            warn!(key = %key, "skipping substitution-only key — handled by plural_extractor");
            continue;
        }

        let is_untranslated = match &entry.localizations {
            None => true,
            Some(locs) => match locs.get(locale) {
                None => true,
                Some(loc) => {
                    if let Some(su) = &loc.string_unit {
                        su.state != TranslationState::Translated
                    } else if loc.variations.is_some() {
                        // Has variations — treat as translated for Phase 1
                        false
                    } else {
                        true
                    }
                }
            },
        };

        if !is_untranslated {
            continue;
        }

        // Get source text: from source_language localization, fallback to key name
        let source_text = entry
            .localizations
            .as_ref()
            .and_then(|locs| locs.get(&file.source_language))
            .and_then(|loc| loc.string_unit.as_ref())
            .map(|su| su.value.clone())
            .unwrap_or_else(|| key.clone());

        let specifiers = extract_specifiers(&source_text);
        let format_specifier_strings: Vec<String> =
            specifiers.iter().map(|s| s.raw.clone()).collect();

        let has_plurals = entry
            .localizations
            .as_ref()
            .and_then(|locs| locs.get(&file.source_language))
            .and_then(|loc| loc.variations.as_ref())
            .is_some_and(|v| v.plural.is_some());

        let has_substitutions = entry
            .localizations
            .as_ref()
            .and_then(|locs| locs.get(&file.source_language))
            .and_then(|loc| loc.substitutions.as_ref())
            .is_some();

        untranslated.push(TranslationUnit {
            key: key.clone(),
            source_text,
            target_locale: locale.to_string(),
            comment: entry.comment.clone(),
            format_specifiers: format_specifier_strings,
            has_plurals,
            has_substitutions,
        });
    }

    let total = untranslated.len();

    let batch: Vec<TranslationUnit> = untranslated
        .into_iter()
        .skip(offset)
        .take(batch_size)
        .collect();

    Ok((batch, total))
}

/// Extract strings with `extractionState == Stale`.
/// The `locale` parameter sets `target_locale` on returned units (stale is a key-level
/// property, not locale-specific — all locales return the same stale keys).
/// Returns `(batch, total_stale_count)`.
pub fn get_stale(
    file: &XcStringsFile,
    locale: &str,
    batch_size: usize,
    offset: usize,
) -> Result<(Vec<TranslationUnit>, usize), XcStringsError> {
    if locale.is_empty() {
        return Err(XcStringsError::LocaleNotFound("locale is empty".into()));
    }
    if batch_size == 0 || batch_size > 100 {
        return Err(XcStringsError::InvalidBatchSize(format!(
            "batch_size must be 1..=100, got {batch_size}"
        )));
    }

    let mut stale = Vec::new();

    for (key, entry) in &file.strings {
        if !entry.should_translate {
            continue;
        }

        if entry.extraction_state != Some(ExtractionState::Stale) {
            continue;
        }

        let source_text = entry
            .localizations
            .as_ref()
            .and_then(|locs| locs.get(&file.source_language))
            .and_then(|loc| loc.string_unit.as_ref())
            .map(|su| su.value.clone())
            .unwrap_or_else(|| key.clone());

        let specifiers = extract_specifiers(&source_text);
        let format_specifier_strings: Vec<String> =
            specifiers.iter().map(|s| s.raw.clone()).collect();

        let has_plurals = entry
            .localizations
            .as_ref()
            .and_then(|locs| locs.get(&file.source_language))
            .and_then(|loc| loc.variations.as_ref())
            .is_some_and(|v| v.plural.is_some());

        let has_substitutions = entry
            .localizations
            .as_ref()
            .and_then(|locs| locs.get(&file.source_language))
            .and_then(|loc| loc.substitutions.as_ref())
            .is_some();

        stale.push(TranslationUnit {
            key: key.clone(),
            source_text,
            target_locale: locale.to_string(),
            comment: entry.comment.clone(),
            format_specifiers: format_specifier_strings,
            has_plurals,
            has_substitutions,
        });
    }

    let total = stale.len();

    let batch: Vec<TranslationUnit> = stale.into_iter().skip(offset).take(batch_size).collect();

    Ok((batch, total))
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::*;
    use crate::model::xcstrings::{Localization, StringEntry, StringUnit, XcStringsFile};

    fn make_file(strings: IndexMap<String, StringEntry>) -> XcStringsFile {
        XcStringsFile {
            source_language: "en".to_string(),
            strings,
            version: "1.0".to_string(),
        }
    }

    fn make_entry(
        source_value: Option<&str>,
        locales: &[(&str, &str, TranslationState)],
    ) -> StringEntry {
        let mut localizations = IndexMap::new();

        if let Some(val) = source_value {
            localizations.insert(
                "en".to_string(),
                Localization {
                    string_unit: Some(StringUnit {
                        state: TranslationState::Translated,
                        value: val.to_string(),
                    }),
                    variations: None,
                    substitutions: None,
                },
            );
        }

        for (locale, value, state) in locales {
            localizations.insert(
                locale.to_string(),
                Localization {
                    string_unit: Some(StringUnit {
                        state: state.clone(),
                        value: value.to_string(),
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

    #[test]
    fn test_empty_file() {
        let file = make_file(IndexMap::new());
        let (batch, total) = get_untranslated(&file, "de", 10, 0).unwrap();
        assert!(batch.is_empty());
        assert_eq!(total, 0);
    }

    #[test]
    fn test_basic_untranslated() {
        let content = include_str!("../../tests/fixtures/simple.xcstrings");
        let file: XcStringsFile = serde_json::from_str(content).unwrap();

        // "de" doesn't exist → both translatable keys are untranslated
        let (batch, total) = get_untranslated(&file, "de", 100, 0).unwrap();
        assert_eq!(total, 2);
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn test_already_translated_skipped() {
        let content = include_str!("../../tests/fixtures/simple.xcstrings");
        let file: XcStringsFile = serde_json::from_str(content).unwrap();

        // "uk" has greeting translated, but welcome_message has no uk locale
        let (batch, total) = get_untranslated(&file, "uk", 100, 0).unwrap();
        assert_eq!(total, 1);
        assert_eq!(batch[0].key, "welcome_message");
    }

    #[test]
    fn test_batch_pagination() {
        let mut strings = IndexMap::new();
        for i in 0..5 {
            strings.insert(
                format!("key_{i}"),
                make_entry(Some(&format!("val {i}")), &[]),
            );
        }
        let file = make_file(strings);

        let (batch, total) = get_untranslated(&file, "de", 2, 0).unwrap();
        assert_eq!(total, 5);
        assert_eq!(batch.len(), 2);

        let (batch, _) = get_untranslated(&file, "de", 2, 2).unwrap();
        assert_eq!(batch.len(), 2);

        let (batch, _) = get_untranslated(&file, "de", 2, 4).unwrap();
        assert_eq!(batch.len(), 1);
    }

    #[test]
    fn test_should_not_translate_filtered() {
        let content = include_str!("../../tests/fixtures/should_not_translate.xcstrings");
        let file: XcStringsFile = serde_json::from_str(content).unwrap();

        let (batch, total) = get_untranslated(&file, "de", 100, 0).unwrap();
        assert_eq!(total, 1);
        assert_eq!(batch[0].key, "hello");
    }

    #[test]
    fn test_invalid_batch_size() {
        let file = make_file(IndexMap::new());
        let result = get_untranslated(&file, "de", 0, 0);
        assert!(matches!(
            result.unwrap_err(),
            XcStringsError::InvalidBatchSize(_)
        ));
    }

    #[test]
    fn test_source_text_fallback() {
        let mut strings = IndexMap::new();
        // Entry with no source language localization → key name used as source_text
        strings.insert("my_key".to_string(), make_entry(None, &[]));
        let file = make_file(strings);

        let (batch, _) = get_untranslated(&file, "de", 10, 0).unwrap();
        assert_eq!(batch[0].source_text, "my_key");
    }

    #[test]
    fn test_format_specifiers_extracted() {
        let mut strings = IndexMap::new();
        strings.insert(
            "greet".to_string(),
            make_entry(Some("Hello %@, you have %lld items"), &[]),
        );
        let file = make_file(strings);

        let (batch, _) = get_untranslated(&file, "de", 10, 0).unwrap();
        assert_eq!(batch[0].format_specifiers, vec!["%@", "%lld"]);
    }

    // --- get_stale tests ---

    fn make_stale_entry(source_value: Option<&str>) -> StringEntry {
        let mut entry = make_entry(source_value, &[]);
        entry.extraction_state = Some(ExtractionState::Stale);
        entry
    }

    #[test]
    fn test_stale_no_stale_keys() {
        let mut strings = IndexMap::new();
        strings.insert(
            "key1".to_string(),
            make_entry(
                Some("Hello"),
                &[("de", "Hallo", TranslationState::Translated)],
            ),
        );
        let file = make_file(strings);

        let (batch, total) = get_stale(&file, "de", 10, 0).unwrap();
        assert!(batch.is_empty());
        assert_eq!(total, 0);
    }

    #[test]
    fn test_stale_keys_returned() {
        let mut strings = IndexMap::new();
        strings.insert("stale_key".to_string(), make_stale_entry(Some("Old text")));
        strings.insert("fresh_key".to_string(), make_entry(Some("Fresh"), &[]));
        let file = make_file(strings);

        let (batch, total) = get_stale(&file, "de", 10, 0).unwrap();
        assert_eq!(total, 1);
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].key, "stale_key");
        assert_eq!(batch[0].source_text, "Old text");
    }

    #[test]
    fn test_stale_should_not_translate_excluded() {
        let mut strings = IndexMap::new();
        let mut entry = make_stale_entry(Some("Do not translate"));
        entry.should_translate = false;
        strings.insert("no_translate".to_string(), entry);
        strings.insert(
            "stale_ok".to_string(),
            make_stale_entry(Some("Translate me")),
        );
        let file = make_file(strings);

        let (batch, total) = get_stale(&file, "de", 10, 0).unwrap();
        assert_eq!(total, 1);
        assert_eq!(batch[0].key, "stale_ok");
    }

    #[test]
    fn test_stale_batch_pagination() {
        let mut strings = IndexMap::new();
        for i in 0..5 {
            strings.insert(
                format!("stale_{i}"),
                make_stale_entry(Some(&format!("val {i}"))),
            );
        }
        let file = make_file(strings);

        let (batch, total) = get_stale(&file, "de", 2, 0).unwrap();
        assert_eq!(total, 5);
        assert_eq!(batch.len(), 2);

        let (batch, _) = get_stale(&file, "de", 2, 4).unwrap();
        assert_eq!(batch.len(), 1);
    }
}
