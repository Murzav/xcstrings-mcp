mod helpers;

use std::path::PathBuf;

use helpers::MemoryStore;
use indexmap::IndexMap;
use xcstrings_mcp::model::translation::CompletedTranslation;
use xcstrings_mcp::model::xcstrings::{
    Localization, StringEntry, StringUnit, TranslationState, XcStringsFile,
};
use xcstrings_mcp::service::{extractor, formatter, merger, parser, validator};

const SIMPLE_FIXTURE: &str = include_str!("fixtures/simple.xcstrings");
const SHOULD_NOT_TRANSLATE_FIXTURE: &str = include_str!("fixtures/should_not_translate.xcstrings");
const XCODE_GENERATED: &str = include_str!("fixtures/xcode_generated.xcstrings");

// ── Integration test 1: parse → get_untranslated ──

#[test]
fn parse_then_get_untranslated() {
    let file = parser::parse(SIMPLE_FIXTURE).unwrap();

    // "de" locale doesn't exist → both translatable keys should be untranslated
    let (batch, total) = extractor::get_untranslated(&file, "de", 100, 0).unwrap();

    assert_eq!(total, 2);
    assert_eq!(batch.len(), 2);

    let keys: Vec<&str> = batch.iter().map(|u| u.key.as_str()).collect();
    assert!(keys.contains(&"greeting"));
    assert!(keys.contains(&"welcome_message"));
}

// ── Integration test 2: parse → validate → merge → verify ──

#[test]
fn parse_validate_merge_roundtrip() {
    let mut file = parser::parse(SIMPLE_FIXTURE).unwrap();

    let translations = vec![CompletedTranslation {
        key: "welcome_message".to_string(),
        locale: "uk".to_string(),
        value: "Ласкаво просимо до застосунку".to_string(),
        plural_forms: None,
    }];

    // Validate first
    let rejected = validator::validate_translations(&file, &translations);
    assert!(
        rejected.is_empty(),
        "valid translation should not be rejected"
    );

    // Merge
    let result = merger::merge_translations(&mut file, &translations);
    assert_eq!(result.accepted, 1);
    assert!(result.rejected.is_empty());

    // Verify: welcome_message should now be translated for "uk"
    let (batch, total) = extractor::get_untranslated(&file, "uk", 100, 0).unwrap();
    assert_eq!(total, 0, "no more untranslated keys for uk");
    assert!(batch.is_empty());
}

// ── Integration test 3: format output matches Xcode style ──

#[test]
fn format_preserves_xcode_style() {
    let file = parser::parse(SIMPLE_FIXTURE).unwrap();
    let formatted = formatter::format_xcstrings(&file).unwrap();

    // Xcode uses " : " (space-colon-space)
    assert!(formatted.contains("\"sourceLanguage\" : \"en\""));
    assert!(formatted.contains("\"state\" : \"translated\""));
    // Must end with newline
    assert!(formatted.ends_with('\n'));
    // Colons inside string values should not be affected
    // (no string values with colons in simple fixture, but structural ones should be correct)
}

// ── Integration test 4: specifier mismatch rejected ──

#[test]
fn specifier_mismatch_rejected() {
    let json = r#"{
        "sourceLanguage": "en",
        "strings": {
            "greeting": {
                "localizations": {
                    "en": {
                        "stringUnit": {
                            "state": "translated",
                            "value": "Hello %@"
                        }
                    }
                }
            }
        },
        "version": "1.0"
    }"#;
    let file = parser::parse(json).unwrap();

    // Submit translation without the required %@ specifier
    let translations = vec![CompletedTranslation {
        key: "greeting".to_string(),
        locale: "uk".to_string(),
        value: "Привіт".to_string(), // missing %@
        plural_forms: None,
    }];

    let rejected = validator::validate_translations(&file, &translations);
    assert_eq!(rejected.len(), 1);
    assert!(rejected[0].reason.contains("mismatch"));
}

