use std::collections::BTreeMap;
use std::path::Path;

use indexmap::IndexMap;
use tokio::sync::Mutex;

use super::FileCache;
use super::parse::{ParseParams, handle_parse};
use super::test_helpers::MemoryStore;
use super::translate::{SubmitTranslationsParams, handle_submit_translations};
use super::xliff::{ImportXliffParams, handle_import_xliff};
use crate::model::translation::CompletedTranslation;
use crate::model::xcstrings::{
    Localization, StringEntry, StringUnit, TranslationState, XcStringsFile,
};
use crate::service::{formatter, parser};

#[path = "../../tests/support/modifier_oracle.rs"]
mod modifier_oracle;

fn prose_catalog() -> String {
    simple_catalog("storage", "100% Local Storage")
}

fn simple_catalog(key: &str, source_value: &str) -> String {
    let source = Localization {
        string_unit: Some(StringUnit {
            state: TranslationState::Translated,
            value: source_value.to_string(),
        }),
        variations: None,
        substitutions: None,
    };
    let entry = StringEntry {
        extraction_state: None,
        should_translate: true,
        comment: None,
        localizations: Some(IndexMap::from([("en".to_string(), source)])),
    };
    formatter::format_xcstrings(&XcStringsFile {
        source_language: "en".to_string(),
        strings: IndexMap::from([(key.to_string(), entry)]),
        version: "1.0".to_string(),
    })
    .unwrap()
}

const PREFIXED_XLIFF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ns0:xliff xmlns:ns0="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
  <ns0:file source-language="en" target-language="de" original="file.xcstrings" datatype="plaintext">
    <ns0:body><ns0:trans-unit id="greeting"><ns0:source>Hello</ns0:source>
      <ns0:target state="translated">Hallo</ns0:target>
    </ns0:trans-unit></ns0:body>
  </ns0:file>
</ns0:xliff>"#;

const OFFICIAL_ROOT_UNQUALIFIED_CHILD_XLIFF: &str = r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
  <file target-language="de"><body><trans-unit id="greeting"><source>Hello</source>
    <target>Mixed Hallo</target>
  </trans-unit></body></file>
</x:xliff>"#;

const LEGACY_ROOT_QUALIFIED_CHILD_XLIFF: &str = r#"<xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
  <x:file target-language="de"><x:body><x:trans-unit id="greeting"><x:source>Hello</x:source>
    <x:target>Mixed Hallo</x:target>
  </x:trans-unit></x:body></x:file>
</xliff>"#;

const DUPLICATE_NAMESPACE_XLIFF: &str = r#"<x:xliff xmlns:x="urn:example:wrong" xmlns:x="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
  <x:file target-language="de"/>
</x:xliff>"#;

async fn assert_mcp_xliff_rejected_without_write(xliff: &str, expected_error: &str) {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", &simple_catalog("greeting", "Hello"));
    store.add_file("/test/input.xliff", xliff);
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        },
    )
    .await
    .unwrap();
    let before = store
        .get_content(Path::new("/test/file.xcstrings"))
        .unwrap();

    let error = handle_import_xliff(
        &store,
        &cache,
        &write_lock,
        ImportXliffParams {
            file_path: None,
            xliff_path: "/test/input.xliff".to_string(),
            dry_run: false,
        },
    )
    .await
    .unwrap_err();

    assert_eq!(error.to_string(), expected_error);
    assert_eq!(
        store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap(),
        before
    );
}

