use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::tools::parse::CachedFile;
use crate::tools::{FileCache, mcp_log};
use crate::workflow_operation::CatalogSnapshot;
use crate::xliff_operation::{
    ImportOptions, execute_import, prepare_export, resolve_export_destination,
};

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
    source_versions: BTreeMap<String, String>,
}

/// Export every supported Apple translation leaf with its exact variation identity.
pub(crate) async fn handle_export_xliff(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    params: ExportXliffParams,
) -> Result<serde_json::Value, XcStringsError> {
    let path = match &params.file_path {
        Some(path) => PathBuf::from(path),
        None => cache
            .lock()
            .await
            .active_path()
            .cloned()
            .ok_or(XcStringsError::NoActiveFile)?,
    };
    let snapshot = CatalogSnapshot::load(store, &path)?;

    let original = params.original.as_deref().unwrap_or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Localizable.xcstrings")
    });

    let prepared = prepare_export(
        &snapshot,
        &params.locale,
        original,
        params.untranslated_only,
    )?;
    let count = prepared.exported_count;

    let output_path =
        resolve_export_destination(store, &path, &PathBuf::from(&params.output_path))?;
    let expected = if store.exists(&output_path) {
        Some(store.read_bytes(&output_path)?)
    } else {
        None
    };
    store.write_if_matches(&output_path, expected.as_deref(), &prepared.xml)?;

    mcp_log(&format!("Exported {count} translation leaves to XLIFF"));

    let result = ExportResult {
        output_path: params.output_path,
        locale: params.locale,
        exported_count: count,
        source_versions: prepared.source_versions,
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
    /// Source versions captured when exporting; required for every target-bearing key.
    pub expected_source_versions: BTreeMap<String, String>,
    /// If true, validate without writing
    #[serde(default)]
    pub dry_run: bool,
}

/// Import one selected Apple catalog scope as an all-or-nothing transaction.
pub(crate) async fn handle_import_xliff(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    write_lock: &Mutex<()>,
    glossary_path: &Path,
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
        ImportOptions {
            original: params.original.as_deref(),
            dry_run: params.dry_run,
            expected_source_versions: &params.expected_source_versions,
        },
        glossary_path,
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
#[path = "xliff/tests.rs"]
mod tests;

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

/// Existing XML behavior fixtures explicitly adopt and capture source inputs at
/// setup. Revision/race tests call the real handler with previously captured data.
#[cfg(test)]
pub(super) async fn import_with_current_checkpoint(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    lock: &Mutex<()>,
    mut params: ImportXliffParams,
) -> Result<serde_json::Value, XcStringsError> {
    let path = match &params.file_path {
        Some(path) => PathBuf::from(path),
        None => cache
            .lock()
            .await
            .active_path()
            .cloned()
            .ok_or(XcStringsError::NoActiveFile)?,
    };
    let mut snapshot = CatalogSnapshot::load(store, &path)?;
    if snapshot.workflow.source_baseline.is_none() {
        let plan = crate::service::workflow::plan_source_sync(
            snapshot.identity_text()?,
            &snapshot.catalog,
            &snapshot.workflow,
            crate::model::workflow::SyncMode::AdoptExisting,
        )?;
        let checkpoint = crate::service::workflow::plan_checkpoint(
            &snapshot.catalog,
            &snapshot.workflow,
            &plan,
        )?;
        store.write(
            &snapshot.workflow_path,
            &crate::service::workflow::format_document(&checkpoint)?,
        )?;
        snapshot = CatalogSnapshot::load(store, &path)?;
    }
    params.expected_source_versions = crate::service::workflow::inspect(
        snapshot.identity_text()?,
        &snapshot.catalog,
        &snapshot.workflow,
    )?
    .keys
    .into_iter()
    .map(|(key, status)| (key, status.source_version))
    .collect();
    handle_import_xliff(store, cache, lock, Path::new("/test/glossary.json"), params).await
}
