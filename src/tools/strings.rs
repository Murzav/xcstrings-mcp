use std::path::PathBuf;

use rmcp::RoleServer;
use rmcp::model::LoggingLevel;
use rmcp::service::Peer;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::model::xcstrings::OrderedMap;
use crate::service::migrate::{self, LocaleImportStats, ParsedLocaleData};
use crate::service::strings_parser::{
    DiscoveredStringsFile, StringsFileType, decode_strings_content, discover_strings_files,
    extract_locale_from_path, parse_strings,
};
use crate::service::stringsdict_parser::parse_stringsdict;

use crate::service::{formatter, parser};
use crate::tools::parse::CachedFile;
use crate::tools::{FileCache, mcp_log};

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ImportStringsParams {
    /// Paths to .strings/.stringsdict files (one per locale).
    /// Mutually exclusive with `directory`.
    #[serde(default)]
    pub file_paths: Option<Vec<String>>,
    /// Directory to scan recursively for .lproj folders (e.g., "/path/to/MyApp").
    /// Mutually exclusive with `file_paths`.
    #[serde(default)]
    pub directory: Option<String>,
    /// Source language code (e.g., "en")
    pub source_language: String,
    /// Output .xcstrings file path
    pub output_path: String,
    /// Preview without writing
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Serialize)]
struct ImportStringsResult {
    output_path: String,
    source_language: String,
    total_keys: usize,
    locales_imported: Vec<LocaleImportStats>,
    plural_keys: usize,
    warnings: Vec<String>,
    dry_run: bool,
}