async fn assert_mcp_xliff_applied(xliff: &str, expected_value: &str) {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", &simple_catalog("greeting", "Hello"));
    store.add_file("/test/input.xliff", xliff);
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        },
    )
    .await
    .unwrap();
    let before = store
        .get_content(Path::new("/test/file.xcstrings"))
        .unwrap();

    let dry_run = handle_import_xliff(
        &store,
        &cache,
        &write_lock,
        ImportXliffParams {
            file_path: None,
            xliff_path: "/test/input.xliff".to_string(),
            dry_run: true,
        },
    )
    .await
    .unwrap();

    assert_eq!(dry_run["accepted"], 1);
    assert_eq!(dry_run["accepted_keys"], serde_json::json!(["greeting"]));
    assert_eq!(dry_run["rejected"], serde_json::json!([]));
    assert_eq!(dry_run["dry_run"], true);
    assert!(dry_run["warnings"].is_null());
    assert_eq!(
        store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap(),
        before
    );

    let applied = handle_import_xliff(
        &store,
        &cache,
        &write_lock,
        ImportXliffParams {
            file_path: None,
            xliff_path: "/test/input.xliff".to_string(),
            dry_run: false,
        },
    )
    .await
    .unwrap();
    let updated = parser::parse(
        &store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap(),
    )
    .unwrap();
    let value = &updated.strings["greeting"].localizations.as_ref().unwrap()["de"]
        .string_unit
        .as_ref()
        .unwrap()
        .value;

    assert_eq!(applied["accepted"], 1);
    assert_eq!(applied["accepted_keys"], serde_json::json!(["greeting"]));
    assert_eq!(applied["rejected"], serde_json::json!([]));
    assert_eq!(applied["dry_run"], false);
    assert!(applied["warnings"].is_null());
    assert_eq!(value, expected_value);
}

#[tokio::test]
async fn mcp_xliff_import_accepts_prefix_bound_elements_for_dry_run_and_apply() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", &simple_catalog("greeting", "Hello"));
    store.add_file("/test/input.xliff", PREFIXED_XLIFF);
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        },
    )
    .await
    .unwrap();
    let before = store
        .get_content(Path::new("/test/file.xcstrings"))
        .unwrap();

    let dry_run = handle_import_xliff(
        &store,
        &cache,
        &write_lock,
        ImportXliffParams {
            file_path: None,
            xliff_path: "/test/input.xliff".to_string(),
            dry_run: true,
        },
    )
    .await
    .unwrap();

    assert_eq!(dry_run["accepted"], 1);
    assert_eq!(dry_run["accepted_keys"], serde_json::json!(["greeting"]));
    assert_eq!(dry_run["rejected"], serde_json::json!([]));
    assert_eq!(dry_run["dry_run"], true);
    assert!(dry_run["warnings"].is_null());
    assert_eq!(
        store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap(),
        before
    );

    let applied = handle_import_xliff(
        &store,
        &cache,
        &write_lock,
        ImportXliffParams {
            file_path: None,
            xliff_path: "/test/input.xliff".to_string(),
            dry_run: false,
        },
    )
    .await
    .unwrap();
    let updated = parser::parse(
        &store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap(),
    )
    .unwrap();
    let value = &updated.strings["greeting"].localizations.as_ref().unwrap()["de"]
        .string_unit
        .as_ref()
        .unwrap()
        .value;

    assert_eq!(applied["accepted"], 1);
    assert_eq!(applied["accepted_keys"], serde_json::json!(["greeting"]));
    assert_eq!(applied["rejected"], serde_json::json!([]));
    assert_eq!(applied["dry_run"], false);
    assert!(applied["warnings"].is_null());
    assert_eq!(value, "Hallo");
}

#[tokio::test]
async fn mcp_xliff_import_rejects_wrong_namespace_without_writing() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", &simple_catalog("greeting", "Hello"));
    store.add_file(
        "/test/input.xliff",
        &PREFIXED_XLIFF.replace("urn:oasis:names:tc:xliff:document:1.2", "urn:example:wrong"),
    );
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        },
    )
    .await
    .unwrap();
    let before = store
        .get_content(Path::new("/test/file.xcstrings"))
        .unwrap();

    let error = handle_import_xliff(
        &store,
        &cache,
        &write_lock,
        ImportXliffParams {
            file_path: None,
            xliff_path: "/test/input.xliff".to_string(),
            dry_run: false,
        },
    )
    .await
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: element <xliff> uses namespace 'urn:example:wrong'; expected 'urn:oasis:names:tc:xliff:document:1.2'"
    );
    assert_eq!(
        store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap(),
        before
    );
}

