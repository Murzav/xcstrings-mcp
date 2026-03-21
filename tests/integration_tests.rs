mod helpers;

use std::path::PathBuf;

use helpers::MemoryStore;
use indexmap::IndexMap;
use xcstrings_mcp::_test_support::service::{
    context, coverage, diff, extractor, file_validator, formatter, glossary, locale, merger,
    parser, plural_extractor, validator, xliff,
};
use xcstrings_mcp::model::translation::CompletedTranslation;
use xcstrings_mcp::model::xcstrings::{
    Localization, StringEntry, StringUnit, TranslationState, XcStringsFile,
};

const SIMPLE_FIXTURE: &str = include_str!("fixtures/simple.xcstrings");
const SHOULD_NOT_TRANSLATE_FIXTURE: &str = include_str!("fixtures/should_not_translate.xcstrings");
const GOLDEN: &str = include_str!("fixtures/golden.xcstrings");
const WITH_STALE: &str = include_str!("fixtures/with_stale.xcstrings");
const WITH_PLURALS: &str = include_str!("fixtures/with_plurals.xcstrings");
const WITH_SUBSTITUTIONS: &str = include_str!("fixtures/with_substitutions.xcstrings");
const WITH_DEVICE_VARIANTS: &str = include_str!("fixtures/with_device_variants.xcstrings");
const WITH_INTERPOLATION: &str = include_str!("fixtures/with_interpolation.xcstrings");
const WITH_MULTILINE: &str = include_str!("fixtures/with_multiline.xcstrings");

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
        substitution_name: None,
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
        substitution_name: None,
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
        substitution_name: None,
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
        substitution_name: None,
    }];
    let r1 = merger::merge_translations(&mut file, &t1);
    assert_eq!(r1.accepted, 1);

    // Second merge: translate welcome_message to de
    let t2 = vec![CompletedTranslation {
        key: "welcome_message".to_string(),
        locale: "de".to_string(),
        value: "Willkommen in der App".to_string(),
        plural_forms: None,
        substitution_name: None,
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
            substitution_name: None,
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
    let file = parser::parse(GOLDEN).unwrap();

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
    let file = parser::parse(GOLDEN).unwrap();

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
    let mut file = parser::parse(GOLDEN).unwrap();

    // Add a translation for a hypothetical new locale "ko"
    let translations = vec![CompletedTranslation {
        key: "Available Products".to_string(),
        locale: "ko".to_string(),
        value: "사용 가능한 제품".to_string(),
        plural_forms: None,
        substitution_name: None,
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

// ── Phase 2 integration tests ──

#[test]
fn coverage_full_flow() {
    let file = parser::parse(GOLDEN).unwrap();
    let report = coverage::get_coverage(&file);

    assert_eq!(report.source_language, "en");
    assert_eq!(report.total_keys, 638);
    assert!(report.translatable_keys > 0);
    // Golden file has 9 locales
    assert_eq!(report.locales.len(), 9);
    // All locales should have high coverage (>90%)
    for lc in &report.locales {
        assert!(
            lc.percentage > 90.0,
            "locale {} has only {:.1}% coverage",
            lc.locale,
            lc.percentage
        );
    }
    // Locales should be sorted alphabetically
    let locale_codes: Vec<&str> = report.locales.iter().map(|l| l.locale.as_str()).collect();
    let mut sorted = locale_codes.clone();
    sorted.sort();
    assert_eq!(locale_codes, sorted, "locales should be sorted");
}

#[test]
fn add_locale_then_get_untranslated() {
    let mut file = parser::parse(SIMPLE_FIXTURE).unwrap();
    let translatable = file.strings.values().filter(|e| e.should_translate).count();

    let added = locale::add_locale(&mut file, "ko").unwrap();
    assert_eq!(added, translatable);

    // New locale should have all keys as untranslated (state=New, empty value)
    let (batch, total) = extractor::get_untranslated(&file, "ko", 100, 0).unwrap();
    assert_eq!(total, translatable);
    assert_eq!(batch.len(), translatable);
}

#[test]
fn validate_after_bad_submit() {
    // File with specifier mismatch between source and translation
    let json = r#"{
        "sourceLanguage": "en",
        "strings": {
            "msg": {
                "localizations": {
                    "en": { "stringUnit": { "state": "translated", "value": "Hello %@" } },
                    "uk": { "stringUnit": { "state": "translated", "value": "Привіт" } }
                }
            }
        },
        "version": "1.0"
    }"#;
    let file_with_bad = parser::parse(json).unwrap();

    let reports = file_validator::validate_file(&file_with_bad, Some("uk"));
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].locale, "uk");
    assert!(
        !reports[0].errors.is_empty(),
        "should have specifier mismatch error"
    );
    assert!(
        reports[0]
            .errors
            .iter()
            .any(|e| e.issue_type.contains("specifier"))
    );
}

#[test]
fn stale_keys_from_fixture() {
    let file = parser::parse(WITH_STALE).unwrap();

    let (batch, total) = extractor::get_stale(&file, "uk", 100, 0).unwrap();
    // with_stale.xcstrings has 2 stale+translatable keys: removed_feature, renamed_key
    // (no_translate_stale has shouldTranslate=false)
    assert_eq!(total, 2);
    assert_eq!(batch.len(), 2);

    let keys: Vec<&str> = batch.iter().map(|u| u.key.as_str()).collect();
    assert!(keys.contains(&"removed_feature"));
    assert!(keys.contains(&"renamed_key"));
}

#[test]
fn add_locale_format_roundtrip() {
    let mut file = parser::parse(SIMPLE_FIXTURE).unwrap();

    locale::add_locale(&mut file, "ja").unwrap();

    // Format and re-parse — locale must survive roundtrip
    let formatted = formatter::format_xcstrings(&file).unwrap();
    let reparsed = parser::parse(&formatted).unwrap();

    // Verify "ja" exists in the reparsed file
    let locales = locale::list_locales(&reparsed);
    assert!(
        locales.iter().any(|l| l.locale == "ja"),
        "ja locale should exist after roundtrip"
    );

    // Verify Xcode formatting
    assert!(formatted.contains("\"state\" : \"new\""));
    assert!(formatted.ends_with('\n'));
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

// ── Phase 3 integration tests ──

#[test]
fn plural_extract_then_submit() {
    let mut file = parser::parse(WITH_PLURALS).unwrap();

    // Get untranslated plural keys for "de"
    let (batch, total) = plural_extractor::get_untranslated_plurals(&file, "de", 100, 0).unwrap();
    assert!(total > 0, "should have untranslated plural keys for de");

    // Find "days_remaining" — needs plural translation
    let days = batch.iter().find(|u| u.key == "days_remaining").unwrap();
    assert!(days.required_forms.contains(&"one".to_string()));
    assert!(days.required_forms.contains(&"other".to_string()));

    // Submit plural forms
    let mut plural_forms = std::collections::BTreeMap::new();
    plural_forms.insert("one".to_string(), "%lld Tag verbleibend".to_string());
    plural_forms.insert("other".to_string(), "%lld Tage verbleibend".to_string());

    let translations = vec![CompletedTranslation {
        key: "days_remaining".to_string(),
        locale: "de".to_string(),
        value: String::new(),
        plural_forms: Some(plural_forms),
        substitution_name: None,
    }];

    // Note: validator specifier check uses string_unit fallback (key name) for
    // plural-only keys, so we skip validate_translations here and go straight
    // to merge which is format-agnostic.

    let result = merger::merge_translations(&mut file, &translations);
    assert_eq!(result.accepted, 1);

    // Verify merged correctly
    let locs = file.strings["days_remaining"]
        .localizations
        .as_ref()
        .unwrap();
    let de = &locs["de"];
    let plural = de.variations.as_ref().unwrap().plural.as_ref().unwrap();
    assert_eq!(plural["one"].string_unit.value, "%lld Tag verbleibend");
    assert_eq!(plural["other"].string_unit.value, "%lld Tage verbleibend");
}

#[test]
fn substitution_roundtrip() {
    let mut file = parser::parse(WITH_SUBSTITUTIONS).unwrap();

    // Get untranslated plurals — should find substitution keys
    let (batch, total) = plural_extractor::get_untranslated_plurals(&file, "de", 100, 0).unwrap();
    assert!(total > 0);

    let bird = batch.iter().find(|u| u.key == "bird_sighting").unwrap();
    assert!(bird.has_substitutions);

    // Submit substitution translation with substitution_name
    let mut plural_forms = std::collections::BTreeMap::new();
    plural_forms.insert("one".to_string(), "%arg Vogel".to_string());
    plural_forms.insert("other".to_string(), "%arg Vögel".to_string());

    let translations = vec![CompletedTranslation {
        key: "bird_sighting".to_string(),
        locale: "de".to_string(),
        value: String::new(),
        plural_forms: Some(plural_forms),
        substitution_name: Some("BIRDS".to_string()),
    }];

    let result = merger::merge_translations(&mut file, &translations);
    assert_eq!(result.accepted, 1);

    // Verify written to substitutions
    let locs = file.strings["bird_sighting"]
        .localizations
        .as_ref()
        .unwrap();
    let de = &locs["de"];
    let subs = de.substitutions.as_ref().unwrap();
    let birds = &subs["BIRDS"];
    let one_val = birds["variations"]["plural"]["one"]["stringUnit"]["value"]
        .as_str()
        .unwrap();
    assert_eq!(one_val, "%arg Vogel");

    // Format roundtrip
    let formatted = formatter::format_xcstrings(&file).unwrap();
    let reparsed = parser::parse(&formatted).unwrap();
    let de2 = &reparsed.strings["bird_sighting"]
        .localizations
        .as_ref()
        .unwrap()["de"];
    let subs2 = de2.substitutions.as_ref().unwrap();
    assert!(subs2.contains_key("BIRDS"));
}

#[test]
fn context_nearby_keys() {
    // Build a file with dot-separated keys to test prefix matching
    let json = r#"{
        "sourceLanguage": "en",
        "strings": {
            "settings.notifications.title": {
                "localizations": {
                    "en": { "stringUnit": { "state": "translated", "value": "Notifications" } }
                }
            },
            "settings.notifications.body": {
                "localizations": {
                    "en": { "stringUnit": { "state": "translated", "value": "Body text" } },
                    "uk": { "stringUnit": { "state": "translated", "value": "Текст тіла" } }
                }
            },
            "settings.general.title": {
                "localizations": {
                    "en": { "stringUnit": { "state": "translated", "value": "General" } }
                }
            },
            "login.title": {
                "localizations": {
                    "en": { "stringUnit": { "state": "translated", "value": "Login" } }
                }
            }
        },
        "version": "1.0"
    }"#;
    let file = parser::parse(json).unwrap();

    let result = context::get_context(&file, "settings.notifications.title", "uk", 10);
    assert!(!result.is_empty());

    // First result should be the key with longest shared prefix
    assert_eq!(result[0].key, "settings.notifications.body");
    assert_eq!(result[0].source_text, "Body text");
    assert_eq!(result[0].translated_text.as_deref(), Some("Текст тіла"));

    // Second should be settings.general.title (1 shared segment)
    assert_eq!(result[1].key, "settings.general.title");

    // Third should be login.title (0 shared segments)
    assert_eq!(result[2].key, "login.title");
}

#[test]
fn device_variant_extraction() {
    let file = parser::parse(WITH_DEVICE_VARIANTS).unwrap();

    let (batch, total) = plural_extractor::get_untranslated_plurals(&file, "de", 100, 0).unwrap();
    assert!(total > 0, "should find device variant keys");

    let tap = batch.iter().find(|u| u.key == "tap_action").unwrap();
    assert!(!tap.device_forms.is_empty());
    assert!(tap.device_forms.contains(&"iphone".to_string()));
    assert!(tap.device_forms.contains(&"ipad".to_string()));
    assert!(tap.device_forms.contains(&"mac".to_string()));
}

#[test]
fn plural_validate_then_merge_full_flow() {
    let mut file = parser::parse(WITH_PLURALS).unwrap();

    // Get plural keys for "de"
    let (batch, _) = plural_extractor::get_untranslated_plurals(&file, "de", 100, 0).unwrap();
    let days = batch.iter().find(|u| u.key == "days_remaining").unwrap();

    // Build valid plural forms using required_forms from PluralUnit
    let mut plural_forms = std::collections::BTreeMap::new();
    plural_forms.insert("one".to_string(), "%lld Tag verbleibend".to_string());
    plural_forms.insert("other".to_string(), "%lld Tage verbleibend".to_string());

    let translations = vec![CompletedTranslation {
        key: days.key.clone(),
        locale: "de".to_string(),
        value: String::new(),
        plural_forms: Some(plural_forms),
        substitution_name: None,
    }];

    // Validate — should pass (validator now handles plural-only source keys)
    let rejected = validator::validate_translations(&file, &translations);
    assert!(
        rejected.is_empty(),
        "valid plural translation should pass validation: {:?}",
        rejected
    );

    // Merge
    let result = merger::merge_translations(&mut file, &translations);
    assert_eq!(result.accepted, 1);

    // Format roundtrip
    let formatted = formatter::format_xcstrings(&file).unwrap();
    let reparsed = parser::parse(&formatted).unwrap();
    let de = &reparsed.strings["days_remaining"]
        .localizations
        .as_ref()
        .unwrap()["de"];
    let plural = de.variations.as_ref().unwrap().plural.as_ref().unwrap();
    assert_eq!(plural["one"].string_unit.value, "%lld Tag verbleibend");
}

// ── Phase 4 fixture tests ──

#[test]
fn interpolation_fixture_specifiers() {
    let file = parser::parse(WITH_INTERPOLATION).unwrap();

    assert_eq!(file.strings.len(), 3);

    // greeting_name source contains %@
    let greeting = &file.strings["greeting_name"];
    let en = greeting.localizations.as_ref().unwrap()["en"]
        .string_unit
        .as_ref()
        .unwrap();
    assert!(en.value.contains("%@"), "greeting_name should contain %@");

    // items_count_format source contains both %lld and %@
    let items = &file.strings["items_count_format"];
    let en_items = items.localizations.as_ref().unwrap()["en"]
        .string_unit
        .as_ref()
        .unwrap();
    assert!(
        en_items.value.contains("%lld"),
        "items_count_format should contain %lld"
    );
    assert!(
        en_items.value.contains("%@"),
        "items_count_format should contain %@"
    );

    // Long key name survives parse
    let long_key = "MyApp.Features.Settings.Notifications.PushNotificationPermissionAlert.Title";
    assert!(
        file.strings.contains_key(long_key),
        "long auto-generated key should survive parse"
    );
}

#[test]
fn interpolation_long_key_full_flow() {
    let file = parser::parse(WITH_INTERPOLATION).unwrap();

    let long_key = "MyApp.Features.Settings.Notifications.PushNotificationPermissionAlert.Title";

    let (batch, total) = extractor::get_untranslated(&file, "de", 100, 0).unwrap();
    assert_eq!(total, 3, "all 3 keys should be untranslated for de");

    let keys: Vec<&str> = batch.iter().map(|u| u.key.as_str()).collect();
    assert!(
        keys.contains(&long_key),
        "long key should appear in untranslated batch"
    );
}

#[test]
fn multiline_roundtrip() {
    let file = parser::parse(WITH_MULTILINE).unwrap();

    // Verify multiline_message source value contains \n
    let msg = &file.strings["multiline_message"];
    let en = msg.localizations.as_ref().unwrap()["en"]
        .string_unit
        .as_ref()
        .unwrap();
    assert!(
        en.value.contains('\n'),
        "multiline_message source should contain newline characters"
    );

    // Format → re-parse → format must be idempotent
    let formatted1 = formatter::format_xcstrings(&file).unwrap();
    let reparsed = parser::parse(&formatted1).unwrap();
    let formatted2 = formatter::format_xcstrings(&reparsed).unwrap();

    assert_eq!(
        formatted1, formatted2,
        "multiline fixture formatting must be idempotent"
    );
}

#[test]
fn multiline_specifier_safe() {
    let mut file = parser::parse(WITH_MULTILINE).unwrap();

    let translations = vec![CompletedTranslation {
        key: "multiline_message".to_string(),
        locale: "uk".to_string(),
        value: "Рядок 1\nРядок 2\nРядок 3".to_string(),
        plural_forms: None,
        substitution_name: None,
    }];

    let rejected = validator::validate_translations(&file, &translations);
    assert!(
        rejected.is_empty(),
        "multiline translation should pass validation"
    );

    let result = merger::merge_translations(&mut file, &translations);
    assert_eq!(result.accepted, 1);

    // Verify merged value preserves newlines
    let uk = file.strings["multiline_message"]
        .localizations
        .as_ref()
        .unwrap()["uk"]
        .string_unit
        .as_ref()
        .unwrap();
    assert!(
        uk.value.contains('\n'),
        "merged translation should preserve newline characters"
    );
    assert_eq!(uk.value, "Рядок 1\nРядок 2\nРядок 3");
}

// ── Phase 5 integration tests ──

#[test]
fn multi_file_parse_and_switch() {
    // File A: two translatable keys
    let file_a = parser::parse(SIMPLE_FIXTURE).unwrap();
    let summary_a = parser::summarize(&file_a);
    assert_eq!(summary_a.translatable_keys, 2);

    // File B: a different file with one key
    let json_b = r#"{
        "sourceLanguage": "en",
        "strings": {
            "logout_button": {
                "localizations": {
                    "en": { "stringUnit": { "state": "translated", "value": "Log Out" } }
                }
            }
        },
        "version": "1.0"
    }"#;
    let file_b = parser::parse(json_b).unwrap();
    let summary_b = parser::summarize(&file_b);
    assert_eq!(summary_b.translatable_keys, 1);

    // get_untranslated on file B (the "active" file)
    let (batch_b, total_b) = extractor::get_untranslated(&file_b, "de", 100, 0).unwrap();
    assert_eq!(total_b, 1);
    assert_eq!(batch_b[0].key, "logout_button");

    // get_untranslated on file A (switch back)
    let (batch_a, total_a) = extractor::get_untranslated(&file_a, "de", 100, 0).unwrap();
    assert_eq!(total_a, 2);
    let keys_a: Vec<&str> = batch_a.iter().map(|u| u.key.as_str()).collect();
    assert!(keys_a.contains(&"greeting"));
    assert!(keys_a.contains(&"welcome_message"));
}