pub(crate) async fn handle_import_strings(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    write_lock: &Mutex<()>,
    params: ImportStringsParams,
    peer: Option<&Peer<RoleServer>>,
) -> Result<serde_json::Value, XcStringsError> {
    // 1. Validate params
    let has_paths = params.file_paths.as_ref().is_some_and(|v| !v.is_empty());
    let has_dir = params.directory.is_some();
    if has_paths == has_dir {
        return Err(XcStringsError::InvalidFormat(
            "exactly one of file_paths or directory must be provided".into(),
        ));
    }

    let output_path = PathBuf::from(&params.output_path);
    match output_path.extension().and_then(|e| e.to_str()) {
        Some("xcstrings") => {}
        _ => {
            return Err(XcStringsError::InvalidPath {
                path: output_path,
                reason: "output file must have .xcstrings extension".into(),
            });
        }
    }

    // 2. Resolve files
    let discovered: Vec<DiscoveredStringsFile> = if let Some(dir) = &params.directory {
        let dir_path = PathBuf::from(dir);
        discover_strings_files(&dir_path)?
    } else {
        // has_paths is true, so file_paths is Some with non-empty vec
        let paths = params.file_paths.as_deref().unwrap_or_default();
        let mut files = Vec::with_capacity(paths.len());
        for p in paths {
            let path = PathBuf::from(p);
            let locale = extract_locale_from_path(&path)?;
            let file_type = match path.extension().and_then(|e| e.to_str()) {
                Some("strings") => StringsFileType::Strings,
                Some("stringsdict") => StringsFileType::Stringsdict,
                _ => {
                    return Err(XcStringsError::InvalidPath {
                        path,
                        reason: "file must have .strings or .stringsdict extension".into(),
                    });
                }
            };
            let table_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_owned();
            files.push(DiscoveredStringsFile {
                path,
                locale,
                table_name,
                file_type,
            });
        }
        files
    };

    if discovered.is_empty() {
        return Err(XcStringsError::InvalidFormat(
            "no .strings or .stringsdict files found".into(),
        ));
    }

    mcp_log(
        peer,
        LoggingLevel::Info,
        &format!("Found {} files to import", discovered.len()),
    )
    .await;

    // 3. Read, decode, and parse each file; group by locale
    let mut locale_data: OrderedMap<String, ParsedLocaleData> = OrderedMap::new();
    let mut warnings: Vec<String> = Vec::new();

    for file_info in &discovered {
        let raw = store.read_bytes(&file_info.path)?;
        let entry = locale_data
            .entry(file_info.locale.clone())
            .or_insert_with(|| ParsedLocaleData {
                strings: Vec::new(),
                stringsdict: Vec::new(),
            });

        match file_info.file_type {
            StringsFileType::Strings => {
                let content = decode_strings_content(&raw)?;
                let parsed = parse_strings(&content)?;
                entry.strings.extend(parsed);
            }
            StringsFileType::Stringsdict => {
                let content = String::from_utf8(raw)
                    .map_err(|e| XcStringsError::StringsdictParse(format!("invalid UTF-8: {e}")))?;
                let parsed = parse_stringsdict(&content)?;
                if !parsed.skipped_keys.is_empty() {
                    warnings.push(format!(
                        "{} keys skipped (unsupported rule types): {}",
                        parsed.skipped_keys.len(),
                        parsed.skipped_keys.join(", ")
                    ));
                }
                entry.stringsdict.extend(parsed.entries);
            }
        }
    }

    // 4. Handle existing file merge mode setup
    let existing = if store.exists(&output_path) {
        let existing_raw = store.read(&output_path)?;
        Some(parser::parse(&existing_raw)?)
    } else {
        None
    };

    // 5. Build xcstrings from legacy data
    let result =
        migrate::build_xcstrings_from_legacy(&params.source_language, &locale_data, existing)?;

    // Combine warnings from file parsing with warnings from conversion
    let mut all_warnings = warnings;
    all_warnings.extend(result.warnings);

    let total_keys = result.total_keys;
    let plural_keys = result.plural_keys;
    let locales_imported = result.locales_imported;
    let xcstrings_file = result.file;

    // 6. Dry run
    if params.dry_run {
        let result = ImportStringsResult {
            output_path: params.output_path,
            source_language: params.source_language,
            total_keys,
            locales_imported,
            plural_keys,
            warnings: all_warnings,
            dry_run: true,
        };
        return Ok(serde_json::to_value(result)?);
    }

    // 7. Write
    let _write_guard = write_lock.lock().await;
    let formatted = formatter::format_xcstrings(&xcstrings_file)?;
    store.write(&output_path, &formatted)?;

    // Update cache
    let mtime = store.modified_time(&output_path)?;
    let mut guard = cache.lock().await;
    guard.insert(
        output_path.clone(),
        CachedFile {
            path: output_path,
            content: xcstrings_file,
            modified: mtime,
        },
    );

    mcp_log(
        peer,
        LoggingLevel::Info,
        &format!("Imported {total_keys} keys ({plural_keys} plural)"),
    )
    .await;

    let result = ImportStringsResult {
        output_path: params.output_path,
        source_language: params.source_language,
        total_keys,
        locales_imported,
        plural_keys,
        warnings: all_warnings,
        dry_run: false,
    };
    Ok(serde_json::to_value(result)?)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::tools::test_helpers::MemoryStore;

    fn en_strings() -> &'static str {
        include_str!("../../tests/fixtures/en.lproj/Localizable.strings")
    }

    fn es_strings() -> &'static str {
        include_str!("../../tests/fixtures/es.lproj/Localizable.strings")
    }

    fn en_stringsdict() -> &'static str {
        include_str!("../../tests/fixtures/en.lproj/Localizable.stringsdict")
    }

    #[tokio::test]
    async fn test_import_single_locale_strings() {
        let store = MemoryStore::new();
        store.add_file("/proj/en.lproj/Localizable.strings", en_strings());
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let params = ImportStringsParams {
            file_paths: Some(vec!["/proj/en.lproj/Localizable.strings".to_string()]),
            directory: None,
            source_language: "en".to_string(),
            output_path: "/proj/Localizable.xcstrings".to_string(),
            dry_run: false,
        };

        let result = handle_import_strings(&store, &cache, &write_lock, params, None)
            .await
            .unwrap();

        assert_eq!(result["source_language"], "en");
        assert!(result["total_keys"].as_u64().unwrap() > 0);
        assert_eq!(result["dry_run"], false);

        let content = store
            .get_content(Path::new("/proj/Localizable.xcstrings"))
            .unwrap();
        assert!(content.contains("\"sourceLanguage\" : \"en\""));
        assert!(content.contains("common.ok"));
    }

    #[tokio::test]
    async fn test_import_source_and_target_locale() {
        let store = MemoryStore::new();
        store.add_file("/proj/en.lproj/Localizable.strings", en_strings());
        store.add_file("/proj/es.lproj/Localizable.strings", es_strings());
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let params = ImportStringsParams {
            file_paths: Some(vec![
                "/proj/en.lproj/Localizable.strings".to_string(),
                "/proj/es.lproj/Localizable.strings".to_string(),
            ]),
            directory: None,
            source_language: "en".to_string(),
            output_path: "/proj/Localizable.xcstrings".to_string(),
            dry_run: false,
        };

        let result = handle_import_strings(&store, &cache, &write_lock, params, None)
            .await
            .unwrap();

        assert_eq!(result["locales_imported"].as_array().unwrap().len(), 2);

        let content = store
            .get_content(Path::new("/proj/Localizable.xcstrings"))
            .unwrap();
        assert!(content.contains("Cancelar")); // es translation
        assert!(content.contains("Cancel")); // en source
    }

    #[tokio::test]
    async fn test_import_stringsdict_simple_plural() {
        let store = MemoryStore::new();
        store.add_file("/proj/en.lproj/Localizable.stringsdict", en_stringsdict());
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let params = ImportStringsParams {
            file_paths: Some(vec!["/proj/en.lproj/Localizable.stringsdict".to_string()]),
            directory: None,
            source_language: "en".to_string(),
            output_path: "/proj/Localizable.xcstrings".to_string(),
            dry_run: false,
        };

        let result = handle_import_strings(&store, &cache, &write_lock, params, None)
            .await
            .unwrap();

        assert!(result["plural_keys"].as_u64().unwrap() >= 2);

        let content = store
            .get_content(Path::new("/proj/Localizable.xcstrings"))
            .unwrap();
        // items_count is a simple plural — should have variations.plural
        assert!(content.contains("items_count"));
    }

    #[tokio::test]
    async fn test_import_stringsdict_complex_plural() {
        let store = MemoryStore::new();
        store.add_file("/proj/en.lproj/Localizable.stringsdict", en_stringsdict());
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let params = ImportStringsParams {
            file_paths: Some(vec!["/proj/en.lproj/Localizable.stringsdict".to_string()]),
            directory: None,
            source_language: "en".to_string(),
            output_path: "/proj/Localizable.xcstrings".to_string(),
            dry_run: false,
        };

        handle_import_strings(&store, &cache, &write_lock, params, None)
            .await
            .unwrap();

        let content = store
            .get_content(Path::new("/proj/Localizable.xcstrings"))
            .unwrap();
        // photos_in_albums is complex (multi-variable) — should have substitutions
        assert!(content.contains("photos_in_albums"));
        assert!(content.contains("substitutions"));
    }

    #[tokio::test]
    async fn test_import_mixed_strings_and_stringsdict() {
        let store = MemoryStore::new();
        store.add_file("/proj/en.lproj/Localizable.strings", en_strings());
        store.add_file("/proj/en.lproj/Localizable.stringsdict", en_stringsdict());
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let params = ImportStringsParams {
            file_paths: Some(vec![
                "/proj/en.lproj/Localizable.strings".to_string(),
                "/proj/en.lproj/Localizable.stringsdict".to_string(),
            ]),
            directory: None,
            source_language: "en".to_string(),
            output_path: "/proj/Localizable.xcstrings".to_string(),
            dry_run: false,
        };

        let result = handle_import_strings(&store, &cache, &write_lock, params, None)
            .await
            .unwrap();

        // Should have both .strings keys and .stringsdict keys
        let total = result["total_keys"].as_u64().unwrap();
        assert!(total > 15); // .strings has ~20 keys + .stringsdict has 3
    }

    #[tokio::test]
    async fn test_import_dry_run() {
        let store = MemoryStore::new();
        store.add_file("/proj/en.lproj/Localizable.strings", en_strings());
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let params = ImportStringsParams {
            file_paths: Some(vec!["/proj/en.lproj/Localizable.strings".to_string()]),
            directory: None,
            source_language: "en".to_string(),
            output_path: "/proj/Localizable.xcstrings".to_string(),
            dry_run: true,
        };

        let result = handle_import_strings(&store, &cache, &write_lock, params, None)
            .await
            .unwrap();

        assert_eq!(result["dry_run"], true);
        assert!(result["total_keys"].as_u64().unwrap() > 0);

        // File should NOT exist
        assert!(!store.exists(Path::new("/proj/Localizable.xcstrings")));
    }

    #[tokio::test]
    async fn test_import_invalid_output_extension() {
        let store = MemoryStore::new();
        store.add_file("/proj/en.lproj/Localizable.strings", en_strings());
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let params = ImportStringsParams {
            file_paths: Some(vec!["/proj/en.lproj/Localizable.strings".to_string()]),
            directory: None,
            source_language: "en".to_string(),
            output_path: "/proj/output.json".to_string(),
            dry_run: false,
        };

        let result = handle_import_strings(&store, &cache, &write_lock, params, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_import_source_language_not_found() {
        let store = MemoryStore::new();
        store.add_file("/proj/es.lproj/Localizable.strings", es_strings());
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let params = ImportStringsParams {
            file_paths: Some(vec!["/proj/es.lproj/Localizable.strings".to_string()]),
            directory: None,
            source_language: "en".to_string(),
            output_path: "/proj/Localizable.xcstrings".to_string(),
            dry_run: false,
        };

        let result = handle_import_strings(&store, &cache, &write_lock, params, None).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("source language"));
    }

    #[tokio::test]
    async fn test_import_merge_into_existing() {
        let store = MemoryStore::new();
        store.add_file("/proj/en.lproj/Localizable.strings", en_strings());

        // Create an existing .xcstrings with one key
        let existing = r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "existing.key" : {
      "extractionState" : "manual",
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "Existing"
          }
        }
      }
    }
  },
  "version" : "1.0"
}"#;
        store.add_file("/proj/Localizable.xcstrings", existing);

        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let params = ImportStringsParams {
            file_paths: Some(vec!["/proj/en.lproj/Localizable.strings".to_string()]),
            directory: None,
            source_language: "en".to_string(),
            output_path: "/proj/Localizable.xcstrings".to_string(),
            dry_run: false,
        };

        let result = handle_import_strings(&store, &cache, &write_lock, params, None)
            .await
            .unwrap();

        let content = store
            .get_content(Path::new("/proj/Localizable.xcstrings"))
            .unwrap();
        // Existing key should still be there
        assert!(content.contains("existing.key"));
        // New keys should be added
        assert!(content.contains("common.ok"));

        // Total keys should include both existing and new
        let total = result["total_keys"].as_u64().unwrap();
        assert!(total > 1); // existing.key + imported keys
    }

    #[tokio::test]
    async fn test_import_both_params_error() {
        let store = MemoryStore::new();
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let params = ImportStringsParams {
            file_paths: Some(vec!["some.strings".to_string()]),
            directory: Some("/dir".to_string()),
            source_language: "en".to_string(),
            output_path: "/out.xcstrings".to_string(),
            dry_run: false,
        };

        let result = handle_import_strings(&store, &cache, &write_lock, params, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_import_key_in_target_not_source() {
        let store = MemoryStore::new();
        // en has only key "a"
        store.add_file("/proj/en.lproj/Localizable.strings", "\"a\" = \"Apple\";");
        // es has keys "a" and "b"
        store.add_file(
            "/proj/es.lproj/Localizable.strings",
            "\"a\" = \"Manzana\";\n\"b\" = \"Banana\";",
        );
        let cache = Mutex::new(FileCache::new());
        let write_lock = Mutex::new(());

        let params = ImportStringsParams {
            file_paths: Some(vec![
                "/proj/en.lproj/Localizable.strings".to_string(),
                "/proj/es.lproj/Localizable.strings".to_string(),
            ]),
            directory: None,
            source_language: "en".to_string(),
            output_path: "/proj/Localizable.xcstrings".to_string(),
            dry_run: false,
        };

        let result = handle_import_strings(&store, &cache, &write_lock, params, None)
            .await
            .unwrap();

        // Should have warning about "b" being in es but not source
        let warnings = result["warnings"].as_array().unwrap();
        let has_warning = warnings.iter().any(|w| {
            w.as_str().unwrap().contains("'b'") && w.as_str().unwrap().contains("not in source")
        });
        assert!(
            has_warning,
            "expected warning about key 'b' not in source, got: {warnings:?}"
        );

        // "b" should be added to the result with source locale having state=new
        let content = store
            .get_content(Path::new("/proj/Localizable.xcstrings"))
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        let b_en = &parsed["strings"]["b"]["localizations"]["en"]["stringUnit"];
        assert_eq!(b_en["state"], "new");
        assert_eq!(b_en["value"], "");
    }
}