#[tokio::test]
async fn mcp_xliff_import_rejects_unqualified_child_in_official_document_without_writing() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", &simple_catalog("greeting", "Hello"));
    store.add_file("/test/input.xliff", OFFICIAL_ROOT_UNQUALIFIED_CHILD_XLIFF);
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        },
    )
    .await
    .unwrap();
    let before = store
        .get_content(Path::new("/test/file.xcstrings"))
        .unwrap();

    let error = handle_import_xliff(
        &store,
        &cache,
        &write_lock,
        ImportXliffParams {
            file_path: None,
            xliff_path: "/test/input.xliff".to_string(),
            dry_run: false,
        },
    )
    .await
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: element <file> is unqualified in namespace-qualified XLIFF document; expected 'urn:oasis:names:tc:xliff:document:1.2'"
    );
    assert_eq!(
        store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap(),
        before
    );
}

#[tokio::test]
async fn mcp_xliff_import_rejects_qualified_child_in_legacy_document_without_writing() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", &simple_catalog("greeting", "Hello"));
    store.add_file("/test/input.xliff", LEGACY_ROOT_QUALIFIED_CHILD_XLIFF);
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        },
    )
    .await
    .unwrap();
    let before = store
        .get_content(Path::new("/test/file.xcstrings"))
        .unwrap();

    let error = handle_import_xliff(
        &store,
        &cache,
        &write_lock,
        ImportXliffParams {
            file_path: None,
            xliff_path: "/test/input.xliff".to_string(),
            dry_run: false,
        },
    )
    .await
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: element <file> uses namespace 'urn:oasis:names:tc:xliff:document:1.2' in legacy unqualified XLIFF document; expected no namespace"
    );
    assert_eq!(
        store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap(),
        before
    );
}

#[tokio::test]
async fn mcp_xliff_import_rejects_duplicate_namespace_without_writing() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", &simple_catalog("greeting", "Hello"));
    store.add_file("/test/input.xliff", DUPLICATE_NAMESPACE_XLIFF);
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        },
    )
    .await
    .unwrap();
    let before = store
        .get_content(Path::new("/test/file.xcstrings"))
        .unwrap();

    let error = handle_import_xliff(
        &store,
        &cache,
        &write_lock,
        ImportXliffParams {
            file_path: None,
            xliff_path: "/test/input.xliff".to_string(),
            dry_run: false,
        },
    )
    .await
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "XLIFF parse error: duplicate attribute on <xliff>"
    );
    assert_eq!(
        store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap(),
        before
    );
}

#[tokio::test]
async fn mcp_xliff_import_rejects_foreign_wrapper_without_writing() {
    assert_mcp_xliff_rejected_without_write(
        r#"<wrapper xmlns:x="urn:oasis:names:tc:xliff:document:1.2"><x:xliff version="1.2">
  <x:file target-language="de"><x:body><x:trans-unit id="greeting"><x:source>Hello</x:source>
    <x:target>Wrapped</x:target></x:trans-unit></x:body></x:file>
</x:xliff></wrapper>"#,
        "XLIFF parse error: document root must be <xliff>; found <wrapper>",
    )
    .await;
}

#[tokio::test]
async fn mcp_xliff_import_rejects_nested_xliff_without_writing() {
    assert_mcp_xliff_rejected_without_write(
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" version="1.2"><x:xliff version="1.2">
  <x:file target-language="de"><x:body><x:trans-unit id="greeting"><x:source>Hello</x:source>
    <x:target>Nested</x:target></x:trans-unit></x:body></x:file>
</x:xliff></x:xliff>"#,
        "XLIFF parse error: nested <xliff> element is not allowed",
    )
    .await;
}

#[tokio::test]
async fn mcp_xliff_import_rejects_multiple_roots_without_writing() {
    assert_mcp_xliff_rejected_without_write(
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" version="1.2"/>
<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" version="1.2"><x:file target-language="de"/></x:xliff>"#,
        "XLIFF parse error: element <xliff> appears after </xliff> document root",
    )
    .await;
}

