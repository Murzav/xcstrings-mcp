use crate::{
    error::XcStringsError,
    io::FileStore,
    service::{workflow, xliff},
    workflow_operation::CatalogSnapshot,
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct PreparedExport {
    pub xml: String,
    pub exported_count: usize,
    pub source_versions: BTreeMap<String, String>,
}
pub fn prepare_export(
    snapshot: &CatalogSnapshot,
    locale: &str,
    original: &str,
    untranslated_only: bool,
) -> Result<PreparedExport, XcStringsError> {
    let view = workflow::inspect(
        snapshot.identity_text()?,
        &snapshot.catalog,
        &snapshot.workflow,
    )?;
    let (xml, exported_count, keys) = xliff::export_xliff_with_keys(
        &view.effective_catalog,
        locale,
        original,
        untranslated_only,
    )?;
    let source_versions = keys
        .into_iter()
        .map(|key| {
            let version = view.keys[&key].source_version.clone();
            (key, version)
        })
        .collect();
    Ok(PreparedExport {
        xml,
        exported_count,
        source_versions,
    })
}
#[derive(Debug, Serialize)]
pub struct ExportWriteReport {
    pub output_path: String,
    pub source_versions_path: String,
    pub xml_written: bool,
    pub source_versions_written: bool,
    pub phase_error: Option<String>,
}
/// Persist the captured version map first, then XML. Report a committed first
/// output accurately if the second conditional write fails.
pub fn save_export_bundle(
    store: &dyn FileStore,
    snapshot: &CatalogSnapshot,
    prepared: &PreparedExport,
    output: &Path,
    versions_output: Option<&Path>,
) -> Result<ExportWriteReport, XcStringsError> {
    let xml_path = super::resolve_export_destination(store, &snapshot.identity, output)?;
    let default_path = default_versions_path(output);
    let requested = versions_output.unwrap_or(&default_path);
    let versions_path = store.file_identity(requested)?;
    if requested.extension().and_then(|value| value.to_str()) != Some("json")
        || versions_path.extension().and_then(|value| value.to_str()) != Some("json")
        || versions_path == xml_path
        || versions_path == snapshot.identity
        || versions_path == snapshot.workflow_path
    {
        return Err(XcStringsError::InvalidPath{path:requested.into(),reason:"source-version output must be a distinct .json file and cannot overwrite the catalog or workflow sidecar".into()});
    }
    let xml_expected = existing_bytes(store, &xml_path)?;
    let versions_expected = existing_bytes(store, &versions_path)?;
    let map = format!(
        "{}\n",
        serde_json::to_string_pretty(&prepared.source_versions)?
    );
    store.write_if_matches(&versions_path, versions_expected.as_deref(), &map)?;
    let mut report = ExportWriteReport {
        output_path: output.display().to_string(),
        source_versions_path: requested.display().to_string(),
        xml_written: false,
        source_versions_written: true,
        phase_error: None,
    };
    match store.write_if_matches(&xml_path, xml_expected.as_deref(), &prepared.xml) {
        Ok(()) => report.xml_written = true,
        Err(error) => {
            report.phase_error = Some(format!(
                "source-version map committed, but XML output was not written: {error}"
            ))
        }
    }
    Ok(report)
}
fn existing_bytes(store: &dyn FileStore, path: &Path) -> Result<Option<Vec<u8>>, XcStringsError> {
    if store.exists(path) {
        store.read_bytes(path).map(Some)
    } else {
        Ok(None)
    }
}
fn default_versions_path(output: &Path) -> PathBuf {
    let mut name = output.as_os_str().to_os_string();
    name.push(".source-versions.json");
    PathBuf::from(name)
}
