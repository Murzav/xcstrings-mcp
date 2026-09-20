use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::guidance_operation::GuidanceSnapshot;
use crate::io::FileStore;
use crate::model::translation::{CompletedTranslation, SubmitResult};
use crate::service::merger;
use crate::tools::parse::CachedFile;
use crate::tools::submit_response;
use crate::tools::{FileCache, mcp_log};
use crate::workflow_operation::CatalogSnapshot;
use std::path::{Path, PathBuf};
mod validation;

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SubmitTranslationsParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Translations to submit. Each entry needs key, locale, and either value (simple strings) or plural_forms (plural keys). Definite format argument components must match; ambiguous percent-in-prose differences are accepted with warnings[].
    pub translations: Vec<CompletedTranslation>,
    /// If true, validate without writing to disk
    #[serde(default)]
    pub dry_run: bool,
    /// If true (default), write accepted translations even when some are rejected.
    /// If false, reject ALL translations when any single one fails validation.
    #[serde(default = "default_true")]
    pub continue_on_error: bool,
}

/// Submit translations: validate, merge, and write back.
pub(crate) async fn handle_submit_translations(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    write_lock: &Mutex<()>,
    glossary_path: &Path,
    params: SubmitTranslationsParams,
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
    if path.extension().and_then(|extension| extension.to_str()) != Some("xcstrings") {
        return Err(XcStringsError::NotXcStrings { path });
    }
    let _write_guard = write_lock.lock().await;
    let snapshot = CatalogSnapshot::load(store, &path)?;
    let identity = snapshot.identity.clone();
    let validation = validation::guarded(&snapshot, &params.translations)?;
    let guidance = GuidanceSnapshot::load(store, glossary_path);
    // The native snapshot remains immutable for both source and sidecar guards.
    let mut file = snapshot.catalog.clone();
    if !params.continue_on_error && !validation.rejected.is_empty() {
        let rejected = params
            .translations
            .iter()
            .enumerate()
            .map(|(index, request)| {
                validation
                    .rejected_indices
                    .iter()
                    .position(|rejected| *rejected == index)
                    .and_then(|position| validation.rejected.get(position))
                    .cloned()
                    .unwrap_or_else(|| {
                        crate::service::submission::reject(
                            request,
                            "batch_rejected",
                            "batch rejected due to other failures",
                        )
                    })
            })
            .collect();
        return response(
            &guidance,
            &file,
            &snapshot.workflow.contexts,
            SubmitResult {
                rejected,
                dry_run: params.dry_run,
                ..Default::default()
            },
            validation.warnings,
        );
    }
    let accepted: Vec<_> = params
        .translations
        .iter()
        .enumerate()
        .filter(|(index, _)| !validation.rejected_indices.contains(index))
        .map(|(_, request)| request.clone())
        .collect();
    let mut result = merger::merge_translations(&mut file, &accepted);
    result.rejected.extend(validation.rejected);
    result.dry_run = params.dry_run;
    if !params.continue_on_error && !result.rejected.is_empty() {
        let rejected = params
            .translations
            .iter()
            .map(|request| {
                result.rejected.iter().find(|rejected| {
                rejected.key == request.key
                    && rejected.locale.as_deref() == Some(request.locale.as_str())
                    && rejected.path == request.path
            }).cloned().unwrap_or_else(|| crate::service::submission::reject(
                request,
                "batch_rejected",
                "batch rejected because combined translations violate catalog constraints",
            ))
            })
            .collect();
        return response(
            &guidance,
            &file,
            &snapshot.workflow.contexts,
            SubmitResult {
                rejected,
                dry_run: params.dry_run,
                ..Default::default()
            },
            validation.warnings,
        );
    }
    if !params.dry_run && result.accepted > 0 {
        snapshot.write_catalog(store, &file)?;
        match store.modified_time(&identity) {
            Ok(modified) => cache.lock().await.insert(
                identity,
                CachedFile {
                    path,
                    content: file.clone(),
                    modified,
                },
            ),
            Err(error) => {
                // The data write committed; discard stale cache state without reporting a false failure.
                cache.lock().await.files.remove(&identity);
                mcp_log(&format!(
                    "Translations saved; cache invalidated because metadata could not be read: {error}"
                ));
            }
        }
    }
    mcp_log(&format!(
        "{} accepted, {} rejected",
        result.accepted,
        result.rejected.len()
    ));
    response(
        &guidance,
        &file,
        &snapshot.workflow.contexts,
        result,
        validation.warnings,
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod cas_tests;

#[cfg(test)]
mod source_guard_tests;

fn response(
    guidance: &GuidanceSnapshot,
    file: &crate::model::xcstrings::XcStringsFile,
    contexts: &crate::model::context::CatalogContexts,
    result: SubmitResult,
    warnings: Vec<crate::model::translation::ValidationIssue>,
) -> Result<serde_json::Value, XcStringsError> {
    let qa = guidance.check_destinations(file, contexts, &result.accepted_destinations);
    let mut value = submit_response::to_value(result, warnings)?;
    value["guidance"] = serde_json::to_value(qa)?;
    Ok(value)
}

#[cfg(test)]
fn captured_params(
    store: &dyn FileStore,
    mut params: SubmitTranslationsParams,
) -> SubmitTranslationsParams {
    if params.translations.is_empty() {
        return params;
    }
    let snapshot = CatalogSnapshot::load(
        store,
        Path::new(
            params
                .file_path
                .as_deref()
                .unwrap_or("/test/file.xcstrings"),
        ),
    )
    .unwrap();
    let view = crate::service::workflow::inspect(
        snapshot.identity_text().unwrap(),
        &snapshot.catalog,
        &snapshot.workflow,
    )
    .unwrap();
    for request in &mut params.translations {
        if let Some(status) = view.keys.get(&request.key) {
            request.expected_source_version = status.source_version.clone();
        }
    }
    params
}

#[cfg(test)]
pub(super) async fn submit_with_captured_versions(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    lock: &Mutex<()>,
    params: SubmitTranslationsParams,
) -> Result<serde_json::Value, XcStringsError> {
    handle_submit_translations(
        store,
        cache,
        lock,
        Path::new("/test/glossary.json"),
        captured_params(store, params),
    )
    .await
}