#[tokio::test]
async fn mcp_xliff_import_rejects_empty_root_fragment_without_writing() {
    assert_mcp_xliff_rejected_without_write(
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" version="1.2"/>
<x:file xmlns:x="urn:oasis:names:tc:xliff:document:1.2" target-language="de"/>"#,
        "XLIFF parse error: element <file> appears after </xliff> document root",
    )
    .await;
}

#[tokio::test]
async fn mcp_xliff_import_rejects_missing_root_without_writing() {
    assert_mcp_xliff_rejected_without_write(
        "\n<!-- no document element -->\n",
        "XLIFF parse error: missing <xliff> document root",
    )
    .await;
}

#[tokio::test]
async fn mcp_xliff_import_rejects_duplicate_expanded_attribute_without_writing() {
    assert_mcp_xliff_rejected_without_write(
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" xmlns:a="urn:attr" xmlns:b="urn:attr" version="1.2">
  <x:file target-language="de"><x:body a:custom="first" b:custom="last"/></x:file>
</x:xliff>"#,
        "XLIFF parse error: duplicate expanded attribute '{urn:attr}custom' on <body>",
    )
    .await;
}

#[tokio::test]
async fn mcp_xliff_import_rejects_unbound_attribute_prefix_without_writing() {
    assert_mcp_xliff_rejected_without_write(
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
  <x:file target-language="de"><x:body missing:custom="value"/></x:file>
</x:xliff>"#,
        "XLIFF parse error: attribute <missing:custom> on <body> uses unbound namespace prefix 'missing'",
    )
    .await;
}

#[tokio::test]
async fn mcp_xliff_import_rejects_unbound_non_empty_extension_without_writing() {
    assert_mcp_xliff_rejected_without_write(
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
  <missing:group><x:file target-language="de"><x:body><x:trans-unit id="greeting">
    <x:source>Hello</x:source><x:target>Unbound extension</x:target>
  </x:trans-unit></x:body></x:file></missing:group>
</x:xliff>"#,
        "XLIFF parse error: element <group> uses unbound namespace prefix 'missing'",
    )
    .await;
}

#[tokio::test]
async fn mcp_xliff_import_rejects_unbound_empty_extension_without_writing() {
    assert_mcp_xliff_rejected_without_write(
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" version="1.2">
  <missing:marker/><x:file target-language="de"/>
</x:xliff>"#,
        "XLIFF parse error: element <marker> uses unbound namespace prefix 'missing'",
    )
    .await;
}

#[tokio::test]
async fn mcp_xliff_import_accepts_normalized_official_namespace_and_bound_extension() {
    assert_mcp_xliff_applied(
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.&#50;" xmlns:ext="urn:example:extension" version="1.2">
  <ext:metadata/><x:file target-language="de"><x:body><x:trans-unit id="greeting">
    <x:source>Hello</x:source><x:target>Normalized namespace</x:target>
  </x:trans-unit></x:body></x:file>
</x:xliff>"#,
        "Normalized namespace",
    )
    .await;
}

#[tokio::test]
async fn mcp_xliff_import_rejects_normalized_alias_collision_without_writing() {
    assert_mcp_xliff_rejected_without_write(
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.2" xmlns:a="urn:attr" xmlns:b="urn:&#97;ttr" version="1.2">
  <x:file target-language="de"><x:body a:custom="first" b:custom="last"/></x:file>
</x:xliff>"#,
        "XLIFF parse error: duplicate expanded attribute '{urn:attr}custom' on <body>",
    )
    .await;
}

#[tokio::test]
async fn mcp_xliff_import_rejects_malformed_namespace_reference_without_writing() {
    assert_mcp_xliff_rejected_without_write(
        r#"<x:xliff xmlns:x="urn:oasis:names:tc:xliff:document:1.&bogus;" version="1.2"><x:file target-language="de"/></x:xliff>"#,
        "XLIFF parse error: invalid XML namespace value on <xliff>",
    )
    .await;
}