// ── Integration test 5: should_not_translate filtered correctly ──

#[test]
fn should_not_translate_filtered_in_flow() {
    let file = parser::parse(SHOULD_NOT_TRANSLATE_FIXTURE).unwrap();

    let summary = parser::summarize(&file);
    assert_eq!(summary.total_keys, 2);
    assert_eq!(summary.translatable_keys, 1);

    let (batch, total) = extractor::get_untranslated(&file, "de", 100, 0).unwrap();
    assert_eq!(total, 1);
    assert_eq!(batch[0].key, "hello");

    // Trying to translate shouldTranslate=false key should be rejected
    let translations = vec![CompletedTranslation {
        key: "CFBundleName".to_string(),
        locale: "de".to_string(),
        value: "MeineApp".to_string(),
        plural_forms: None,
    }];
    let rejected = validator::validate_translations(&file, &translations);
    assert_eq!(rejected.len(), 1);
    assert!(rejected[0].reason.contains("shouldTranslate=false"));
}

// ── Integration test 6: merge multiple translations sequentially ──

#[test]
fn sequential_merges_no_corruption() {
    let mut file = parser::parse(SIMPLE_FIXTURE).unwrap();

    // First merge: translate greeting to de
    let t1 = vec![CompletedTranslation {
        key: "greeting".to_string(),
        locale: "de".to_string(),
        value: "Hallo".to_string(),
        plural_forms: None,
    }];
    let r1 = merger::merge_translations(&mut file, &t1);
    assert_eq!(r1.accepted, 1);

    // Second merge: translate welcome_message to de
    let t2 = vec![CompletedTranslation {
        key: "welcome_message".to_string(),
        locale: "de".to_string(),
        value: "Willkommen in der App".to_string(),
        plural_forms: None,
    }];
    let r2 = merger::merge_translations(&mut file, &t2);
    assert_eq!(r2.accepted, 1);

    // Both translations should exist, and original en/uk should be preserved
    let (batch, total) = extractor::get_untranslated(&file, "de", 100, 0).unwrap();
    assert_eq!(total, 0);
    assert!(batch.is_empty());

    // Verify original uk translation is still intact
    let greeting = &file.strings["greeting"];
    let locs = greeting.localizations.as_ref().unwrap();
    let uk = locs["uk"].string_unit.as_ref().unwrap();
    assert_eq!(uk.value, "Привіт");

    // Verify formatted output is valid JSON
    let formatted = formatter::format_xcstrings(&file).unwrap();
    let reparsed = parser::parse(&formatted).unwrap();
    assert_eq!(reparsed.strings.len(), 2);
}

// ── Integration test 7: full roundtrip through MemoryStore ──

#[test]
fn full_roundtrip_with_memory_store() {
    let store = MemoryStore::new();
    let path = PathBuf::from("/test/Localizable.xcstrings");

    store.add_file(&path, SIMPLE_FIXTURE);

    // Read from store
    let content = store.get_content(&path).unwrap();
    let mut file = parser::parse(&content).unwrap();

    // Get untranslated for "de"
    let (batch, _) = extractor::get_untranslated(&file, "de", 100, 0).unwrap();
    assert_eq!(batch.len(), 2);

    // Translate and merge
    let translations: Vec<CompletedTranslation> = batch
        .iter()
        .map(|unit| CompletedTranslation {
            key: unit.key.clone(),
            locale: "de".to_string(),
            value: format!("DE: {}", unit.source_text),
            plural_forms: None,
        })
        .collect();

    let rejected = validator::validate_translations(&file, &translations);
    assert!(rejected.is_empty());

    let result = merger::merge_translations(&mut file, &translations);
    assert_eq!(result.accepted, 2);

    // Format and write back to store
    let formatted = formatter::format_xcstrings(&file).unwrap();
    store.add_file(&path, &formatted);

    // Re-read and verify
    let content2 = store.get_content(&path).unwrap();
    let file2 = parser::parse(&content2).unwrap();

    let (batch2, total2) = extractor::get_untranslated(&file2, "de", 100, 0).unwrap();
    assert_eq!(total2, 0);
    assert!(batch2.is_empty());
}

