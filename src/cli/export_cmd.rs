use std::path::PathBuf;
use std::process::ExitCode;

use serde::Serialize;
use xcstrings_mcp::error::XcStringsError;
use xcstrings_mcp::io::FileStore;
use xcstrings_mcp::io::fs::FsFileStore;
use xcstrings_mcp::service::xliff;

use super::common::{EXIT_OK, handle_error, load_file};

#[derive(Serialize)]
struct ExportResult {
    output_path: String,
    locale: String,
    exported_count: usize,
}

pub fn run(
    file: Option<PathBuf>,
    locale: String,
    output: Option<PathBuf>,
    all: bool,
    json: bool,
) -> ExitCode {
    match execute(file, locale, output, all, json) {
        Ok(code) => code,
        Err(err) => handle_error(err),
    }
}

fn execute(
    file: Option<PathBuf>,
    locale: String,
    output: Option<PathBuf>,
    all: bool,
    json: bool,
) -> Result<ExitCode, XcStringsError> {
    let (path, parsed) = load_file(file)?;

    let original = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Localizable.xcstrings");

    let untranslated_only = !all;

    let (xml, count) = xliff::export_xliff(&parsed, &locale, original, untranslated_only)?;

    let output_path = output.unwrap_or_else(|| PathBuf::from(format!("{locale}.xliff")));

    let store = FsFileStore::new();
    store.write(&output_path, &xml)?;

    let output_display = output_path.display().to_string();

    if json {
        let result = ExportResult {
            output_path: output_display,
            locale,
            exported_count: count,
        };
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        eprintln!("Exported {count} keys to {output_display}");
    }

    Ok(ExitCode::from(EXIT_OK))
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