#[tokio::test]
async fn submit_returns_machine_readable_ambiguous_warning() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", &prose_catalog());
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        },
    )
    .await
    .unwrap();

    let result = handle_submit_translations(
        &store,
        &cache,
        &write_lock,
        SubmitTranslationsParams {
            file_path: None,
            translations: vec![CompletedTranslation {
                key: "storage".to_string(),
                locale: "de".to_string(),
                value: "100% lokaler Speicher".to_string(),
                plural_forms: None,
                substitution_name: None,
            }],
            dry_run: true,
            continue_on_error: true,
        },
    )
    .await
    .unwrap();

    assert_eq!(result["accepted"], 1);
    assert!(result["rejected"].as_array().unwrap().is_empty());
    assert_eq!(
        result["warnings"][0]["issue_type"],
        "ambiguous_format_sequence_mismatch"
    );
}

#[tokio::test]
async fn submit_handler_applies_compatible_repeated_positions() {
    let store = MemoryStore::new();
    store.add_file(
        "/test/file.xcstrings",
        &simple_catalog("items", "%1$@ has %2$d items; %1$@ again"),
    );
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        },
    )
    .await
    .unwrap();

    let result = handle_submit_translations(
        &store,
        &cache,
        &write_lock,
        SubmitTranslationsParams {
            file_path: None,
            translations: vec![CompletedTranslation {
                key: "items".to_string(),
                locale: "de".to_string(),
                value: "%1$@ erneut: %2$d; %1$@".to_string(),
                plural_forms: None,
                substitution_name: None,
            }],
            dry_run: false,
            continue_on_error: true,
        },
    )
    .await
    .unwrap();

    assert_eq!(result["accepted"], 1);
    assert!(result["rejected"].as_array().unwrap().is_empty());
    assert_eq!(result["dry_run"], false);
    assert!(result["warnings"].is_null());

    let updated = parser::parse(
        &store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        updated.strings["items"].localizations.as_ref().unwrap()["de"]
            .string_unit
            .as_ref()
            .unwrap()
            .value,
        "%1$@ erneut: %2$d; %1$@"
    );
}

#[tokio::test]
async fn submit_handler_rejects_missing_repeated_occurrence_without_writing() {
    let store = MemoryStore::new();
    store.add_file(
        "/test/file.xcstrings",
        &simple_catalog("items", "%1$@ has %2$d items; %1$@ again"),
    );
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        },
    )
    .await
    .unwrap();
    let before = store
        .get_content(Path::new("/test/file.xcstrings"))
        .unwrap();

    let result = handle_submit_translations(
        &store,
        &cache,
        &write_lock,
        SubmitTranslationsParams {
            file_path: None,
            translations: vec![CompletedTranslation {
                key: "items".to_string(),
                locale: "de".to_string(),
                value: "%1$@ erneut: %2$d".to_string(),
                plural_forms: None,
                substitution_name: None,
            }],
            dry_run: false,
            continue_on_error: true,
        },
    )
    .await
    .unwrap();

    assert_eq!(result["accepted"], 0);
    assert_eq!(result["rejected"].as_array().unwrap().len(), 1);
    assert!(
        result["rejected"][0]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("format specifier count mismatch"))
    );
    assert_eq!(result["dry_run"], false);
    assert!(result["warnings"].is_null());
    assert_eq!(
        store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap(),
        before
    );
}

