use super::XcStringsMcpServer;
use crate::tools::plural::{
    GetContextParams, GetPluralsParams, handle_get_context, handle_get_plurals,
};
use crate::tools::workflow::{
    self, ApproveTranslationsParams, ReviewQueueParams, SyncSourceParams, UpdateContextParams,
};
use rmcp::{handler::server::wrapper::Parameters, tool, tool_router};
use tracing::error;

fn response(value: Result<serde_json::Value, crate::XcStringsError>) -> Result<String, String> {
    value
        .map_err(|error| error.to_string())
        .and_then(|value| serde_json::to_string_pretty(&value).map_err(|error| error.to_string()))
}

#[tool_router(router = workflow_tool_router, vis = "pub(super)")]
impl XcStringsMcpServer {
    #[tool(
        name = "get_review_queue",
        description = "List existing draft and source-changed translation leaves with exact source/target versions and preserved change evidence. Later offset pages require expected_queue_version. After changing or approving entries restart at offset zero. Missing targets are translation work, not approval items."
    )]
    async fn get_review_queue(
        &self,
        Parameters(params): Parameters<ReviewQueueParams>,
    ) -> Result<String, String> {
        response(workflow::handle_review_queue(self.store.as_ref(), &self.cache, params).await)
    }

    #[tool(
        name = "approve_translations",
        description = "Explicitly approve existing new/needs_review leaves without changing text. Requires current source checkpoint and exact captured source/physical-target versions. Entire batch rejects on any failure. Reusing a token after approval rejects as stale; get a fresh queue before another review. Use dry_run for preview."
    )]
    async fn approve_translations(
        &self,
        Parameters(params): Parameters<ApproveTranslationsParams>,
    ) -> Result<String, String> {
        response(
            workflow::handle_approve_translations(
                self.store.as_ref(),
                &self.cache,
                &self.write_lock,
                &self.glossary_path,
                params,
            )
            .await,
        )
    }

    #[tool(
        name = "sync_source_changes",
        description = "Preview then synchronize source/context changes: retain target text, mark affected ready leaves needs_review, then checkpoint source snapshots. Apply requires exact input revisions from dry_run. First initialization defaults to review; explicit adopt_existing preserves native states with unknown historical freshness. Inspect catalog_written/checkpoint_written/retry_required: two guarded file writes are not one transaction."
    )]
    async fn sync_source_changes(
        &self,
        Parameters(params): Parameters<SyncSourceParams>,
    ) -> Result<String, String> {
        response(
            workflow::handle_sync_source_changes(
                self.store.as_ref(),
                &self.cache,
                &self.write_lock,
                params,
            )
            .await,
        )
    }

    #[tool(
        name = "update_context",
        description = "Set or remove authored context records: screen, UI role, purpose, variable meanings and explicit neighbors, with leaf overrides. Set replaces the whole addressed record; preserve unknown fields via get-modify-set. Apply requires captured catalog/workflow revisions. Context edits make tracked translations require review; no facts are inferred."
    )]
    async fn update_context(
        &self,
        Parameters(params): Parameters<UpdateContextParams>,
    ) -> Result<String, String> {
        response(
            workflow::handle_update_context(
                self.store.as_ref(),
                &self.cache,
                &self.write_lock,
                params,
            )
            .await,
        )
    }
    /// Get keys requiring plural/device translation for a locale.
    #[tool(
        name = "get_plurals",
        description = "Get keys needing plural or device-variant translation. format_specifiers contains only definite Foundation arguments; percent-in-prose ambiguities are diagnosed during validation. Returns required CLDR forms per locale (e.g., one/few/many/other for Ukrainian), existing partial translations, and substitution info. All substitutions and nested devices appear in leaves with typed paths and states. Submit individual leaves using path, or direct aggregate plural_forms where unambiguous; partial submissions need not complete every CLDR category."
    )]
    async fn get_plurals(
        &self,
        Parameters(params): Parameters<GetPluralsParams>,
    ) -> Result<String, String> {
        match handle_get_plurals(self.store.as_ref(), &self.cache, params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "get_plurals failed");
                Err(e.to_string())
            }
        }
    }

    /// Get nearby context keys for a translation key.
    #[tool(
        name = "get_context",
        description = "Get a current-key context package: source/version, developer comment, authored screen/role/purpose, observed variables and authored meanings, applicable terms, leaf overrides and bounded neighbors with provenance. Unknown facts remain absent. authored_contexts supports get-modify-set with update_context."
    )]
    async fn get_context(
        &self,
        Parameters(params): Parameters<GetContextParams>,
    ) -> Result<String, String> {
        match handle_get_context(
            self.store.as_ref(),
            &self.cache,
            &self.glossary_path,
            params,
        )
        .await
        {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "get_context failed");
                Err(e.to_string())
            }
        }
    }
}
