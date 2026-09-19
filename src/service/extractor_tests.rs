use crate::model::xcstrings::TranslationState;
use indexmap::IndexMap;

use super::*;
use crate::model::xcstrings::{Localization, StringEntry, StringUnit, XcStringsFile};

fn make_file(strings: IndexMap<String, StringEntry>) -> XcStringsFile {
    XcStringsFile {
        source_language: "en".to_string(),
        strings,
        version: "1.0".to_string(),
        ..Default::default()
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
                    ..Default::default()
                }),
                variations: None,
                substitutions: None,
                ..Default::default()
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
                    ..Default::default()
                }),
                variations: None,
                substitutions: None,
                ..Default::default()
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
        ..Default::default()
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