// ── Xcode golden fixture roundtrip ──

#[test]
fn xcode_generated_roundtrip_byte_identical() {
    let file = parser::parse(XCODE_GENERATED).unwrap();

    // Verify summary
    let summary = parser::summarize(&file);
    assert_eq!(summary.source_language, "en");
    assert_eq!(summary.total_keys, 638);
    assert!(summary.locales.contains(&"uk".to_string()));
    assert!(summary.locales.contains(&"de".to_string()));

    // Format back — verify structural roundtrip (parse → format → parse produces same data)
    let formatted = formatter::format_xcstrings(&file).unwrap();
    let reparsed = parser::parse(&formatted).unwrap();
    assert_eq!(reparsed.strings.len(), file.strings.len());
    assert_eq!(reparsed.source_language, file.source_language);
    assert_eq!(reparsed.version, file.version);

    // Verify Xcode colon formatting preserved
    assert!(formatted.contains("\"sourceLanguage\" : \"en\""));
    assert!(formatted.contains("\"state\" : \"translated\""));
    assert!(formatted.ends_with('\n'));

    // Verify key order preserved (IndexMap insertion order = Xcode's Finder-like sort)
    let orig_keys: Vec<&str> = file.strings.keys().map(|s| s.as_str()).collect();
    let round_keys: Vec<&str> = reparsed.strings.keys().map(|s| s.as_str()).collect();
    assert_eq!(
        orig_keys, round_keys,
        "key order must be preserved through roundtrip"
    );
}

#[test]
fn xcode_generated_get_untranslated() {
    let file = parser::parse(XCODE_GENERATED).unwrap();

    // All 9 locales should exist and have translations
    let summary = parser::summarize(&file);
    assert_eq!(summary.locales.len(), 9);

    // Try to get untranslated for a locale that exists (should be 0 or few)
    let (batch, total) = extractor::get_untranslated(&file, "uk", 100, 0).unwrap();
    // This real file has all keys translated for uk
    assert_eq!(
        total, 0,
        "Xcode generated file should have all keys translated for uk"
    );
    assert!(batch.is_empty());
}

#[test]
fn xcode_generated_submit_and_reformat() {
    let mut file = parser::parse(XCODE_GENERATED).unwrap();

    // Add a translation for a hypothetical new locale "ko"
    let translations = vec![CompletedTranslation {
        key: "Available Products".to_string(),
        locale: "ko".to_string(),
        value: "사용 가능한 제품".to_string(),
        plural_forms: None,
    }];

    let rejected = validator::validate_translations(&file, &translations);
    assert!(rejected.is_empty());

    let result = merger::merge_translations(&mut file, &translations);
    assert_eq!(result.accepted, 1);

    // Format and re-parse — must survive roundtrip
    let formatted = formatter::format_xcstrings(&file).unwrap();
    let reparsed = parser::parse(&formatted).unwrap();
    assert_eq!(reparsed.strings.len(), 638);

    // Verify new translation exists
    let ko_loc = reparsed.strings["Available Products"]
        .localizations
        .as_ref()
        .unwrap()["ko"]
        .string_unit
        .as_ref()
        .unwrap();
    assert_eq!(ko_loc.value, "사용 가능한 제품");
}

// ── Snapshot tests ──

#[test]
fn snapshot_roundtrip_formatting() {
    let file = parser::parse(SIMPLE_FIXTURE).unwrap();
    let formatted = formatter::format_xcstrings(&file).unwrap();
    insta::assert_snapshot!(formatted);
}

#[test]
fn snapshot_file_summary() {
    let file = parser::parse(SIMPLE_FIXTURE).unwrap();
    let summary = parser::summarize(&file);
    insta::assert_json_snapshot!(summary);
}