#[tokio::test]
async fn submit_handler_applies_apple_unsigned_z_and_t_aliases() {
    let store = MemoryStore::new();
    store.add_file(
        "/test/file.xcstrings",
        &simple_catalog("size", "%1$zu / %1$tu"),
    );
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        },
    )
    .await
    .unwrap();

    let result = handle_submit_translations(
        &store,
        &cache,
        &write_lock,
        SubmitTranslationsParams {
            file_path: None,
            translations: vec![CompletedTranslation {
                key: "size".to_string(),
                locale: "de".to_string(),
                value: "%1$tu / %1$zu".to_string(),
                plural_forms: None,
                substitution_name: None,
            }],
            dry_run: false,
            continue_on_error: true,
        },
    )
    .await
    .unwrap();

    assert_eq!(result["accepted"], 1);
    assert!(result["rejected"].as_array().unwrap().is_empty());
    let updated = parser::parse(
        &store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        updated.strings["size"].localizations.as_ref().unwrap()["de"]
            .string_unit
            .as_ref()
            .unwrap()
            .value,
        "%1$tu / %1$zu"
    );
}

#[tokio::test]
async fn submit_handler_rejects_signed_unsigned_z_t_collision_without_writing() {
    let store = MemoryStore::new();
    store.add_file(
        "/test/file.xcstrings",
        &simple_catalog("size", "%1$zu / %1$tu"),
    );
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        },
    )
    .await
    .unwrap();
    let before = store
        .get_content(Path::new("/test/file.xcstrings"))
        .unwrap();

    let result = handle_submit_translations(
        &store,
        &cache,
        &write_lock,
        SubmitTranslationsParams {
            file_path: None,
            translations: vec![CompletedTranslation {
                key: "size".to_string(),
                locale: "de".to_string(),
                value: "%1$zu / %1$td".to_string(),
                plural_forms: None,
                substitution_name: None,
            }],
            dry_run: false,
            continue_on_error: true,
        },
    )
    .await
    .unwrap();

    assert_eq!(result["accepted"], 0);
    assert_eq!(result["rejected"].as_array().unwrap().len(), 1);
    assert_eq!(
        result["rejected"][0]["reason"],
        "translation reuses positional argument 1 with incompatible argument types"
    );
    assert_eq!(
        store
            .get_content(Path::new("/test/file.xcstrings"))
            .unwrap(),
        before
    );
}

#[tokio::test]
async fn mcp_xliff_import_returns_same_ambiguous_warning() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", &prose_catalog());
    store.add_file(
        "/test/input.xliff",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2"><file target-language="de"><body>
<trans-unit id="storage"><source>100% Local Storage</source>
<target>100% lokaler Speicher</target></trans-unit>
</body></file></xliff>"#,
    );
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        },
    )
    .await
    .unwrap();

    let result = handle_import_xliff(
        &store,
        &cache,
        &write_lock,
        ImportXliffParams {
            file_path: None,
            xliff_path: "/test/input.xliff".to_string(),
            dry_run: true,
        },
    )
    .await
    .unwrap();

    assert_eq!(result["accepted"], 1);
    assert!(result["rejected"].as_array().unwrap().is_empty());
    assert_eq!(
        result["warnings"][0]["issue_type"],
        "ambiguous_format_sequence_mismatch"
    );
}

#[tokio::test]
async fn submit_and_mcp_xliff_block_same_definite_modifier_mismatch() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", &simple_catalog("days", "%lld days"));
    store.add_file(
        "/test/input.xliff",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2"><file target-language="de"><body>
<trans-unit id="days"><source>%lld days</source>
<target>%Ld Tage</target></trans-unit>
</body></file></xliff>"#,
    );
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        },
    )
    .await
    .unwrap();

    let submitted = handle_submit_translations(
        &store,
        &cache,
        &write_lock,
        SubmitTranslationsParams {
            file_path: None,
            translations: vec![CompletedTranslation {
                key: "days".to_string(),
                locale: "de".to_string(),
                value: "%Ld Tage".to_string(),
                plural_forms: None,
                substitution_name: None,
            }],
            dry_run: true,
            continue_on_error: true,
        },
    )
    .await
    .unwrap();
    let imported = handle_import_xliff(
        &store,
        &cache,
        &write_lock,
        ImportXliffParams {
            file_path: None,
            xliff_path: "/test/input.xliff".to_string(),
            dry_run: true,
        },
    )
    .await
    .unwrap();

    for result in [&submitted, &imported] {
        assert_eq!(result["accepted"], 0);
        assert_eq!(result["rejected"].as_array().unwrap().len(), 1);
        assert!(
            result["rejected"][0]["reason"]
                .as_str()
                .unwrap()
                .contains("invalid format sequence %Ld")
        );
        assert!(result["warnings"].is_null());
    }
}

