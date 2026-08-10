use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::service::merge_operation::{MergeRequest, execute_merge};
use crate::service::semantic_merge::{ConflictResolution, ExpectedFingerprints, MergeReport};
use crate::tools::parse::CachedFile;
use crate::tools::{FileCache, mcp_log};

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct MergeXcStringsParams {
    /// Common ancestor catalog. Always read fresh; the active cache is ignored.
    pub base_path: String,
    /// Catalog containing the current branch's edits.
    pub current_path: String,
    /// Catalog containing the incoming branch's edits.
    pub incoming_path: String,
    /// Destination catalog. May be the same path as current_path.
    pub output_path: String,
    /// Preview only. Apply requires false plus expected_fingerprints from a fresh dry-run.
    #[serde(default = "default_true")]
    pub dry_run: bool,
    /// Conflict decisions from a previous report. Values can only select current, incoming, or base.
    #[serde(default)]
    pub resolutions: Vec<ConflictResolution>,
    /// Exact input/output SHA-256 values returned by dry-run. Required for apply.
    #[serde(default)]
    pub expected_fingerprints: Option<ExpectedFingerprints>,
    /// Zero-based offset into unresolved conflicts.
    #[serde(default)]
    pub conflict_offset: usize,
    /// Number of unresolved conflicts to return (1 through 500).
    #[serde(default = "default_conflict_limit")]
    #[schemars(range(min = 1, max = 500))]
    pub conflict_limit: usize,
}

fn default_true() -> bool {
    true
}

fn default_conflict_limit() -> usize {
    50
}

pub(crate) async fn handle_merge_xcstrings(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    write_lock: &Mutex<()>,
    params: MergeXcStringsParams,
) -> Result<MergeReport, XcStringsError> {
    let request = MergeRequest {
        base_path: PathBuf::from(&params.base_path),
        current_path: PathBuf::from(&params.current_path),
        incoming_path: PathBuf::from(&params.incoming_path),
        output_path: PathBuf::from(&params.output_path),
        dry_run: params.dry_run,
        resolutions: params.resolutions,
        expected_fingerprints: params.expected_fingerprints,
        conflict_offset: params.conflict_offset,
        conflict_limit: params.conflict_limit,
    };

    // One server-level writer at a time. The operation still uses filesystem
    // CAS because independent MCP/CLI processes do not share this mutex.
    let _write_guard = write_lock.lock().await;
    let execution = execute_merge(store, &request)?;
    if execution.report.written {
        let modified = store.modified_time(&request.output_path)?;
        let identity = store.file_identity(&request.output_path)?;
        let cached = CachedFile {
            path: request.output_path.clone(),
            content: execution.parsed,
            modified,
        };
        cache.lock().await.replace_if_cached(identity, cached);
        mcp_log(&format!(
            "Merged catalogs into {} ({} keys)",
            request.output_path.display(),
            execution.report.fingerprints.result.key_count
        ));
    }
    Ok(execution.report)
}