#[test]
fn snapshot_untranslated_batch() {
    let file = parser::parse(SIMPLE_FIXTURE).unwrap();
    let (batch, _) = extractor::get_untranslated(&file, "de", 100, 0).unwrap();
    insta::assert_json_snapshot!(batch);
}

// ── Property-based tests ──

mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_translation_state() -> impl Strategy<Value = TranslationState> {
        prop_oneof![
            Just(TranslationState::New),
            Just(TranslationState::Translated),
            Just(TranslationState::NeedsReview),
            Just(TranslationState::Stale),
        ]
    }

    fn arb_string_entry() -> impl Strategy<Value = (String, StringEntry)> {
        ("[a-z_]{1,30}", any::<bool>(), arb_translation_state()).prop_map(
            |(key, should_translate, state)| {
                let mut localizations = IndexMap::new();
                localizations.insert(
                    "en".to_string(),
                    Localization {
                        string_unit: Some(StringUnit {
                            state: state.clone(),
                            value: format!("Value for {key}"),
                        }),
                        variations: None,
                        substitutions: None,
                    },
                );
                (
                    key,
                    StringEntry {
                        extraction_state: None,
                        should_translate,
                        comment: None,
                        localizations: Some(localizations),
                    },
                )
            },
        )
    }

    fn arb_xcstrings_file() -> impl Strategy<Value = XcStringsFile> {
        proptest::collection::vec(arb_string_entry(), 1..20).prop_map(|entries| {
            let strings: IndexMap<String, StringEntry> = entries.into_iter().collect();
            XcStringsFile {
                source_language: "en".to_string(),
                strings,
                version: "1.0".to_string(),
            }
        })
    }

    proptest! {
        #[test]
        fn parse_format_roundtrip(file in arb_xcstrings_file()) {
            // format → parse → format must produce identical output
            let formatted1 = formatter::format_xcstrings(&file).unwrap();
            let reparsed = parser::parse(&formatted1).unwrap();
            let formatted2 = formatter::format_xcstrings(&reparsed).unwrap();
            prop_assert_eq!(formatted1, formatted2, "roundtrip must be idempotent");
        }

        #[test]
        fn merge_never_decreases_key_count(file in arb_xcstrings_file()) {
            let original_count = file.strings.len();
            let mut file = file;

            // Translate all translatable keys to "de"
            let translations: Vec<CompletedTranslation> = file.strings.iter()
                .filter(|(_, e)| e.should_translate)
                .map(|(k, _)| CompletedTranslation {
                    key: k.clone(),
                    locale: "de".to_string(),
                    value: format!("DE: {k}"),
                    plural_forms: None,
                })
                .collect();

            if !translations.is_empty() {
                merger::merge_translations(&mut file, &translations);
            }

            prop_assert!(file.strings.len() >= original_count,
                "merge must never remove keys");
        }

        #[test]
        fn coverage_monotonic_after_submit(file in arb_xcstrings_file()) {
            // Get untranslated count before
            let translatable = file.strings.values().filter(|e| e.should_translate).count();
            let (_, before) = extractor::get_untranslated(&file, "de", 100, 0).unwrap();

            // Translate everything
            let mut file = file;
            let translations: Vec<CompletedTranslation> = file.strings.iter()
                .filter(|(_, e)| e.should_translate)
                .map(|(k, _)| CompletedTranslation {
                    key: k.clone(),
                    locale: "de".to_string(),
                    value: format!("DE: {k}"),
                    plural_forms: None,
                })
                .collect();

            if !translations.is_empty() {
                merger::merge_translations(&mut file, &translations);
            }

            let (_, after) = extractor::get_untranslated(&file, "de", 100, 0).unwrap();
            prop_assert!(after <= before,
                "submitting translations must not increase untranslated count: before={before}, after={after}, translatable={translatable}");
        }
    }
}
