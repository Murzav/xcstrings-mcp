use super::import_with_current_checkpoint as handle_import_xliff;
use super::*;
use crate::tools::FileCache;
use crate::tools::parse::{ParseParams, handle_parse};
use crate::tools::test_helpers::{MemoryStore, SIMPLE_FIXTURE};
use std::path::Path;

#[tokio::test]
async fn test_export_xliff_writes_file() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
    let cache = Mutex::new(FileCache::new());

    let parse_params = ParseParams {
        file_path: "/test/file.xcstrings".to_string(),
    };
    handle_parse(&store, &cache, parse_params).await.unwrap();

    let params = ExportXliffParams {
        original: None,
        file_path: None,
        locale: "de".to_string(),
        output_path: "/test/output.xliff".to_string(),
        untranslated_only: false,
    };

    let result = handle_export_xliff(&store, &cache, params).await.unwrap();
    assert_eq!(result["locale"], "de");
    assert!(result["exported_count"].as_u64().unwrap() > 0);

    let content = store.get_content(Path::new("/test/output.xliff")).unwrap();
    assert!(content.contains("<xliff"));
    assert!(content.contains("target-language=\"de\""));
}

#[tokio::test]
async fn test_export_xliff_rejects_non_xliff_output_path() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
    let cache = Mutex::new(FileCache::new());

    let parse_params = ParseParams {
        file_path: "/test/file.xcstrings".to_string(),
    };
    handle_parse(&store, &cache, parse_params).await.unwrap();

    let params = ExportXliffParams {
        original: None,
        file_path: None,
        locale: "de".to_string(),
        output_path: "/test/output.txt".to_string(),
        untranslated_only: false,
    };

    let result = handle_export_xliff(&store, &cache, params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, XcStringsError::InvalidPath { .. }),
        "expected InvalidPath, got: {err:?}"
    );
}

#[tokio::test]
async fn test_import_xliff_dry_run() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());

    let parse_params = ParseParams {
        file_path: "/test/file.xcstrings".to_string(),
    };
    handle_parse(&store, &cache, parse_params).await.unwrap();

    let xliff = r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">
  <file source-language="en" target-language="de" original="file.xcstrings" datatype="plaintext">
    <body>
      <trans-unit id="greeting">
        <source>Hello</source>
        <target state="translated">Hallo</target>
      </trans-unit>
    </body>
  </file>
</xliff>"#;
    store.add_file("/test/input.xliff", xliff);

    let params = ImportXliffParams {
        expected_source_versions: Default::default(),
        original: None,
        file_path: None,
        xliff_path: "/test/input.xliff".to_string(),
        dry_run: true,
    };

    let result = handle_import_xliff(&store, &cache, &write_lock, params)
        .await
        .unwrap();
    assert_eq!(result["dry_run"], true);
    assert_eq!(result["accepted"], 1);

    // File should NOT be modified
    let content = store
        .get_content(Path::new("/test/file.xcstrings"))
        .unwrap();
    assert!(!content.contains("Hallo"));
}

#[tokio::test]
async fn test_import_xliff_writes_translations() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());

    let parse_params = ParseParams {
        file_path: "/test/file.xcstrings".to_string(),
    };
    handle_parse(&store, &cache, parse_params).await.unwrap();

    let xliff = r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">
  <file source-language="en" target-language="de" original="file.xcstrings" datatype="plaintext">
    <body>
      <trans-unit id="greeting">
        <source>Hello</source>
        <target state="translated">Hallo</target>
      </trans-unit>
      <trans-unit id="welcome_message">
        <source>Welcome to the app</source>
        <target state="translated">Willkommen in der App</target>
      </trans-unit>
    </body>
  </file>
</xliff>"#;
    store.add_file("/test/input.xliff", xliff);

    let params = ImportXliffParams {
        expected_source_versions: Default::default(),
        original: None,
        file_path: None,
        xliff_path: "/test/input.xliff".to_string(),
        dry_run: false,
    };

    let result = handle_import_xliff(&store, &cache, &write_lock, params)
        .await
        .unwrap();
    assert_eq!(result["dry_run"], false);
    assert_eq!(result["accepted"], 2);

    let content = store
        .get_content(Path::new("/test/file.xcstrings"))
        .unwrap();
    assert!(content.contains("Hallo"));
    assert!(content.contains("Willkommen"));
}

#[tokio::test]
async fn test_import_empty_xliff() {
    let store = MemoryStore::new();
    store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
    let cache = Mutex::new(FileCache::new());
    let write_lock = Mutex::new(());

    let parse_params = ParseParams {
        file_path: "/test/file.xcstrings".to_string(),
    };
    handle_parse(&store, &cache, parse_params).await.unwrap();

    let xliff = r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">
  <file source-language="en" target-language="de" original="file.xcstrings" datatype="plaintext">
    <body>
    </body>
  </file>
</xliff>"#;
    store.add_file("/test/input.xliff", xliff);

    let params = ImportXliffParams {
        expected_source_versions: Default::default(),
        original: None,
        file_path: None,
        xliff_path: "/test/input.xliff".to_string(),
        dry_run: false,
    };

    let result = handle_import_xliff(&store, &cache, &write_lock, params)
        .await
        .unwrap();
    assert_eq!(result["accepted"], 0);
    assert!(result["rejected"].as_array().unwrap().is_empty());
}