#[tokio::test]
async fn submit_blocks_non_exact_substitution_placeholder_tokens() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", &substitution_catalog());
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        },
    )
    .await
    .unwrap();

    for target in ["%argument", "%arg_suffix", "%%arg", "%argéclair", "%argβ"] {
        let result = handle_submit_translations(
            &store,
            &cache,
            &write_lock,
            SubmitTranslationsParams {
                file_path: None,
                translations: vec![CompletedTranslation {
                    key: "birds".to_string(),
                    locale: "de".to_string(),
                    value: String::new(),
                    plural_forms: Some(BTreeMap::from([
                        ("one".to_string(), target.to_string()),
                        ("other".to_string(), target.to_string()),
                    ])),
                    substitution_name: Some("BIRDS".to_string()),
                }],
                dry_run: true,
                continue_on_error: true,
            },
        )
        .await
        .unwrap();

        assert_eq!(result["accepted"], 0, "target {target:?}");
        assert_eq!(
            result["rejected"].as_array().unwrap().len(),
            2,
            "target {target:?}"
        );
        assert!(result["rejected"].as_array().unwrap().iter().all(|issue| {
            issue["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("substitution placeholder mismatch"))
        }));
    }
}

#[tokio::test]
async fn submit_accepts_supported_unspaced_script_adjacency() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", &substitution_catalog());
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        },
    )
    .await
    .unwrap();

    let result = handle_submit_translations(
        &store,
        &cache,
        &write_lock,
        SubmitTranslationsParams {
            file_path: None,
            translations: vec![CompletedTranslation {
                key: "birds".to_string(),
                locale: "de".to_string(),
                value: String::new(),
                plural_forms: Some(BTreeMap::from([
                    ("one".to_string(), "%argꥠ".to_string()),
                    ("other".to_string(), "%arg𰀀".to_string()),
                ])),
                substitution_name: Some("BIRDS".to_string()),
            }],
            dry_run: true,
            continue_on_error: true,
        },
    )
    .await
    .unwrap();

    assert_eq!(result["accepted"], 1);
    assert!(result["rejected"].as_array().unwrap().is_empty());
    assert!(result["warnings"].is_null());
}

#[tokio::test]
async fn submit_preserves_format_arguments_next_to_supported_unspaced_scripts() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", &simple_catalog("days", "%d days"));
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        },
    )
    .await
    .unwrap();

    for target in ["%dꥠ", "%d𰀀"] {
        let result = handle_submit_translations(
            &store,
            &cache,
            &write_lock,
            SubmitTranslationsParams {
                file_path: None,
                translations: vec![CompletedTranslation {
                    key: "days".to_string(),
                    locale: "de".to_string(),
                    value: target.to_string(),
                    plural_forms: None,
                    substitution_name: None,
                }],
                dry_run: true,
                continue_on_error: true,
            },
        )
        .await
        .unwrap();

        assert_eq!(result["accepted"], 1, "target {target:?}");
        assert!(result["rejected"].as_array().unwrap().is_empty());
        assert!(result["warnings"].is_null());
    }

    for (target, code) in [
        ("ꥠ", "format specifier count mismatch"),
        ("%fꥠ", "format specifier type mismatch"),
    ] {
        let result = handle_submit_translations(
            &store,
            &cache,
            &write_lock,
            SubmitTranslationsParams {
                file_path: None,
                translations: vec![CompletedTranslation {
                    key: "days".to_string(),
                    locale: "de".to_string(),
                    value: target.to_string(),
                    plural_forms: None,
                    substitution_name: None,
                }],
                dry_run: true,
                continue_on_error: true,
            },
        )
        .await
        .unwrap();

        assert_eq!(result["accepted"], 0, "target {target:?}");
        assert_eq!(result["rejected"].as_array().unwrap().len(), 1);
        assert!(
            result["rejected"][0]["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains(code))
        );
    }
}