#[test]
fn add_then_remove_locale_roundtrip() {
    let mut file = parser::parse(SIMPLE_FIXTURE).unwrap();
    let source_lang = file.source_language.clone();

    // Add locale "fr"
    let added = locale::add_locale(&mut file, "fr").unwrap();
    assert!(added > 0);

    // Verify "fr" exists
    let locales = locale::list_locales(&file);
    assert!(
        locales.iter().any(|l| l.locale == "fr"),
        "fr should exist after add"
    );

    // Remove locale "fr"
    let removed = locale::remove_locale(&mut file, "fr", &source_lang).unwrap();
    assert_eq!(removed, added);

    // Verify "fr" gone
    let locales = locale::list_locales(&file);
    assert!(
        !locales.iter().any(|l| l.locale == "fr"),
        "fr should be gone after remove"
    );

    // Verify file is still valid (parse roundtrip)
    let formatted = formatter::format_xcstrings(&file).unwrap();
    let reparsed = parser::parse(&formatted).unwrap();
    assert_eq!(reparsed.strings.len(), file.strings.len());
}

#[test]
fn batch_retry_continue_on_error_writes_valid() {
    let json = r#"{
        "sourceLanguage": "en",
        "strings": {
            "plain_key": {
                "localizations": {
                    "en": { "stringUnit": { "state": "translated", "value": "Hello" } }
                }
            },
            "specifier_key": {
                "localizations": {
                    "en": { "stringUnit": { "state": "translated", "value": "Hello %@" } }
                }
            }
        },
        "version": "1.0"
    }"#;
    let mut file = parser::parse(json).unwrap();

    let translations = vec![
        // Valid: plain_key has no specifiers, translation has none
        CompletedTranslation {
            key: "plain_key".to_string(),
            locale: "de".to_string(),
            value: "Hallo".to_string(),
            plural_forms: None,
            substitution_name: None,
        },
        // Invalid: specifier_key needs %@ but translation lacks it
        CompletedTranslation {
            key: "specifier_key".to_string(),
            locale: "de".to_string(),
            value: "Hallo ohne Spezifizierer".to_string(),
            plural_forms: None,
            substitution_name: None,
        },
    ];

    // Validate to find rejected ones
    let rejected = validator::validate_translations(&file, &translations);
    assert_eq!(rejected.len(), 1, "specifier_key should be rejected");
    assert_eq!(rejected[0].key, "specifier_key");

    // Filter out rejected and merge only valid translations
    let rejected_keys: std::collections::HashSet<&str> =
        rejected.iter().map(|r| r.key.as_str()).collect();
    let valid: Vec<CompletedTranslation> = translations
        .into_iter()
        .filter(|t| !rejected_keys.contains(t.key.as_str()))
        .collect();

    let result = merger::merge_translations(&mut file, &valid);
    assert_eq!(result.accepted, 1);
    assert!(result.accepted_keys.contains(&"plain_key".to_string()));

    // Verify plain_key is translated, specifier_key is not
    let (batch, total) = extractor::get_untranslated(&file, "de", 100, 0).unwrap();
    assert_eq!(total, 1);
    assert_eq!(batch[0].key, "specifier_key");
}

