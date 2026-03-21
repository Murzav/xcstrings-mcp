use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::model::translation::SubmitResult;
use crate::service::{formatter, merger, parser, validator, xliff};
use crate::tools::parse::CachedFile;
use crate::tools::{FileCache, resolve_file};

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ExportXliffParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Target locale for the XLIFF export
    pub locale: String,
    /// Path where the XLIFF file will be written
    pub output_path: String,
    /// If true (default), only export untranslated strings
    #[serde(default = "default_true")]
    pub untranslated_only: bool,
}

#[derive(Debug, Serialize)]
struct ExportResult {
    output_path: String,
    locale: String,
    exported_count: usize,
}

/// Handle the `export_xliff` tool call.
///
/// **Limitation**: Only exports simple string translations. Plural forms and
/// device variant forms cannot be represented in XLIFF 1.2 format and are
/// excluded from the export.
pub(crate) async fn handle_export_xliff(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    params: ExportXliffParams,
) -> Result<serde_json::Value, XcStringsError> {
    let (path, file) = resolve_file(store, cache, params.file_path.as_deref()).await?;

    let original = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Localizable.xcstrings");

    let (xml, count) =
        xliff::export_xliff(&file, &params.locale, original, params.untranslated_only)?;

    let output_path = PathBuf::from(&params.output_path);
    match output_path.extension().and_then(|e| e.to_str()) {
        Some("xliff") | Some("xlf") => {}
        _ => {
            return Err(XcStringsError::InvalidPath {
                path: output_path,
                reason: "output file must have .xliff or .xlf extension".into(),
            });
        }
    }
    store.write(&output_path, &xml)?;

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
    /// Path to the XLIFF file to import
    pub xliff_path: String,
    /// If true, validate without writing
    #[serde(default)]
    pub dry_run: bool,
}

/// Handle the `import_xliff` tool call.
///
/// **Limitation**: Only imports simple string translations. Plural forms and
/// substitution translations cannot be represented in XLIFF 1.2 format and
/// are skipped during import. Use `submit_translations` with `plural_forms`
/// for plural key translations.
pub(crate) async fn handle_import_xliff(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    write_lock: &Mutex<()>,
    params: ImportXliffParams,
) -> Result<serde_json::Value, XcStringsError> {
    let (path, file) = resolve_file(store, cache, params.file_path.as_deref()).await?;

    // Read and parse XLIFF
    let xliff_path = PathBuf::from(&params.xliff_path);
    let xliff_content = store.read(&xliff_path)?;
    let (_locale, translations) = xliff::import_xliff(&xliff_content)?;

    if translations.is_empty() {
        let result = SubmitResult {
            accepted: 0,
            accepted_keys: vec![],
            rejected: vec![],
            dry_run: params.dry_run,
        };
        return Ok(serde_json::to_value(result)?);
    }

    // Validate using existing pipeline
    let rejected = validator::validate_translations(&file, &translations);
    let rejected_keys: std::collections::HashSet<&str> =
        rejected.iter().map(|r| r.key.as_str()).collect();
    let accepted: Vec<_> = translations
        .iter()
        .filter(|t| !rejected_keys.contains(t.key.as_str()))
        .collect();

    if params.dry_run || accepted.is_empty() {
        let result = SubmitResult {
            accepted: accepted.len(),
            accepted_keys: accepted.iter().map(|t| t.key.clone()).collect(),
            rejected,
            dry_run: params.dry_run,
        };
        return Ok(serde_json::to_value(result)?);
    }

    // Write: acquire lock, re-read, merge, format, write
    let _write_guard = write_lock.lock().await;
    let raw = store.read(&path)?;
    let mut fresh_file = parser::parse(&raw)?;

    // Re-validate against fresh file (it may have changed since initial validation)
    let fresh_rejected = validator::validate_translations(&fresh_file, &translations);
    let fresh_rejected_keys: std::collections::HashSet<&str> =
        fresh_rejected.iter().map(|r| r.key.as_str()).collect();

    let owned: Vec<_> = accepted
        .into_iter()
        .filter(|t| !fresh_rejected_keys.contains(t.key.as_str()))
        .cloned()
        .collect();

    if owned.is_empty() {
        let mut all_rejected = rejected;
        all_rejected.extend(fresh_rejected);
        let result = SubmitResult {
            accepted: 0,
            accepted_keys: vec![],
            rejected: all_rejected,
            dry_run: false,
        };
        return Ok(serde_json::to_value(result)?);
    }

    let merge_result = merger::merge_translations(&mut fresh_file, &owned);

    let formatted = formatter::format_xcstrings(&fresh_file)?;
    store.write(&path, &formatted)?;

    // Update cache (same pattern as translate.rs)
    let mtime = store.modified_time(&path)?;
    let mut guard = cache.lock().await;
    guard.insert(
        path.clone(),
        CachedFile {
            path,
            content: fresh_file,
            modified: mtime,
        },
    );

    let mut all_rejected = rejected;
    all_rejected.extend(fresh_rejected);
    all_rejected.extend(merge_result.rejected);

    let result = SubmitResult {
        accepted: merge_result.accepted,
        accepted_keys: merge_result.accepted_keys,
        rejected: all_rejected,
        dry_run: false,
    };
    Ok(serde_json::to_value(result)?)
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
