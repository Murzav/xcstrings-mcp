use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use super::{FileCache, parse::CachedFile};
use crate::model::{
    context::ContextEdit,
    workflow::{ApprovalRequest, ReviewQuery},
};
use crate::service::{context, workflow};
use crate::workflow_operation::{
    CatalogSnapshot, InputRevisions,
    sync::{SyncRequest, synchronize},
};
use crate::{FileStore, XcStringsError};

#[cfg(test)]
mod tests;

pub(crate) async fn resolve_snapshot(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    file_path: Option<&str>,
) -> Result<CatalogSnapshot, XcStringsError> {
    let path = resolve_path(cache, file_path).await?;
    CatalogSnapshot::load(store, &path)
}

pub(crate) async fn resolve_read_snapshot(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    file_path: Option<&str>,
) -> Result<CatalogSnapshot, XcStringsError> {
    let snapshot = resolve_snapshot(store, cache, file_path).await?;
    let modified = store.modified_time(&snapshot.identity)?;
    // Cache the native catalog, never the read-only stale-state overlay.
    cache.lock().await.insert(
        snapshot.identity.clone(),
        CachedFile {
            path: snapshot.display_path.clone(),
            content: snapshot.catalog.clone(),
            modified,
        },
    );
    Ok(snapshot)
}

pub(crate) fn read_result(
    value: impl serde::Serialize,
    snapshot: &CatalogSnapshot,
    view: &workflow::WorkflowView<'_>,
) -> Result<Value, XcStringsError> {
    let mut value = serde_json::to_value(value)?;
    let object = value.as_object_mut().ok_or_else(|| {
        XcStringsError::InvalidFormat("workflow read result must be an object".into())
    })?;
    object.insert("tracking".into(), serde_json::to_value(view.tracking)?);
    object.insert(
        "input_revisions".into(),
        serde_json::to_value(snapshot.revisions())?,
    );
    Ok(value)
}

async fn resolve_path(
    cache: &Mutex<FileCache>,
    file_path: Option<&str>,
) -> Result<PathBuf, XcStringsError> {
    match file_path {
        Some(path) => Ok(PathBuf::from(path)),
        None => cache
            .lock()
            .await
            .active_path()
            .cloned()
            .ok_or(XcStringsError::NoActiveFile),
    }
}

pub(crate) async fn refresh_after_write(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    snapshot: &CatalogSnapshot,
    candidate: crate::XcStringsFile,
) {
    match store.modified_time(&snapshot.identity) {
        Ok(modified) => cache.lock().await.insert(
            snapshot.identity.clone(),
            CachedFile {
                path: snapshot.display_path.clone(),
                content: candidate,
                modified,
            },
        ),
        Err(error) => {
            cache.lock().await.files.remove(&snapshot.identity);
            super::mcp_log(&format!(
                "Catalog saved; invalidating cache after metadata failure: {error}"
            ));
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ReviewQueueParams {
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(flatten)]
    pub query: ReviewQuery,
}

pub(crate) async fn handle_review_queue(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    params: ReviewQueueParams,
) -> Result<Value, XcStringsError> {
    let snapshot = resolve_snapshot(store, cache, params.file_path.as_deref()).await?;
    let page = workflow::review_queue(
        snapshot.identity_text()?,
        &snapshot.catalog,
        &snapshot.workflow,
        &params.query,
    )?;
    Ok(serde_json::to_value(page)?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ApproveTranslationsParams {
    #[serde(default)]
    pub file_path: Option<String>,
    /// Exact existing drafts with source and physical target versions from the queue.
    pub approvals: Vec<ApprovalRequest>,
    #[serde(default)]
    pub dry_run: bool,
}

pub(crate) async fn handle_approve_translations(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    write_lock: &Mutex<()>,
    glossary_path: &Path,
    params: ApproveTranslationsParams,
) -> Result<Value, XcStringsError> {
    let _guard = write_lock.lock().await;
    let snapshot = resolve_snapshot(store, cache, params.file_path.as_deref()).await?;
    let plan = workflow::plan_approval(
        snapshot.identity_text()?,
        &snapshot.catalog,
        &snapshot.workflow,
        &params.approvals,
    )?;
    let terminology = crate::guidance_operation::GuidanceSnapshot::load(store, glossary_path)
        .check_destinations(
            &snapshot.catalog,
            &snapshot.workflow.contexts,
            &plan.report.accepted_destinations,
        );
    let mut written = false;
    if !params.dry_run
        && let Some(candidate) = plan.candidate
    {
        snapshot.write_catalog(store, &candidate)?;
        refresh_after_write(store, cache, &snapshot, candidate).await;
        written = true;
    }
    Ok(
        json!({"report":plan.report,"dry_run":params.dry_run,"written":written,"input_revisions":snapshot.revisions(),"terminology":terminology}),
    )
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct SyncSourceParams {
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(flatten)]
    pub request: SyncRequest,
}

pub(crate) async fn handle_sync_source_changes(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    write_lock: &Mutex<()>,
    params: SyncSourceParams,
) -> Result<Value, XcStringsError> {
    let _guard = write_lock.lock().await;
    let path = resolve_path(cache, params.file_path.as_deref()).await?;
    let identity = store.file_identity(&path)?;
    let result = synchronize(store, &path, &params.request)?;
    if result.catalog_written {
        // Even if checkpoint phase failed, the catalog changed. Evict rather than
        // presenting the pre-invalidation cached state as current.
        match CatalogSnapshot::load(store, &path) {
            Ok(snapshot) => {
                refresh_after_write(store, cache, &snapshot, snapshot.catalog.clone()).await
            }
            Err(_) => {
                cache.lock().await.files.remove(&identity);
            }
        }
    }
    Ok(serde_json::to_value(result)?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateContextParams {
    #[serde(default)]
    pub file_path: Option<String>,
    /// Whole-record set/remove operations. Fetch, preserve unknown fields, then set.
    pub edits: Vec<ContextEdit>,
    /// Captured catalog and workflow byte revisions from get_context/dry-run.
    #[serde(default)]
    pub expected: Option<InputRevisions>,
    #[serde(default)]
    pub dry_run: bool,
}

pub(crate) async fn handle_update_context(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    write_lock: &Mutex<()>,
    params: UpdateContextParams,
) -> Result<Value, XcStringsError> {
    let _guard = write_lock.lock().await;
    let snapshot = resolve_snapshot(store, cache, params.file_path.as_deref()).await?;
    if let Some(expected) = &params.expected {
        snapshot.check_revisions(expected)?;
    } else if !params.dry_run {
        return Err(XcStringsError::InvalidFormat(
            "context apply requires captured input revisions".into(),
        ));
    }
    let revisions = snapshot.revisions();
    match context::apply_context_edits(
        &snapshot.catalog,
        &snapshot.workflow.contexts,
        &params.edits,
    ) {
        Err(diagnostics) => Ok(
            json!({"written":false,"dry_run":params.dry_run,"rejected":diagnostics,"input_revisions":revisions}),
        ),
        Ok(contexts) => {
            let changed = contexts != snapshot.workflow.contexts;
            let mut candidate = snapshot.workflow.clone();
            candidate.contexts = contexts;
            if changed && !params.dry_run {
                snapshot.write_workflow(store, &candidate, &snapshot.catalog_bytes)?;
            }
            Ok(
                json!({"written":changed && !params.dry_run,"dry_run":params.dry_run,"changed":changed,"rejected":[],"input_revisions":revisions}),
            )
        }
    }
}
