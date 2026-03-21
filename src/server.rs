use std::path::PathBuf;
use std::sync::Arc;

use rmcp::{
    RoleServer, ServerHandler,
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    model::{
        GetPromptRequestParams, GetPromptResult, ListPromptsResult, PaginatedRequestParams,
        ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    prompt_handler,
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use tokio::sync::Mutex;
use tracing::error;

use crate::io::FileStore;
use crate::tools::{
    FileCache,
    coverage::{GetCoverageParams, ValidateFileParams, handle_get_coverage, handle_validate_file},
    diff::{GetDiffParams, handle_get_diff},
    extract::{
        GetStaleParams, GetUntranslatedParams, SearchKeysParams, handle_get_stale,
        handle_get_untranslated, handle_search_keys,
    },
    files::{ListFilesParams, handle_list_files},
    glossary::{
        GetGlossaryParams, UpdateGlossaryParams, handle_get_glossary, handle_update_glossary,
    },
    manage::{
        AddLocaleParams, ListLocalesParams, RemoveLocaleParams, handle_add_locale,
        handle_list_locales, handle_remove_locale,
    },
    parse::{ParseParams, handle_parse},
    plural::{GetContextParams, GetPluralsParams, handle_get_context, handle_get_plurals},
    translate::{SubmitTranslationsParams, handle_submit_translations},
    xliff::{ExportXliffParams, ImportXliffParams, handle_export_xliff, handle_import_xliff},
};

#[derive(Clone)]
pub struct XcStringsMcpServer {
    store: Arc<dyn FileStore>,
    cache: Arc<Mutex<FileCache>>,
    write_lock: Arc<Mutex<()>>,
    glossary_path: PathBuf,
    glossary_write_lock: Arc<Mutex<()>>,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

impl XcStringsMcpServer {
    pub fn new(store: Arc<dyn FileStore>, glossary_path: PathBuf) -> Self {
        Self {
            store,
            cache: Arc::new(Mutex::new(FileCache::new())),
            write_lock: Arc::new(Mutex::new(())),
            glossary_path,
            glossary_write_lock: Arc::new(Mutex::new(())),
            tool_router: Self::tool_router(),
            prompt_router: crate::prompts::build_prompt_router(),
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
        context: RequestContext<RoleServer>,
    ) -> Result<String, String> {
        match handle_parse(
            self.store.as_ref(),
            &self.cache,
            params,
            Some(&context.peer),
        )
        .await
        {
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
        context: RequestContext<RoleServer>,
    ) -> Result<String, String> {
        match handle_submit_translations(
            self.store.as_ref(),
            &self.cache,
            &self.write_lock,
            params,
            Some(&context.peer),
        )
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

    /// Search keys by substring pattern (case-insensitive).
    #[tool(
        name = "search_keys",
        description = "Search keys by substring pattern (case-insensitive). Matches both key names and source text. Returns translation units with pagination. Empty pattern returns all translatable keys."
    )]
    async fn search_keys(
        &self,
        Parameters(params): Parameters<SearchKeysParams>,
    ) -> Result<String, String> {
        match handle_search_keys(self.store.as_ref(), &self.cache, params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "search_keys failed");
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
        context: RequestContext<RoleServer>,
    ) -> Result<String, String> {
        match handle_add_locale(
            self.store.as_ref(),
            &self.cache,
            &self.write_lock,
            params,
            Some(&context.peer),
        )
        .await
        {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "add_locale failed");
                Err(e.to_string())
            }
        }
    }

    /// Remove a locale from the file.
    #[tool(
        name = "remove_locale",
        description = "Remove a locale from the file. Deletes all translations for that locale from every entry. Cannot remove the source locale. Writes the file atomically."
    )]
    async fn remove_locale(
        &self,
        Parameters(params): Parameters<RemoveLocaleParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<String, String> {
        match handle_remove_locale(
            self.store.as_ref(),
            &self.cache,
            &self.write_lock,
            params,
            Some(&context.peer),
        )
        .await
        {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "remove_locale failed");
                Err(e.to_string())
            }
        }
    }

    /// Get keys requiring plural/device translation for a locale.
    #[tool(
        name = "get_plurals",
        description = "Get keys needing plural or device-variant translation. Returns plural forms needed, existing translations, and required CLDR forms. Use for translating plurals/substitutions/device variants."
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
        description = "Get nearby keys sharing a common prefix with the given key. Helps translators understand context by seeing related strings and their translations."
    )]
    async fn get_context(
        &self,
        Parameters(params): Parameters<GetContextParams>,
    ) -> Result<String, String> {
        match handle_get_context(self.store.as_ref(), &self.cache, params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "get_context failed");
                Err(e.to_string())
            }
        }
    }

    /// List all cached .xcstrings files.
    #[tool(
        name = "list_files",
        description = "List all cached .xcstrings files with source language, key count, and active status."
    )]
    async fn list_files(
        &self,
        Parameters(_params): Parameters<ListFilesParams>,
    ) -> Result<String, String> {
        match handle_list_files(&self.cache).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "list_files failed");
                Err(e.to_string())
            }
        }
    }

    /// Compare cached file with current on-disk version.
    #[tool(
        name = "get_diff",
        description = "Compare cached file with current on-disk version. Shows added keys, removed keys, and keys whose source language text changed. Does not track translation changes in non-source locales."
    )]
    async fn get_diff(
        &self,
        Parameters(params): Parameters<GetDiffParams>,
    ) -> Result<String, String> {
        match handle_get_diff(self.store.as_ref(), &self.cache, params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "get_diff failed");
                Err(e.to_string())
            }
        }
    }

    /// Get glossary entries for a language pair.
    #[tool(
        name = "get_glossary",
        description = "Get glossary entries for a source/target locale pair. The glossary persists across sessions and stores preferred translations for terms. Supports optional substring filter."
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
        description = "Add or update glossary entries for a source/target locale pair. The glossary persists across sessions. Upserts entries — existing terms are overwritten, new terms are added."
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
        description = "Export translations to XLIFF 1.2 format for external tools. Only exports simple strings; plural forms not included. By default exports untranslated strings only."
    )]
    async fn export_xliff(
        &self,
        Parameters(params): Parameters<ExportXliffParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<String, String> {
        match handle_export_xliff(
            self.store.as_ref(),
            &self.cache,
            params,
            Some(&context.peer),
        )
        .await
        {
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
        description = "Import translations from XLIFF 1.2 file. Only simple strings imported; use submit_translations for plurals. Validates specifiers, merges accepted. Use dry_run=true to preview."
    )]
    async fn import_xliff(
        &self,
        Parameters(params): Parameters<ImportXliffParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<String, String> {
        match handle_import_xliff(
            self.store.as_ref(),
            &self.cache,
            &self.write_lock,
            params,
            Some(&context.peer),
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

#[tool_handler]
#[prompt_handler]
impl ServerHandler for XcStringsMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
        )
            .with_protocol_version(ProtocolVersion::V_2025_06_18)
            .with_instructions(
                "MCP server for iOS/macOS .xcstrings (String Catalog) localization files. \
                     Use parse_xcstrings to load a file, get_untranslated to find strings needing \
                     translation, get_plurals for plural/device variant keys, get_context for nearby \
                     related keys, submit_translations to write translations back, get_coverage for \
                     per-locale statistics, get_stale to find removed strings, validate_translations \
                     to check correctness, list_locales to see all locales, add_locale to add a \
                     new locale, remove_locale to remove a locale, list_files to see all cached files, \
                     get_diff to compare cached vs on-disk versions, get_glossary to retrieve \
                     glossary terms, update_glossary to add or update glossary entries, \
                     export_xliff to export translations to XLIFF 1.2, import_xliff to \
                     import translations from XLIFF files, and search_keys to find keys by \
                     substring pattern matching key names and source text.",
            )
    }
}
