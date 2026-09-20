use super::*;
use crate::model::translation::CompletedTranslation;
use crate::tools::parse::{ParseParams, handle_parse};
use crate::tools::test_helpers::{MIXED_SPECIFIER_FIXTURE, MemoryStore, SIMPLE_FIXTURE};
use std::path::Path;

#[tokio::test]
async fn test_submit_dry_run() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());

    let parse_params = ParseParams {
        file_path: "/test/file.xcstrings".to_string(),
    };
    handle_parse(&store, &cache, parse_params).await.unwrap();

    let params = SubmitTranslationsParams {
        file_path: None,
        translations: vec![CompletedTranslation {
            key: "welcome_message".to_string(),
            locale: "de".to_string(),
            value: "Willkommen in der App".to_string(),
            plural_forms: None,
            substitution_name: None,
            ..Default::default()
        }],
        dry_run: true,
        continue_on_error: true,
    };

    let result = handle_submit_translations(
        &store,
        &cache,
        &write_lock,
        Path::new("/test/glossary.json"),
        captured_params(&store, params),
    )
    .await
    .unwrap();
    assert_eq!(result["dry_run"], true);
    assert_eq!(result["accepted"], 1);

    let content = store
        .get_content(Path::new("/test/file.xcstrings"))
        .unwrap();
    assert!(!content.contains("Willkommen"));
}

#[tokio::test]
async fn test_submit_writes_file() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());

    let parse_params = ParseParams {
        file_path: "/test/file.xcstrings".to_string(),
    };
    handle_parse(&store, &cache, parse_params).await.unwrap();

    let params = SubmitTranslationsParams {
        file_path: None,
        translations: vec![CompletedTranslation {
            key: "welcome_message".to_string(),
            locale: "de".to_string(),
            value: "Willkommen in der App".to_string(),
            plural_forms: None,
            substitution_name: None,
            ..Default::default()
        }],
        dry_run: false,
        continue_on_error: true,
    };

    let result = handle_submit_translations(
        &store,
        &cache,
        &write_lock,
        Path::new("/test/glossary.json"),
        captured_params(&store, params),
    )
    .await
    .unwrap();
    assert_eq!(result["accepted"], 1);
    assert_eq!(result["dry_run"], false);

    let content = store
        .get_content(Path::new("/test/file.xcstrings"))
        .unwrap();
    assert!(content.contains("Willkommen"));
}

#[tokio::test]
async fn test_submit_rejects_invalid_specifier() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", MIXED_SPECIFIER_FIXTURE);
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());

    let parse_params = ParseParams {
        file_path: "/test/file.xcstrings".to_string(),
    };
    handle_parse(&store, &cache, parse_params).await.unwrap();

    let params = SubmitTranslationsParams {
        file_path: None,
        translations: vec![CompletedTranslation {
            key: "greeting".to_string(),
            locale: "de".to_string(),
            value: "Hallo".to_string(),
            plural_forms: None,
            substitution_name: None,
            ..Default::default()
        }],
        dry_run: false,
        continue_on_error: true,
    };

    let result = handle_submit_translations(
        &store,
        &cache,
        &write_lock,
        Path::new("/test/glossary.json"),
        captured_params(&store, params),
    )
    .await
    .unwrap();
    assert_eq!(result["accepted"], 0);
    assert!(!result["rejected"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_submit_no_active_file() {
    let store = MemoryStore::new();
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());

    let params = SubmitTranslationsParams {
        file_path: None,
        translations: vec![],
        dry_run: false,
        continue_on_error: true,
    };
    let result = handle_submit_translations(
        &store,
        &cache,
        &write_lock,
        Path::new("/test/glossary.json"),
        captured_params(&store, params),
    )
    .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_continue_on_error_false_rejects_all() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", MIXED_SPECIFIER_FIXTURE);
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());

    let parse_params = ParseParams {
        file_path: "/test/file.xcstrings".to_string(),
    };
    handle_parse(&store, &cache, parse_params).await.unwrap();

    let params = SubmitTranslationsParams {
        file_path: None,
        translations: vec![
            CompletedTranslation {
                key: "greeting".to_string(),
                locale: "de".to_string(),
                // Missing %@ — should be rejected
                value: "Hallo".to_string(),
                plural_forms: None,
                substitution_name: None,
                ..Default::default()
            },
            CompletedTranslation {
                key: "farewell".to_string(),
                locale: "de".to_string(),
                value: "Tschuess".to_string(),
                plural_forms: None,
                substitution_name: None,
                ..Default::default()
            },
        ],
        dry_run: false,
        continue_on_error: false,
    };

    let result = handle_submit_translations(
        &store,
        &cache,
        &write_lock,
        Path::new("/test/glossary.json"),
        captured_params(&store, params),
    )
    .await
    .unwrap();
    assert_eq!(result["accepted"], 0);
    // All should be rejected (both greeting and farewell)
    assert_eq!(result["rejected"].as_array().unwrap().len(), 2);

    // File should NOT have been written
    let content = store
        .get_content(Path::new("/test/file.xcstrings"))
        .unwrap();
    assert!(!content.contains("Tschuess"));
}

