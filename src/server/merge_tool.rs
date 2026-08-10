use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    tool, tool_router,
};
use tracing::error;

use super::XcStringsMcpServer;
use crate::service::semantic_merge::MergeReport;
use crate::tools::merge::{MergeXcStringsParams, handle_merge_xcstrings};

#[tool_router(router = merge_tool_router, vis = "pub(super)")]
impl XcStringsMcpServer {
    /// Three-way semantic merge for complete String Catalogs.
    #[tool(
        name = "merge_xcstrings",
        description = "Conservatively merge base/current/incoming .xcstrings catalogs using ordered raw JSON. Known schema maps merge recursively; stringUnit and unknown subtrees are atomic. Start with dry_run=true, resolve stable conflict IDs with current/incoming/base choices, then apply with dry_run=false and the exact expected_fingerprints returned by dry-run. Apply rejects unresolved conflicts, new blocking validation issues, stale inputs, and stale/missing/unexpected output. Writes use cooperating-writer advisory locking plus exact-byte CAS; live catalog aliases cannot resolve to internal sidecars or non-.xcstrings files. External writers that ignore the lock remain outside that guarantee and can still race."
    )]
    async fn merge_xcstrings(
        &self,
        Parameters(params): Parameters<MergeXcStringsParams>,
    ) -> Result<Json<MergeReport>, String> {
        match handle_merge_xcstrings(self.store.as_ref(), &self.cache, &self.write_lock, params)
            .await
        {
            Ok(report) => Ok(Json(report)),
            Err(error) => {
                error!(error = %error, "merge_xcstrings failed");
                Err(error.to_string())
            }
        }
    }
}