#[tokio::test]
async fn submit_matches_all_240_modifier_oracle_cases() {
    let mut strings = IndexMap::new();
    let mut translations = Vec::with_capacity(240);
    let mut valid_keys = Vec::with_capacity(72);
    let mut invalid_cases = Vec::with_capacity(168);

    for (index, token) in modifier_oracle::VALID_CASES.iter().enumerate() {
        let key = format!("valid_{index:03}");
        strings.insert(key.clone(), modifier_entry(token));
        translations.push(CompletedTranslation {
            key: key.clone(),
            locale: "de".to_string(),
            value: format!("{token} Artikel"),
            plural_forms: None,
            substitution_name: None,
        });
        valid_keys.push(key);
    }
    for (index, token) in modifier_oracle::INVALID_CASES.iter().enumerate() {
        let key = format!("invalid_{index:03}");
        strings.insert(key.clone(), modifier_entry(token));
        translations.push(CompletedTranslation {
            key: key.clone(),
            locale: "de".to_string(),
            value: format!("{token} Artikel"),
            plural_forms: None,
            substitution_name: None,
        });
        invalid_cases.push((key, *token));
    }

    let store = MemoryStore::new();
    let catalog = formatter::format_xcstrings(&XcStringsFile {
        source_language: "en".to_string(),
        strings,
        version: "1.0".to_string(),
    })
    .unwrap();
    store.add_file("/test/file.xcstrings", &catalog);
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/file.xcstrings".to_string(),
        },
    )
    .await
    .unwrap();

    let result = handle_submit_translations(
        &store,
        &cache,
        &write_lock,
        SubmitTranslationsParams {
            file_path: None,
            translations,
            dry_run: true,
            continue_on_error: true,
        },
    )
    .await
    .unwrap();

    assert_eq!(result["accepted"], 72);
    assert_eq!(result["accepted_keys"], serde_json::json!(valid_keys));
    assert_eq!(result["rejected"].as_array().unwrap().len(), 336);
    for (key, token) in invalid_cases {
        let matching: Vec<_> = result["rejected"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|issue| issue["key"] == key)
            .collect();
        assert_eq!(matching.len(), 2, "{token:?}");
        assert!(matching.iter().all(|issue| {
            issue["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains(&format!("invalid format sequence {token}")))
        }));
    }
    assert!(result["warnings"].is_null());
}

fn modifier_entry(token: &str) -> StringEntry {
    StringEntry {
        extraction_state: None,
        should_translate: true,
        comment: None,
        localizations: Some(IndexMap::from([(
            "en".to_string(),
            Localization {
                string_unit: Some(StringUnit {
                    state: TranslationState::Translated,
                    value: format!("{token} items"),
                }),
                variations: None,
                substitutions: None,
            },
        )])),
    }
}

fn substitution_catalog() -> String {
    let substitution = serde_json::json!({
        "argNum": 1,
        "formatSpecifier": "lld",
        "variations": {
            "plural": {
                "one": { "stringUnit": { "state": "translated", "value": "%arg bird" } },
                "other": { "stringUnit": { "state": "translated", "value": "%arg birds" } }
            }
        }
    });
    let source = Localization {
        string_unit: Some(StringUnit {
            state: TranslationState::Translated,
            value: "I saw %#@BIRDS@".to_string(),
        }),
        variations: None,
        substitutions: Some(BTreeMap::from([("BIRDS".to_string(), substitution)])),
    };
    let entry = StringEntry {
        extraction_state: None,
        should_translate: true,
        comment: None,
        localizations: Some(IndexMap::from([("en".to_string(), source)])),
    };
    formatter::format_xcstrings(&XcStringsFile {
        source_language: "en".to_string(),
        strings: IndexMap::from([("birds".to_string(), entry)]),
        version: "1.0".to_string(),
    })
    .unwrap()
}
