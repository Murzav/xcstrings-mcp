//! Shared CLI/MCP XLIFF transaction over an injected file store.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
pub mod export;
pub use export::{ExportWriteReport, PreparedExport, prepare_export, save_export_bundle};

use serde::Serialize;

use crate::error::XcStringsError;
use crate::guidance_operation::{GuidanceReport, GuidanceSnapshot};
use crate::io::FileStore;
use crate::model::xcstrings::XcStringsFile;
use crate::model::xliff::XliffImportReport;
use crate::model::{
    translation::TranslationDestination, workflow::SourceFreshness, xliff::XliffDiagnostic,
};
use crate::service::{assessment, workflow, xliff};
use crate::workflow_operation::CatalogSnapshot;

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
    pub guidance: GuidanceReport,
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
#[derive(Debug, Clone, Copy)]
pub struct ImportOptions<'a> {
    pub original: Option<&'a str>,
    pub dry_run: bool,
    pub expected_source_versions: &'a BTreeMap<String, String>,
}

pub fn execute_import(
    store: &dyn FileStore,
    path: &Path,
    xml: &str,
    options: ImportOptions<'_>,
    glossary_path: &Path,
) -> Result<ImportOutcome, XcStringsError> {
    let snapshot = CatalogSnapshot::load(store, path)?;
    let path = snapshot.identity.clone();
    let document = xliff::parse_document(xml)?;
    let mut plan = xliff::plan_import(&snapshot.catalog, &document, options.original)?;
    let view = workflow::inspect(
        snapshot.identity_text()?,
        &snapshot.catalog,
        &snapshot.workflow,
    )?;
    for destination in &plan.report.accepted_destinations {
        let status = &view.keys[&destination.key];
        let failure = match options.expected_source_versions.get(&destination.key) {
            None => Some((
                "source_version_required",
                "captured source version is required for every imported key",
            )),
            Some(expected) if expected != &status.source_version => Some((
                "source_version_mismatch",
                "source or authored context changed; export again before translating",
            )),
            _ => {
                let ready = plan
                    .candidate
                    .as_ref()
                    .and_then(|file| file.strings.get(&destination.key))
                    .and_then(|entry| entry.localizations.as_ref()?.get(&destination.locale))
                    .and_then(|root| {
                        crate::model::xcstrings::paths::find_leaf(root, &destination.path)
                    })
                    .is_some_and(|unit| assessment::ready(&unit.state));
                (ready && status.freshness != SourceFreshness::Current).then_some((
                    "source_checkpoint_required",
                    "ready XLIFF targets require a current source checkpoint",
                ))
            }
        };
        if let Some((code, message)) = failure {
            plan.report.rejected.push(XliffDiagnostic {
                code: code.into(),
                unit_id: destination.unit_id.clone(),
                message: message.into(),
                destination: Some(destination.clone()),
            });
        }
    }
    if !plan.report.rejected.is_empty() {
        plan.candidate = None;
        plan.report.accepted = 0;
        plan.report.accepted_destinations.clear();
    }
    let guidance = GuidanceSnapshot::load(store, glossary_path);
    let qa_destinations = plan
        .report
        .accepted_destinations
        .iter()
        .map(|destination| TranslationDestination {
            key: destination.key.clone(),
            locale: destination.locale.clone(),
            path: destination.path.clone(),
        })
        .collect::<Vec<_>>();
    let qa = guidance.check_destinations(
        plan.candidate.as_ref().unwrap_or(&snapshot.catalog),
        &snapshot.workflow.contexts,
        &qa_destinations,
    );
    let dry_run = options.dry_run;
    let mut updated_file = None;
    if !dry_run && plan.report.rejected.is_empty() && plan.report.accepted > 0 {
        let candidate = plan.candidate.ok_or_else(|| {
            XcStringsError::XliffFormat("accepted import has no candidate catalog".into())
        })?;
        snapshot.write_catalog(store, &candidate)?;
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
            guidance: qa,
        },
        path,
        updated_file,
    })
}

#[cfg(test)]
mod tests;
