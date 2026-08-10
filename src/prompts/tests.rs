use super::*;
use rmcp::model::{ContentBlock, TextContent};
use std::path::PathBuf;
use std::sync::Arc;

fn make_server() -> XcStringsMcpServer {
    let store = Arc::new(crate::tools::test_helpers::MemoryStore::new());
    XcStringsMcpServer::new(store, PathBuf::from("/tmp/g.json"))
}

#[test]
fn translate_batch_returns_content() {
    let server = make_server();
    let result = server
        .translate_batch(Parameters(TranslateBatchParams {
            locale: "uk".into(),
            count: Some("10".into()),
        }))
        .unwrap();
    let ContentBlock::Text(TextContent { ref text, .. }) = result.messages[0].content else {
        panic!("expected text")
    };
    assert!(text.contains("uk"));
    assert!(text.contains("10"));
    assert!(text.contains("get_untranslated"));
    assert!(text.contains("definite Foundation format arguments"));
    assert!(text.contains("warnings[]"));
    assert!(text.contains("positional reordering"));
}

#[test]
fn translate_batch_default_count() {
    let server = make_server();
    let result = server
        .translate_batch(Parameters(TranslateBatchParams {
            locale: "de".into(),
            count: None,
        }))
        .unwrap();
    let ContentBlock::Text(TextContent { ref text, .. }) = result.messages[0].content else {
        panic!("expected text")
    };
    assert!(text.contains("20"));
}

#[test]
fn review_translations_returns_content() {
    let server = make_server();
    let result = server
        .review_translations(Parameters(ReviewTranslationsParams {
            locale: "fr".into(),
        }))
        .unwrap();
    let ContentBlock::Text(TextContent { ref text, .. }) = result.messages[0].content else {
        panic!("expected text")
    };
    assert!(text.contains("fr"));
    assert!(text.contains("validate_translations"));
    assert!(text.contains("blocking errors"));
    assert!(text.contains("non-blocking warnings"));
}

#[test]
fn full_translate_returns_content() {
    let server = make_server();
    let result = server
        .full_translate(Parameters(FullTranslateParams {
            locale: "ja".into(),
            file_path: "/App/L.xcstrings".into(),
        }))
        .unwrap();
    let ContentBlock::Text(TextContent { ref text, .. }) = result.messages[0].content else {
        panic!("expected text")
    };
    assert!(text.contains("ja"));
    assert!(text.contains("/App/L.xcstrings"));
}

#[test]
fn localization_audit_returns_content() {
    let server = make_server();
    let result = server
        .localization_audit(Parameters(LocalizationAuditParams {
            locale: "uk".into(),
        }))
        .unwrap();
    let ContentBlock::Text(TextContent { ref text, .. }) = result.messages[0].content else {
        panic!("expected text")
    };
    assert!(text.contains("uk"));
    assert!(text.contains("get_coverage"));
    assert!(text.contains("get_stale"));
    assert!(text.contains("ambiguous percent-in-prose"));
}

#[test]
fn fix_validation_errors_returns_content() {
    let server = make_server();
    let result = server
        .fix_validation_errors(Parameters(FixValidationErrorsParams {
            locale: "de".into(),
        }))
        .unwrap();
    let ContentBlock::Text(TextContent { ref text, .. }) = result.messages[0].content else {
        panic!("expected text")
    };
    assert!(text.contains("de"));
    assert!(text.contains("blocking format errors"));
    assert!(text.contains("warnings[]"));
}

#[test]
fn extract_strings_returns_content() {
    let server = make_server();
    let result = server
        .extract_strings(Parameters(ExtractStringsParams {
            source_language: "en".into(),
            file_path: "/App/L.xcstrings".into(),
        }))
        .unwrap();
    let ContentBlock::Text(TextContent { ref text, .. }) = result.messages[0].content else {
        panic!("expected text")
    };
    assert!(text.contains("create_xcstrings"));
    assert!(text.contains("add_keys"));
    assert!(text.contains("String(localized"));
}

#[test]
fn add_language_with_file_path() {
    let server = make_server();
    let result = server
        .add_language(Parameters(AddLanguageParams {
            locale: "ko".into(),
            file_path: Some("/App/L.xcstrings".into()),
        }))
        .unwrap();
    let ContentBlock::Text(TextContent { ref text, .. }) = result.messages[0].content else {
        panic!("expected text")
    };
    assert!(text.contains("ko"));
    assert!(text.contains("/App/L.xcstrings"));
}

#[test]
fn cleanup_stale_returns_content() {
    let server = make_server();
    let result = server
        .cleanup_stale(Parameters(CleanupStaleParams {
            file_path: Some("/App/L.xcstrings".into()),
        }))
        .unwrap();
    let ContentBlock::Text(TextContent { ref text, .. }) = result.messages[0].content else {
        panic!("expected text")
    };
    assert!(text.contains("get_stale"));
    assert!(text.contains("delete_keys"));
    assert!(text.contains("/App/L.xcstrings"));
}

#[test]
fn cleanup_stale_without_file_path() {
    let server = make_server();
    let result = server
        .cleanup_stale(Parameters(CleanupStaleParams { file_path: None }))
        .unwrap();
    let ContentBlock::Text(TextContent { ref text, .. }) = result.messages[0].content else {
        panic!("expected text")
    };
    assert!(text.contains("Ensure a file is already parsed"));
}

#[test]
fn add_language_without_file_path() {
    let server = make_server();
    let result = server
        .add_language(Parameters(AddLanguageParams {
            locale: "zh".into(),
            file_path: None,
        }))
        .unwrap();
    let ContentBlock::Text(TextContent { ref text, .. }) = result.messages[0].content else {
        panic!("expected text")
    };
    assert!(text.contains("zh"));
    assert!(text.contains("Ensure a file is already parsed"));
}