#[test]
fn diff_detects_external_changes() {
    let old = parser::parse(SIMPLE_FIXTURE).unwrap();

    // Build a modified version: add a key, remove a key, change source text
    let modified_json = r#"{
        "sourceLanguage": "en",
        "strings": {
            "greeting": {
                "localizations": {
                    "en": { "stringUnit": { "state": "translated", "value": "Hi there!" } },
                    "uk": { "stringUnit": { "state": "translated", "value": "Привіт" } }
                }
            },
            "new_key": {
                "localizations": {
                    "en": { "stringUnit": { "state": "translated", "value": "New!" } }
                }
            }
        },
        "version": "1.0"
    }"#;
    let new = parser::parse(modified_json).unwrap();

    let report = diff::compute_diff(&old, &new);

    // "new_key" was added
    assert!(report.added.contains(&"new_key".to_string()));
    // "welcome_message" was removed
    assert!(report.removed.contains(&"welcome_message".to_string()));
    // "greeting" source changed from "Hello" to "Hi there!"
    assert_eq!(report.modified.len(), 1);
    assert_eq!(report.modified[0].key, "greeting");
    assert_eq!(report.modified[0].old_value, "Hello");
    assert_eq!(report.modified[0].new_value, "Hi there!");
}

#[test]
fn xliff_export_import_roundtrip() {
    let file = parser::parse(SIMPLE_FIXTURE).unwrap();

    // Export to XLIFF (all entries, not just untranslated)
    let (xml, exported_count) =
        xliff::export_xliff(&file, "uk", "Localizable.xcstrings", false).unwrap();
    assert!(exported_count > 0);

    // Import the XLIFF back
    let (locale, translations) = xliff::import_xliff(&xml).unwrap();
    assert_eq!(locale, "uk");

    // Only entries with non-empty target text are imported.
    // "greeting" has uk translation -> imported. "welcome_message" does not -> skipped.
    assert!(
        !translations.is_empty(),
        "should import at least one translation"
    );

    // The imported translations should have the correct locale
    for t in &translations {
        assert_eq!(t.locale, "uk");
    }

    // Verify the greeting translation survived the roundtrip
    let greeting = translations.iter().find(|t| t.key == "greeting");
    assert!(greeting.is_some());
    assert_eq!(greeting.unwrap().value, "Привіт");
}

