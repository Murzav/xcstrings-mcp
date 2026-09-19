use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::service::xliff;
use crate::tools::parse::CachedFile;
use crate::tools::{FileCache, mcp_log, resolve_file};
use crate::xliff_operation::{execute_import, resolve_export_destination};

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ExportXliffParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Exact XLIFF file original, such as App/Localizable.xcstrings; defaults to the filename.
    #[serde(default)]
    pub original: Option<String>,
    /// Target locale for the XLIFF export
    pub locale: String,
    /// Path where the XLIFF file will be written
    pub output_path: String,
    /// If true (default), only export untranslated strings. Set false to export all including already-translated.
    #[serde(default = "default_true")]
    pub untranslated_only: bool,
}

#[derive(Debug, Serialize)]
struct ExportResult {
    output_path: String,
    locale: String,
    exported_count: usize,
}

/// Export every supported Apple translation leaf with its exact variation identity.
pub(crate) async fn handle_export_xliff(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    params: ExportXliffParams,
) -> Result<serde_json::Value, XcStringsError> {
    let (path, file) = resolve_file(store, cache, params.file_path.as_deref()).await?;

    let original = params.original.as_deref().unwrap_or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Localizable.xcstrings")
    });

    let (xml, count) =
        xliff::export_xliff(&file, &params.locale, original, params.untranslated_only)?;

    let output_path =
        resolve_export_destination(store, &path, &PathBuf::from(&params.output_path))?;
    store.write(&output_path, &xml)?;

    mcp_log(&format!("Exported {count} translation leaves to XLIFF"));

    let result = ExportResult {
        output_path: params.output_path,
        locale: params.locale,
        exported_count: count,
    };
    Ok(serde_json::to_value(result)?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ImportXliffParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Select an exact file@original when the document contains multiple catalog scopes.
    #[serde(default)]
    pub original: Option<String>,
    /// Path to the XLIFF file to import
    pub xliff_path: String,
    /// If true, validate without writing
    #[serde(default)]
    pub dry_run: bool,
}

/// Import one selected Apple catalog scope as an all-or-nothing transaction.
pub(crate) async fn handle_import_xliff(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    write_lock: &Mutex<()>,
    params: ImportXliffParams,
) -> Result<serde_json::Value, XcStringsError> {
    let path = match params.file_path {
        Some(path) => PathBuf::from(path),
        None => cache
            .lock()
            .await
            .active_path()
            .cloned()
            .ok_or(XcStringsError::NoActiveFile)?,
    };
    if path.extension().and_then(|extension| extension.to_str()) != Some("xcstrings") {
        return Err(XcStringsError::NotXcStrings { path });
    }
    let xml = store.read(&PathBuf::from(&params.xliff_path))?;
    let _write_guard = write_lock.lock().await;
    let outcome = execute_import(
        store,
        &path,
        &xml,
        params.original.as_deref(),
        params.dry_run,
    )?;
    if let Some(content) = outcome.updated_file {
        match store.modified_time(&outcome.path) {
            Ok(modified) => cache.lock().await.insert(
                outcome.path,
                CachedFile {
                    path,
                    content,
                    modified,
                },
            ),
            Err(error) => {
                // The conditional write already committed. Never report it as failed
                // because refreshing optional cache metadata was unsuccessful.
                cache.lock().await.files.remove(&outcome.path);
                mcp_log(&format!(
                    "Import saved; catalog cache invalidated because metadata could not be read: {error}"
                ));
            }
        }
    }
    Ok(serde_json::to_value(outcome.result)?)
}

#[cfg(test)]
mod tests {
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
}

#[cfg(test)]
#[path = "xliff/structure_tests.rs"]
mod structure_tests;

#[cfg(test)]
#[path = "xliff/apple_compat_tests.rs"]
mod apple_compat_tests;

#[cfg(test)]
#[path = "xliff/cdata_tests.rs"]
mod cdata_tests;

#[cfg(test)]
#[path = "xliff/cache_tests.rs"]
mod cache_tests;
