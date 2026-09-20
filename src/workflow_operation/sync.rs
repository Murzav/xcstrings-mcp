use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{CatalogSnapshot, InputRevisions};
use crate::model::workflow::{SourceSyncReport, SyncMode};
use crate::service::workflow;
use crate::{FileStore, XcStringsError};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SyncRequest {
    #[serde(default)]
    pub mode: SyncMode,
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
    #[serde(default)]
    pub expected: Option<InputRevisions>,
}

fn default_dry_run() -> bool {
    true
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SyncResult {
    pub report: SourceSyncReport,
    pub input_revisions: InputRevisions,
    pub dry_run: bool,
    pub catalog_written: bool,
    pub checkpoint_written: bool,
    pub retry_required: bool,
    pub phase_error: Option<String>,
}

pub fn synchronize(
    store: &dyn FileStore,
    path: &Path,
    request: &SyncRequest,
) -> Result<SyncResult, XcStringsError> {
    let snapshot = CatalogSnapshot::load(store, path)?;
    if let Some(expected) = &request.expected {
        snapshot.check_revisions(expected)?;
    } else if !request.dry_run {
        return Err(XcStringsError::InvalidFormat(
            "sync apply requires the preview input revisions".into(),
        ));
    }
    let plan = workflow::plan_source_sync(
        snapshot.identity_text()?,
        &snapshot.catalog,
        &snapshot.workflow,
        request.mode,
    )?;
    let mut result = SyncResult {
        input_revisions: snapshot.revisions(),
        dry_run: request.dry_run,
        catalog_written: false,
        checkpoint_written: false,
        retry_required: false,
        phase_error: None,
        report: plan.report.clone(),
    };
    if request.dry_run || !plan.report.checkpoint_needed {
        return Ok(result);
    }
    // Validate the proposed checkpoint before any mutation. The same checks are
    // applied after phase one using the actual candidate bytes guarded below.
    let candidate = plan.invalidation.as_ref().unwrap_or(&snapshot.catalog);
    let document = workflow::plan_checkpoint(candidate, &snapshot.workflow, &plan)?;
    let written_bytes = match &plan.invalidation {
        Some(candidate) => {
            let bytes = snapshot.write_catalog(store, candidate)?;
            result.catalog_written = true;
            Some(bytes)
        }
        None => None,
    };
    let actual_bytes = written_bytes.as_deref().unwrap_or(&snapshot.catalog_bytes);
    match snapshot.write_workflow(store, &document, actual_bytes) {
        Ok(()) => result.checkpoint_written = true,
        Err(error) => {
            result.retry_required = true;
            result.phase_error = Some(error.to_string());
        }
    }
    Ok(result)
}
