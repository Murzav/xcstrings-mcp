//! Shared CLI/MCP XLIFF transaction over an injected file store.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::model::xcstrings::XcStringsFile;
use crate::model::xliff::XliffImportReport;
use crate::service::{formatter, parser, xliff};

/// Resolve a safe XML destination once so aliases cannot overwrite a catalog.
pub fn resolve_export_destination(
    store: &dyn FileStore,
    source: &Path,
    output: &Path,
) -> Result<PathBuf, XcStringsError> {
    let validate_extension = |path: &Path| {
        if matches!(
            path.extension().and_then(|part| part.to_str()),
            Some("xliff" | "xlf")
        ) {
            Ok(())
        } else {
            Err(XcStringsError::InvalidPath {
                path: path.to_path_buf(),
                reason: "output file must have .xliff or .xlf extension".into(),
            })
        }
    };
    validate_extension(output)?;
    let destination = store.file_identity(output)?;
    if destination == store.file_identity(source)? {
        return Err(XcStringsError::InvalidPath {
            path: output.to_path_buf(),
            reason: "XLIFF output must not overwrite the source catalog".into(),
        });
    }
    validate_extension(&destination)?;
    Ok(destination)
}

#[derive(Debug, Serialize)]
pub struct ImportResult {
    #[serde(flatten)]
    pub report: XliffImportReport,
    pub dry_run: bool,
    pub written: bool,
    /// Catalog keys corresponding to accepted trans-units; repeated keys denote
    /// distinct leaves. Use accepted_destinations for their exact identities.
    pub accepted_keys: Vec<String>,
}

#[derive(Debug)]
pub struct ImportOutcome {
    pub result: ImportResult,
    pub path: PathBuf,
    /// Available only after a successful conditional write, for cache updates.
    pub updated_file: Option<XcStringsFile>,
}

/// Plan against freshly read bytes and apply the whole selected file scope.
///
/// Rejected units, missing targets, dry runs, and failed conditional writes never
/// expose a staged catalog as successfully written. The filesystem store uses
/// its existing cooperative writer lock; external editors need not honor it.
pub fn execute_import(
    store: &dyn FileStore,
    path: &Path,
    xml: &str,
    original: Option<&str>,
    dry_run: bool,
) -> Result<ImportOutcome, XcStringsError> {
    let path = store.file_identity(path)?;
    let expected = store.read_bytes(&path)?;
    let raw = std::str::from_utf8(&expected)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let file = parser::parse(raw.strip_prefix('\u{feff}').unwrap_or(raw))?;
    let document = xliff::parse_document(xml)?;
    let plan = xliff::plan_import(&file, &document, original)?;
    let mut updated_file = None;
    if !dry_run && plan.report.rejected.is_empty() && plan.report.accepted > 0 {
        let candidate = plan.candidate.ok_or_else(|| {
            XcStringsError::XliffFormat("accepted import has no candidate catalog".into())
        })?;
        let formatted = formatter::format_xcstrings(&candidate)?;
        store.write_if_matches(&path, Some(&expected), &formatted)?;
        updated_file = Some(candidate);
    }
    // The report owns key names independently of the destination records.
    let accepted_keys = plan
        .report
        .accepted_destinations
        .iter()
        .map(|destination| destination.key.clone())
        .collect();
    Ok(ImportOutcome {
        result: ImportResult {
            report: plan.report,
            dry_run,
            written: updated_file.is_some(),
            accepted_keys,
        },
        path,
        updated_file,
    })
}

#[cfg(test)]
mod tests;
