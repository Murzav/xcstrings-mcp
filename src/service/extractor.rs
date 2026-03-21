use tracing::warn;

use crate::error::XcStringsError;
use crate::model::specifier::extract_specifiers;
use crate::model::translation::TranslationUnit;
use crate::model::xcstrings::{ExtractionState, StringEntry, TranslationState, XcStringsFile};

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

        untranslated.push(build_translation_unit(
            key,
            entry,
            &file.source_language,
            locale,
        ));
    }

    let total = untranslated.len();

    let batch: Vec<TranslationUnit> = untranslated
        .into_iter()
        .skip(offset)
        .take(batch_size)
        .collect();

    Ok((batch, total))
}

/// Build a `TranslationUnit` from a key/entry pair.
/// Shared by `get_untranslated`, `get_stale`, and `search_keys`.
fn build_translation_unit(
    key: &str,
    entry: &StringEntry,
    source_language: &str,
    locale: &str,
) -> TranslationUnit {
    let source_text = entry
        .localizations
        .as_ref()
        .and_then(|locs| locs.get(source_language))
        .and_then(|loc| loc.string_unit.as_ref())
        .map(|su| su.value.clone())
        .unwrap_or_else(|| key.to_string());

    let specifiers = extract_specifiers(&source_text);
    let format_specifier_strings: Vec<String> = specifiers.iter().map(|s| s.raw.clone()).collect();

    let has_plurals = entry
        .localizations
        .as_ref()
        .and_then(|locs| locs.get(source_language))
        .and_then(|loc| loc.variations.as_ref())
        .is_some_and(|v| v.plural.is_some());

    let has_substitutions = entry
        .localizations
        .as_ref()
        .and_then(|locs| locs.get(source_language))
        .and_then(|loc| loc.substitutions.as_ref())
        .is_some();

    TranslationUnit {
        key: key.to_string(),
        source_text,
        target_locale: locale.to_string(),
        comment: entry.comment.clone(),
        format_specifiers: format_specifier_strings,
        has_plurals,
        has_substitutions,
    }
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

        stale.push(build_translation_unit(
            key,
            entry,
            &file.source_language,
            locale,
        ));
    }

    let total = stale.len();

    let batch: Vec<TranslationUnit> = stale.into_iter().skip(offset).take(batch_size).collect();

    Ok((batch, total))
}

/// Search keys by substring pattern (case-insensitive).
/// Matches against both key name and source text.
/// Returns matching `TranslationUnit`s with pagination.
/// An empty pattern matches all translatable keys.
pub fn search_keys(
    file: &XcStringsFile,
    pattern: &str,
    locale: &str,
    batch_size: usize,
    offset: usize,
) -> Result<(Vec<TranslationUnit>, usize), XcStringsError> {
    if batch_size == 0 || batch_size > 100 {
        return Err(XcStringsError::InvalidBatchSize(format!(
            "batch_size must be 1..=100, got {batch_size}"
        )));
    }

    let pattern_lower = pattern.to_lowercase();

    let mut matching = Vec::new();

    for (key, entry) in &file.strings {
        if !entry.should_translate {
            continue;
        }

        // Empty pattern matches all translatable keys
        if !pattern_lower.is_empty() {
            let key_matches = key.to_lowercase().contains(&pattern_lower);

            let source_text = entry
                .localizations
                .as_ref()
                .and_then(|locs| locs.get(&file.source_language))
                .and_then(|loc| loc.string_unit.as_ref())
                .map(|su| su.value.as_str())
                .unwrap_or("");
            let source_matches = source_text.to_lowercase().contains(&pattern_lower);

            if !key_matches && !source_matches {
                continue;
            }
        }

        matching.push(build_translation_unit(
            key,
            entry,
            &file.source_language,
            locale,
        ));
    }

    let total = matching.len();
    let batch: Vec<TranslationUnit> = matching.into_iter().skip(offset).take(batch_size).collect();

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

    // --- search_keys tests ---

    #[test]
    fn test_search_by_key_name() {
        let mut strings = IndexMap::new();
        strings.insert("greeting_hello".to_string(), make_entry(Some("Hello"), &[]));
        strings.insert("farewell_bye".to_string(), make_entry(Some("Goodbye"), &[]));
        let file = make_file(strings);

        let (batch, total) = search_keys(&file, "greet", "de", 30, 0).unwrap();
        assert_eq!(total, 1);
        assert_eq!(batch[0].key, "greeting_hello");
    }

    #[test]
    fn test_search_by_source_text() {
        let mut strings = IndexMap::new();
        strings.insert("key_a".to_string(), make_entry(Some("Welcome home"), &[]));
        strings.insert("key_b".to_string(), make_entry(Some("Goodbye"), &[]));
        let file = make_file(strings);

        let (batch, total) = search_keys(&file, "welcome", "de", 30, 0).unwrap();
        assert_eq!(total, 1);
        assert_eq!(batch[0].key, "key_a");
        assert_eq!(batch[0].source_text, "Welcome home");
    }

    #[test]
    fn test_search_empty_pattern_returns_all() {
        let mut strings = IndexMap::new();
        strings.insert("key_a".to_string(), make_entry(Some("Alpha"), &[]));
        strings.insert("key_b".to_string(), make_entry(Some("Beta"), &[]));
        let file = make_file(strings);

        let (batch, total) = search_keys(&file, "", "de", 30, 0).unwrap();
        assert_eq!(total, 2);
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn test_search_no_matches() {
        let mut strings = IndexMap::new();
        strings.insert("key_a".to_string(), make_entry(Some("Hello"), &[]));
        let file = make_file(strings);

        let (batch, total) = search_keys(&file, "xyz_no_match", "de", 30, 0).unwrap();
        assert_eq!(total, 0);
        assert!(batch.is_empty());
    }

    #[test]
    fn test_search_pagination() {
        let mut strings = IndexMap::new();
        for i in 0..5 {
            strings.insert(
                format!("search_key_{i}"),
                make_entry(Some(&format!("val {i}")), &[]),
            );
        }
        let file = make_file(strings);

        let (batch, total) = search_keys(&file, "search", "de", 2, 0).unwrap();
        assert_eq!(total, 5);
        assert_eq!(batch.len(), 2);

        let (batch, _) = search_keys(&file, "search", "de", 2, 3).unwrap();
        assert_eq!(batch.len(), 2);

        let (batch, _) = search_keys(&file, "search", "de", 2, 4).unwrap();
        assert_eq!(batch.len(), 1);
    }

    #[test]
    fn test_search_case_insensitive() {
        let mut strings = IndexMap::new();
        strings.insert("MyKey".to_string(), make_entry(Some("Hello World"), &[]));
        let file = make_file(strings);

        let (batch, total) = search_keys(&file, "mykey", "de", 30, 0).unwrap();
        assert_eq!(total, 1);
        assert_eq!(batch[0].key, "MyKey");

        let (batch, total) = search_keys(&file, "HELLO", "de", 30, 0).unwrap();
        assert_eq!(total, 1);
        assert_eq!(batch[0].source_text, "Hello World");
    }
}
