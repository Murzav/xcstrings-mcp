use std::path::PathBuf;
use std::process::ExitCode;

use serde::Serialize;
use xcstrings_mcp::error::XcStringsError;
use xcstrings_mcp::io::fs::FsFileStore;
use xcstrings_mcp::workflow_operation::CatalogSnapshot;
use xcstrings_mcp::xliff_operation::{ExportWriteReport, prepare_export, save_export_bundle};

use super::common::{EXIT_ERROR, EXIT_OK, handle_error, load_file};

#[derive(Serialize)]
struct ExportResult {
    #[serde(flatten)]
    writes: ExportWriteReport,
    source_versions: std::collections::BTreeMap<String, String>,
    locale: String,
    exported_count: usize,
}

pub fn run(
    file: Option<PathBuf>,
    locale: String,
    output: Option<PathBuf>,
    original: Option<String>,
    source_versions_output: Option<PathBuf>,
    all: bool,
    json: bool,
) -> ExitCode {
    match execute(
        file,
        locale,
        output,
        original,
        source_versions_output,
        all,
        json,
    ) {
        Ok(code) => code,
        Err(err) => handle_error(err),
    }
}

fn execute(
    file: Option<PathBuf>,
    locale: String,
    output: Option<PathBuf>,
    original: Option<String>,
    source_versions_output: Option<PathBuf>,
    all: bool,
    json: bool,
) -> Result<ExitCode, XcStringsError> {
    let (path, _) = load_file(file)?;
    let store = FsFileStore::new();
    let snapshot = CatalogSnapshot::load(&store, &path)?;

    let original = original.as_deref().unwrap_or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Localizable.xcstrings")
    });

    let untranslated_only = !all;

    let prepared = prepare_export(&snapshot, &locale, original, untranslated_only)?;
    let count = prepared.exported_count;

    let output_path = output.unwrap_or_else(|| PathBuf::from(format!("{locale}.xliff")));

    let writes = save_export_bundle(
        &store,
        &snapshot,
        &prepared,
        &output_path,
        source_versions_output.as_deref(),
    )?;
    let success = writes.xml_written;

    let output_display = output_path.display().to_string();

    if json {
        let result = ExportResult {
            writes,
            source_versions: prepared.source_versions,
            locale,
            exported_count: count,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        if let Some(error) = writes.phase_error {
            eprintln!("{error}");
        } else {
            eprintln!("Exported {count} translation leaves to {output_display}");
        }
        eprintln!("Captured source versions: {}", writes.source_versions_path);
    }

    Ok(ExitCode::from(if success { EXIT_OK } else { EXIT_ERROR }))
}

#[cfg(test)]
mod tests {
    use xcstrings_mcp::service::{parser, xliff};

    const SIMPLE_FIXTURE: &str = include_str!("../../tests/fixtures/simple.xcstrings");

    #[test]
    fn export_produces_xliff_output() {
        let file = parser::parse(SIMPLE_FIXTURE).unwrap();
        let (xml, count) =
            xliff::export_xliff(&file, "uk", "Localizable.xcstrings", false).unwrap();

        assert!(count > 0);
        assert!(xml.contains("<xliff"));
        assert!(xml.contains("target-language=\"uk\""));
    }

    #[test]
    fn export_untranslated_only_filters() {
        let file = parser::parse(SIMPLE_FIXTURE).unwrap();
        let (_all_xml, all_count) =
            xliff::export_xliff(&file, "uk", "test.xcstrings", false).unwrap();
        let (_filtered_xml, filtered_count) =
            xliff::export_xliff(&file, "uk", "test.xcstrings", true).unwrap();

        assert!(all_count >= filtered_count);
    }
}
