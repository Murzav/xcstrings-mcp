use std::path::PathBuf;
use std::process::ExitCode;

use xcstrings_mcp::error::XcStringsError;
use xcstrings_mcp::io::FileStore;
use xcstrings_mcp::io::fs::FsFileStore;
use xcstrings_mcp::model::xcstrings::OrderedMap;
use xcstrings_mcp::service::migrate::{self, ParsedLocaleData};
use xcstrings_mcp::service::strings_parser::{
    DiscoveredStringsFile, StringsFileType, decode_strings_content, discover_strings_files,
    extract_locale_from_path, parse_strings,
};
use xcstrings_mcp::service::stringsdict_parser::parse_stringsdict;
use xcstrings_mcp::service::{formatter, parser};

use super::common::{EXIT_OK, handle_error};

pub fn run(
    source_language: String,
    output: PathBuf,
    directory: Option<PathBuf>,
    files: Vec<PathBuf>,
    dry_run: bool,
    json: bool,
) -> ExitCode {
    match execute(source_language, output, directory, files, dry_run, json) {
        Ok(code) => code,
        Err(err) => handle_error(err),
    }
}

fn execute(
    source_language: String,
    output: PathBuf,
    directory: Option<PathBuf>,
    files: Vec<PathBuf>,
    dry_run: bool,
    json: bool,
) -> Result<ExitCode, XcStringsError> {
    // 1. Validate args
    let has_dir = directory.is_some();
    let has_files = !files.is_empty();
    if has_dir == has_files {
        return Err(XcStringsError::InvalidFormat(
            "exactly one of --directory or --files must be provided".into(),
        ));
    }

    match output.extension().and_then(|e| e.to_str()) {
        Some("xcstrings") => {}
        _ => {
            return Err(XcStringsError::InvalidPath {
                path: output,
                reason: "output file must have .xcstrings extension".into(),
            });
        }
    }

    // 2. Discover files
    let discovered: Vec<DiscoveredStringsFile> = if let Some(dir) = &directory {
        discover_strings_files(dir)?
    } else {
        let mut result = Vec::with_capacity(files.len());
        for path in &files {
            let locale = extract_locale_from_path(path)?;
            let file_type = match path.extension().and_then(|e| e.to_str()) {
                Some("strings") => StringsFileType::Strings,
                Some("stringsdict") => StringsFileType::Stringsdict,
                _ => {
                    return Err(XcStringsError::InvalidPath {
                        path: path.clone(),
                        reason: "file must have .strings or .stringsdict extension".into(),
                    });
                }
            };
            let table_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_owned();
            result.push(DiscoveredStringsFile {
                path: path.clone(),
                locale,
                table_name,
                file_type,
            });
        }
        result
    };

    if discovered.is_empty() {
        return Err(XcStringsError::InvalidFormat(
            "no .strings or .stringsdict files found".into(),
        ));
    }

    // 3. Read and parse each file
    let store = FsFileStore::new();
    let mut locale_data: OrderedMap<String, ParsedLocaleData> = OrderedMap::new();
    let mut parse_warnings: Vec<String> = Vec::new();

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
                    parse_warnings.push(format!(
                        "{} keys skipped (unsupported rule types): {}",
                        parsed.skipped_keys.len(),
                        parsed.skipped_keys.join(", ")
                    ));
                }
                entry.stringsdict.extend(parsed.entries);
            }
        }
    }

    // 4. Handle existing file (merge mode)
    let existing = if store.exists(&output) {
        let raw = store.read(&output)?;
        Some(parser::parse(&raw)?)
    } else {
        None
    };

    // 5. Build xcstrings
    let result = migrate::build_xcstrings_from_legacy(&source_language, &locale_data, existing)?;

    let mut all_warnings = parse_warnings;
    all_warnings.extend(result.warnings);

    // 6. Output
    if json {
        #[derive(serde::Serialize)]
        struct JsonOutput {
            output_path: String,
            source_language: String,
            total_keys: usize,
            locales_imported: Vec<migrate::LocaleImportStats>,
            plural_keys: usize,
            warnings: Vec<String>,
            dry_run: bool,
        }
        let out = JsonOutput {
            output_path: output.display().to_string(),
            source_language,
            total_keys: result.total_keys,
            locales_imported: result.locales_imported,
            plural_keys: result.plural_keys,
            warnings: all_warnings,
            dry_run,
        };
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        eprintln!(
            "Migrated {} keys ({} plural) from {} locales",
            result.total_keys,
            result.plural_keys,
            result.locales_imported.len()
        );
        for stats in &result.locales_imported {
            eprintln!("  {}: {} keys", stats.locale, stats.keys_count);
        }
        if !all_warnings.is_empty() {
            eprintln!("Warnings:");
            for w in &all_warnings {
                eprintln!("  {w}");
            }
        }
        if dry_run {
            eprintln!("(dry run — no files written)");
        } else {
            eprintln!("Output: {}", output.display());
        }
    }

    if dry_run {
        return Ok(ExitCode::from(EXIT_OK));
    }

    // 7. Write
    let formatted = formatter::format_xcstrings(&result.file)?;
    store.write(&output, &formatted)?;

    Ok(ExitCode::from(EXIT_OK))
}
