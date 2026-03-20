use std::sync::Arc;

use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ProtocolVersion, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};
use tokio::sync::Mutex;
use tracing::error;

use crate::io::FileStore;
use crate::tools::{
    coverage::{GetCoverageParams, ValidateFileParams, handle_get_coverage, handle_validate_file},
    extract::{GetStaleParams, GetUntranslatedParams, handle_get_stale, handle_get_untranslated},
    manage::{AddLocaleParams, ListLocalesParams, handle_add_locale, handle_list_locales},
    parse::{CachedFile, ParseParams, handle_parse},
    translate::{SubmitTranslationsParams, handle_submit_translations},
};

#[derive(Clone)]
pub struct XcStringsMcpServer {
    store: Arc<dyn FileStore>,
    cache: Arc<Mutex<Option<CachedFile>>>,
    write_lock: Arc<Mutex<()>>,
    tool_router: ToolRouter<Self>,
}

impl XcStringsMcpServer {
    pub fn new(store: Arc<dyn FileStore>) -> Self {
        Self {
            store,
            cache: Arc::new(Mutex::new(None)),
            write_lock: Arc::new(Mutex::new(())),
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl XcStringsMcpServer {
    /// Parse an .xcstrings file and return a summary of its contents
    /// including locales, key counts, and translation states.
    #[tool(
        name = "parse_xcstrings",
        description = "Parse an .xcstrings file and return a summary (locales, key counts, states). Must be called before other tools if no file_path is passed."
    )]
    async fn parse_xcstrings(
        &self,
        Parameters(params): Parameters<ParseParams>,
    ) -> Result<String, String> {
        match handle_parse(self.store.as_ref(), &self.cache, params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "parse_xcstrings failed");
                Err(e.to_string())
            }
        }
    }

    /// Get untranslated strings for a target locale with batching support.
    #[tool(
        name = "get_untranslated",
        description = "Get untranslated strings for a target locale. Returns batched results with pagination. Call parse_xcstrings first or pass file_path."
    )]
    async fn get_untranslated(
        &self,
        Parameters(params): Parameters<GetUntranslatedParams>,
    ) -> Result<String, String> {
        match handle_get_untranslated(self.store.as_ref(), &self.cache, params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "get_untranslated failed");
                Err(e.to_string())
            }
        }
    }

    /// Submit translations: validates format specifiers and plural forms,
    /// merges into the file, and writes back atomically.
    #[tool(
        name = "submit_translations",
        description = "Submit translations for review and writing. Validates specifiers/plurals, merges, and writes atomically. Use dry_run=true to validate without writing."
    )]
    async fn submit_translations(
        &self,
        Parameters(params): Parameters<SubmitTranslationsParams>,
    ) -> Result<String, String> {
        match handle_submit_translations(self.store.as_ref(), &self.cache, &self.write_lock, params)
            .await
        {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "submit_translations failed");
                Err(e.to_string())
            }
        }
    }

    /// Get translation coverage statistics per locale.
    #[tool(
        name = "get_coverage",
        description = "Get translation coverage statistics per locale. Shows translated/total counts and percentages for each locale."
    )]
    async fn get_coverage(
        &self,
        Parameters(params): Parameters<GetCoverageParams>,
    ) -> Result<String, String> {
        match handle_get_coverage(self.store.as_ref(), &self.cache, params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "get_coverage failed");
                Err(e.to_string())
            }
        }
    }

    /// Get stale strings (extractionState=stale) for a target locale.
    #[tool(
        name = "get_stale",
        description = "Get strings marked as stale (removed from source code). Returns batched results with pagination."
    )]
    async fn get_stale(
        &self,
        Parameters(params): Parameters<GetStaleParams>,
    ) -> Result<String, String> {
        match handle_get_stale(self.store.as_ref(), &self.cache, params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "get_stale failed");
                Err(e.to_string())
            }
        }
    }

    /// Validate translations in the file for correctness.
    #[tool(
        name = "validate_translations",
        description = "Validate all translations for format specifier mismatches, missing plural forms, empty values, and other issues. Optionally filter by locale."
    )]
    async fn validate_translations_file(
        &self,
        Parameters(params): Parameters<ValidateFileParams>,
    ) -> Result<String, String> {
        match handle_validate_file(self.store.as_ref(), &self.cache, params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "validate_translations failed");
                Err(e.to_string())
            }
        }
    }

    /// List all locales with translation statistics.
    #[tool(
        name = "list_locales",
        description = "List all locales in the file with translation counts and percentages."
    )]
    async fn list_locales(
        &self,
        Parameters(params): Parameters<ListLocalesParams>,
    ) -> Result<String, String> {
        match handle_list_locales(self.store.as_ref(), &self.cache, params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "list_locales failed");
                Err(e.to_string())
            }
        }
    }

    /// Add a new locale to the file.
    #[tool(
        name = "add_locale",
        description = "Add a new locale to the file. Initializes all translatable keys with empty translations (state=new). Writes the file atomically."
    )]
    async fn add_locale(
        &self,
        Parameters(params): Parameters<AddLocaleParams>,
    ) -> Result<String, String> {
        match handle_add_locale(self.store.as_ref(), &self.cache, &self.write_lock, params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "add_locale failed");
                Err(e.to_string())
            }
        }
    }
}

#[tool_handler]
impl ServerHandler for XcStringsMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_protocol_version(ProtocolVersion::V_2025_06_18)
            .with_instructions(
                "MCP server for iOS/macOS .xcstrings (String Catalog) localization files. \
                     Use parse_xcstrings to load a file, get_untranslated to find strings needing \
                     translation, submit_translations to write translations back, get_coverage for \
                     per-locale statistics, get_stale to find removed strings, validate_translations \
                     to check correctness, list_locales to see all locales, and add_locale to add a \
                     new locale.",
            )
    }
}