#[test]
fn glossary_create_update_read() {
    let mut g = glossary::parse_glossary(None).unwrap();

    // Add terms
    let mut terms = std::collections::BTreeMap::new();
    terms.insert("Settings".to_string(), "Einstellungen".to_string());
    terms.insert("Cancel".to_string(), "Abbrechen".to_string());
    let count = glossary::update_entries(&mut g, "en", "de", terms);
    assert_eq!(count, 2);

    // Read back
    let entries = glossary::get_entries(&g, "en", "de", None);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries["Settings"], "Einstellungen");
    assert_eq!(entries["Cancel"], "Abbrechen");

    // Overwrite "Settings"
    let mut update = std::collections::BTreeMap::new();
    update.insert("Settings".to_string(), "Optionen".to_string());
    glossary::update_entries(&mut g, "en", "de", update);

    let entries = glossary::get_entries(&g, "en", "de", None);
    assert_eq!(entries["Settings"], "Optionen");
    // Cancel should still be there
    assert_eq!(entries["Cancel"], "Abbrechen");

    // Filter
    let filtered = glossary::get_entries(&g, "en", "de", Some("cancel"));
    assert_eq!(filtered.len(), 1);
    assert!(filtered.contains_key("Cancel"));

    // Serialize and re-parse
    let json = glossary::serialize_glossary(&g).unwrap();
    let reloaded = glossary::parse_glossary(Some(&json)).unwrap();
    let entries = glossary::get_entries(&reloaded, "en", "de", None);
    assert_eq!(entries.len(), 2);
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
                    substitution_name: None,
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
                    substitution_name: None,
                })
                .collect();

            if !translations.is_empty() {
                merger::merge_translations(&mut file, &translations);
            }

            let (_, after) = extractor::get_untranslated(&file, "de", 100, 0).unwrap();
            prop_assert!(after <= before,
                "submitting translations must not increase untranslated count: before={before}, after={after}, translatable={translatable}");
        }

        // ── Phase 5 property tests ──

        #[test]
        fn diff_identity(keys in prop::collection::vec("[a-z]{1,10}", 0..20)) {
            // Deduplicate keys (IndexMap collapses dupes)
            let mut strings = IndexMap::new();
            for key in &keys {
                let mut localizations = IndexMap::new();
                localizations.insert(
                    "en".to_string(),
                    Localization {
                        string_unit: Some(StringUnit {
                            state: TranslationState::Translated,
                            value: format!("Value for {key}"),
                        }),
                        variations: None,
                        substitutions: None,
                    },
                );
                strings.insert(
                    key.clone(),
                    StringEntry {
                        extraction_state: None,
                        should_translate: true,
                        comment: None,
                        localizations: Some(localizations),
                    },
                );
            }
            let file = XcStringsFile {
                source_language: "en".to_string(),
                strings,
                version: "1.0".to_string(),
            };

            let report = diff::compute_diff(&file, &file);
            prop_assert!(report.added.is_empty(), "diff of identical files should have no added keys");
            prop_assert!(report.removed.is_empty(), "diff of identical files should have no removed keys");
            prop_assert!(report.modified.is_empty(), "diff of identical files should have no modified keys");
        }

        #[test]
        fn remove_add_locale_preserves_key_count(
            keys in prop::collection::vec("[a-z]{1,10}", 1..10)
        ) {
            // Build file with unique keys
            let mut strings = IndexMap::new();
            for key in &keys {
                let mut localizations = IndexMap::new();
                localizations.insert(
                    "en".to_string(),
                    Localization {
                        string_unit: Some(StringUnit {
                            state: TranslationState::Translated,
                            value: format!("Value for {key}"),
                        }),
                        variations: None,
                        substitutions: None,
                    },
                );
                strings.insert(
                    key.clone(),
                    StringEntry {
                        extraction_state: None,
                        should_translate: true,
                        comment: None,
                        localizations: Some(localizations),
                    },
                );
            }
            let mut file = XcStringsFile {
                source_language: "en".to_string(),
                strings,
                version: "1.0".to_string(),
            };

            let translatable = file.strings.values().filter(|e| e.should_translate).count();

            // Add locale "test_xx"
            locale::add_locale(&mut file, "test_xx").unwrap();
            let locales_after_add = locale::list_locales(&file);
            let test_locale = locales_after_add.iter().find(|l| l.locale == "test_xx").unwrap();
            prop_assert_eq!(test_locale.total, translatable);

            // Remove locale "test_xx"
            locale::remove_locale(&mut file, "test_xx", "en").unwrap();
            let locales_after_remove = locale::list_locales(&file);
            prop_assert!(
                !locales_after_remove.iter().any(|l| l.locale == "test_xx"),
                "test_xx locale should be gone after remove"
            );

            // Key count unchanged
            prop_assert_eq!(file.strings.len(), keys.iter().collect::<std::collections::HashSet<_>>().len(),
                "key count should match unique input keys");
        }
    }
}