#[tokio::test]
async fn test_continue_on_error_true_writes_valid() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", MIXED_SPECIFIER_FIXTURE);
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());

    let parse_params = ParseParams {
        file_path: "/test/file.xcstrings".to_string(),
    };
    handle_parse(&store, &cache, parse_params).await.unwrap();

    let params = SubmitTranslationsParams {
        file_path: None,
        translations: vec![
            CompletedTranslation {
                key: "greeting".to_string(),
                locale: "de".to_string(),
                value: "Hallo".to_string(),
                plural_forms: None,
                substitution_name: None,
                ..Default::default()
            },
            CompletedTranslation {
                key: "farewell".to_string(),
                locale: "de".to_string(),
                value: "Tschuess".to_string(),
                plural_forms: None,
                substitution_name: None,
                ..Default::default()
            },
        ],
        dry_run: false,
        continue_on_error: true,
    };

    let result = handle_submit_translations(
        &store,
        &cache,
        &write_lock,
        Path::new("/test/glossary.json"),
        captured_params(&store, params),
    )
    .await
    .unwrap();
    // "farewell" accepted, "greeting" rejected (missing %@)
    assert_eq!(result["accepted"], 1);
    assert!(!result["rejected"].as_array().unwrap().is_empty());

    // File should have farewell written
    let content = store
        .get_content(Path::new("/test/file.xcstrings"))
        .unwrap();
    assert!(content.contains("Tschuess"));
}

#[tokio::test]
async fn test_continue_on_error_default_is_true() {
    // Test that deserialization defaults to true
    let json = r#"{
            "translations": [],
            "dry_run": true
        }"#;
    let params: SubmitTranslationsParams = serde_json::from_str(json).unwrap();
    assert!(params.continue_on_error);
}

#[tokio::test]
async fn test_accepted_keys_returned() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());

    let parse_params = ParseParams {
        file_path: "/test/file.xcstrings".to_string(),
    };
    handle_parse(&store, &cache, parse_params).await.unwrap();

    let params = SubmitTranslationsParams {
        file_path: None,
        translations: vec![CompletedTranslation {
            key: "welcome_message".to_string(),
            locale: "de".to_string(),
            value: "Willkommen in der App".to_string(),
            plural_forms: None,
            substitution_name: None,
            ..Default::default()
        }],
        dry_run: false,
        continue_on_error: true,
    };

    let result = handle_submit_translations(
        &store,
        &cache,
        &write_lock,
        Path::new("/test/glossary.json"),
        captured_params(&store, params),
    )
    .await
    .unwrap();
    let accepted_keys = result["accepted_keys"].as_array().unwrap();
    assert_eq!(accepted_keys.len(), 1);
    assert_eq!(accepted_keys[0], "welcome_message");
}
