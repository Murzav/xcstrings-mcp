use super::XcStringsMcpServer;
use crate::tools::{
    glossary::{
        GetGlossaryParams, UpdateGlossaryParams, handle_get_glossary, handle_update_glossary,
    },
    xliff::{ExportXliffParams, ImportXliffParams, handle_export_xliff, handle_import_xliff},
};
use rmcp::{handler::server::wrapper::Parameters, tool, tool_router};
use tracing::error;

#[tool_router(router = exchange_tool_router, vis = "pub(super)")]
impl XcStringsMcpServer {
    /// Get glossary entries for a language pair.
    #[tool(
        name = "get_glossary",
        description = "Read structured glossary rules and revision for a source/target locale pair, with preferred/forbidden/do-not-translate terms, variants and scoped exceptions. Legacy glossary data reads without rewriting. Supports optional substring filter."
    )]
    async fn get_glossary(
        &self,
        Parameters(params): Parameters<GetGlossaryParams>,
    ) -> Result<String, String> {
        match handle_get_glossary(self.store.as_ref(), &self.glossary_path, params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "get_glossary failed");
                Err(e.to_string())
            }
        }
    }

    /// Update glossary entries for a language pair.
    #[tool(
        name = "update_glossary",
        description = "Preview or update glossary rules with upsert/remove_ids or legacy entries. Apply requires expected_revision captured by get_glossary or preview; malformed policy is never overwritten. Legacy storage migrates only on explicit update. Unknown rule metadata survives."
    )]
    async fn update_glossary(
        &self,
        Parameters(params): Parameters<UpdateGlossaryParams>,
    ) -> Result<String, String> {
        match handle_update_glossary(
            self.store.as_ref(),
            &self.glossary_path,
            &self.glossary_write_lock,
            params,
        )
        .await
        {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "update_glossary failed");
                Err(e.to_string())
            }
        }
    }

    /// Export translations to XLIFF 1.2 format for external tools.
    #[tool(
        name = "export_xliff",
        description = "Export supported Apple String Catalog leaves to XLIFF 1.2, including plural, all seven device categories, substitutions, and supported chains. By default exports incomplete leaves; set untranslated_only=false for all. original sets the exact file scope and defaults to the catalog filename. Returns source_versions captured from this same export; preserve that map for import expected_source_versions. exported_count counts trans-units, not catalog keys. Rejects unsafe shapes, ambiguous IDs, literal keys resembling valid variation IDs, and unsafe substitution names before writing. Xcode may lose newly introduced target-only substitutions; compare content after external import."
    )]
    async fn export_xliff(
        &self,
        Parameters(params): Parameters<ExportXliffParams>,
    ) -> Result<String, String> {
        match handle_export_xliff(self.store.as_ref(), &self.cache, params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "export_xliff failed");
                Err(e.to_string())
            }
        }
    }

    /// Import translations from XLIFF 1.2 file.
    #[tool(
        name = "import_xliff",
        description = "Atomically import supported Apple XLIFF 1.2 translation leaves. Requires expected_source_versions captured during export; meaningful XML source text must also match current source. Ready targets require a current source checkpoint; synchronize before export. Select exact original when multiple file scopes exist; skipped_scopes reports unselected files. Preserve draft new/needs-review text and state; translated + leveraged-mt maps to machine_translated. Missing target is a no-op; explicit empty target intentionally clears a leaf. Resolve empty keys and variation IDs in catalog context. accepted counts trans-units; accepted_destinations identifies original/key/locale/path/unit_id. Any rejected unit or stale conditional write prevents the whole selected import and leaves cache unchanged. Strict structure, namespace, duplicate ID, format, and state validation remains; XLIFF 2.x and opaque inline placeholders are unsupported. Use dry_run=true, inspect rejected/warnings, then apply."
    )]
    async fn import_xliff(
        &self,
        Parameters(params): Parameters<ImportXliffParams>,
    ) -> Result<String, String> {
        match handle_import_xliff(
            self.store.as_ref(),
            &self.cache,
            &self.write_lock,
            &self.glossary_path,
            params,
        )
        .await
        {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "import_xliff failed");
                Err(e.to_string())
            }
        }
    }
}
