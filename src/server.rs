use std::path::PathBuf;
use std::sync::Arc;

use rmcp::{
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    tool, tool_router,
};
use tokio::sync::Mutex;
use tracing::error;

use crate::io::FileStore;
use crate::tools::{
    FileCache,
    coverage::{GetCoverageParams, ValidateFileParams, handle_get_coverage, handle_validate_file},
    create::{
        AddKeysParams, CreateXcStringsParams, UpdateCommentsParams, handle_add_keys,
        handle_create_xcstrings, handle_update_comments,
    },
    diff::{GetDiffParams, handle_get_diff},
    extract::{
        GetKeyParams, GetStaleParams, GetUntranslatedParams, SearchKeysParams, handle_get_key,
        handle_get_stale, handle_get_untranslated, handle_search_keys,
    },
    files::{DiscoverFilesParams, ListFilesParams, handle_discover_files, handle_list_files},
    keys::{
        DeleteKeysParams, DeleteTranslationsParams, RenameKeyParams, handle_delete_keys,
        handle_delete_translations, handle_rename_key,
    },
    manage::{
        AddLocaleParams, ListLocalesParams, RemoveLocaleParams, handle_add_locale,
        handle_list_locales, handle_remove_locale,
    },
    parse::{ParseParams, handle_parse},
    strings::{ImportStringsParams, handle_import_strings},
    translate::{SubmitTranslationsParams, handle_submit_translations},
};

mod exchange_tools;
mod merge_tool;
mod protocol;
mod workflow_tools;

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
        let mut tool_router = Self::tool_router();
        tool_router.merge(Self::merge_tool_router());
        tool_router.merge(Self::workflow_tool_router());
        tool_router.merge(Self::exchange_tool_router());
        Self {
            store,
            cache: Arc::new(Mutex::new(FileCache::new())),
            write_lock: Arc::new(Mutex::new(())),
            glossary_path,
            glossary_write_lock: Arc::new(Mutex::new(())),
            tool_router,
            prompt_router: Self::prompt_router(),
        }
    }
}

#[tool_router]
impl XcStringsMcpServer {
    /// Parse an .xcstrings file and return a summary of its contents
    /// including locales, key counts, and translation states.
    #[tool(
        name = "parse_xcstrings",
        description = "Parse an .xcstrings file and cache it. Returns summary: locales, key counts, extraction states. The parsed file becomes the active file — other tools use it automatically when file_path is omitted."
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
        description = "Get untranslated strings for one or more target locales. format_specifiers contains only definite Foundation arguments; percent-in-prose ambiguities are excluded and diagnosed during validation. Returns batched results. Capture a finite worklist before writes; drafts stay incomplete and must not be resubmitted in a loop. Each key carries source_version for expected_source_version. Each key includes all recursive leaves with typed paths, current states, completeness and diagnostics. Plural completeness uses CLDR recommendations; draft states are incomplete, translated or machine_translated explicit blanks are ready."
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

    /// Submit translations: validates format arguments and plural forms,
    /// merges into the file, and writes back atomically.
    #[tool(
        name = "submit_translations",
        description = "Save translations as needs_review drafts with validation and atomic writing. Each request requires expected_source_version captured when reading its source; stale inputs reject. Separate approve_translations marks reviewed leaves ready. Terminology checks are advisory. Definite Foundation format arguments must preserve position, conversion, length modifier (including integer j), flags, width, and precision; valid positional reordering is allowed, including next to unspaced Han, Hiragana, Katakana, or Hangul text. Invalid positional indices block. Named substitution forms require exact %arg tokens, rejecting longer Unicode words while permitting those unspaced-script adjacencies. Percent sequences that can also be prose are accepted only with machine-readable warnings[]. Use path=[] for the root or typed device/plural/substitution paths for individual leaves; do not combine path with plural_forms or substitution_name. Partial updates are valid; duplicate or overlapping destinations reject every involved request. accepted counts requests and accepted_destinations identifies written leaves. Explicit blanks are intentional. Use dry_run=true first. Check rejected[] for blocking failures and warnings[] for accepted ambiguities. Set continue_on_error=false to reject the entire batch on any blocking failure."
    )]
    async fn submit_translations(
        &self,
        Parameters(params): Parameters<SubmitTranslationsParams>,
    ) -> Result<String, String> {
        match handle_submit_translations(
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
        description = "Get strings marked as stale (removed from source code but still in the file). Returned format_specifiers contains only definite Foundation arguments, never percent-in-prose ambiguities. Returns batched results with pagination. Use delete_keys to remove confirmed stale keys."
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
        description = "Search keys by substring pattern (case-insensitive). Matches both key names and source text. Returned format_specifiers contains only definite Foundation arguments, never percent-in-prose ambiguities. Returns translation units with pagination. Empty pattern returns all translatable keys."
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
        description = "Validate simple, plural, and substitution translations with the same source resolver and format comparator used by submit_translations. Definite Foundation argument mismatches and invalid positional indices are errors, including arguments next to unspaced Han, Hiragana, Katakana, or Hangul text; named substitution forms require exact %arg tokens and reject longer Unicode words. Ambiguous percent-in-prose differences are warnings. Returns an object with reports, advisory terminology, tracking, source_changed_keys, untracked_keys and input_revisions. Also reports missing plural forms and empty values. Optionally filter by locale."
    )]
    async fn validate_translations_file(
        &self,
        Parameters(params): Parameters<ValidateFileParams>,
    ) -> Result<String, String> {
        match handle_validate_file(
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

    /// Remove a locale from the file.
    #[tool(
        name = "remove_locale",
        description = "Remove a locale from the file. Deletes all translations for that locale from every entry. Cannot remove the source locale. Writes the file atomically."
    )]
    async fn remove_locale(
        &self,
        Parameters(params): Parameters<RemoveLocaleParams>,
    ) -> Result<String, String> {
        match handle_remove_locale(self.store.as_ref(), &self.cache, &self.write_lock, params).await
        {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "remove_locale failed");
                Err(e.to_string())
            }
        }
    }

    /// List all cached .xcstrings files.
    #[tool(
        name = "list_files",
        description = "List all previously parsed .xcstrings files in memory. Shows source language, key count, and which file is active (used when file_path is omitted from other tool calls)."
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

    /// Import legacy .strings and .stringsdict files into .xcstrings format.
    #[tool(
        name = "import_strings",
        description = "Import legacy .strings and .stringsdict files into .xcstrings format. Provide file paths directly or a directory to scan for .lproj folders. Handles UTF-8 and UTF-16 encodings, plural rules, and comments. Creates new .xcstrings or merges into existing. Use dry_run=true to preview."
    )]
    async fn import_strings(
        &self,
        Parameters(params): Parameters<ImportStringsParams>,
    ) -> Result<String, String> {
        match handle_import_strings(self.store.as_ref(), &self.cache, &self.write_lock, params)
            .await
        {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "import_strings failed");
                Err(e.to_string())
            }
        }
    }

    /// Create a new empty .xcstrings file.
    #[tool(
        name = "create_xcstrings",
        description = "Create a new empty .xcstrings file with the given source language. Fails if the file already exists."
    )]
    async fn create_xcstrings(
        &self,
        Parameters(params): Parameters<CreateXcStringsParams>,
    ) -> Result<String, String> {
        match handle_create_xcstrings(self.store.as_ref(), &self.cache, params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "create_xcstrings failed");
                Err(e.to_string())
            }
        }
    }

    /// Add new localization keys with source text.
    #[tool(
        name = "add_keys",
        description = "Add new localization keys with source text to the .xcstrings file. Skips keys that already exist. Writes atomically."
    )]
    async fn add_keys(
        &self,
        Parameters(params): Parameters<AddKeysParams>,
    ) -> Result<String, String> {
        match handle_add_keys(self.store.as_ref(), &self.cache, &self.write_lock, params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "add_keys failed");
                Err(e.to_string())
            }
        }
    }

    /// Discover localization files in a directory tree.
    #[tool(
        name = "discover_files",
        description = "Recursively search a directory for localization files. Returns .xcstrings files and legacy .strings/.stringsdict files found in .lproj directories. Use legacy_files to identify projects that can be migrated with import_strings."
    )]
    async fn discover_files(
        &self,
        Parameters(params): Parameters<DiscoverFilesParams>,
    ) -> Result<String, String> {
        match handle_discover_files(params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "discover_files failed");
                Err(e.to_string())
            }
        }
    }

    /// Update developer comments on localization keys.
    #[tool(
        name = "update_comments",
        description = "Update developer comments on existing localization keys. Silently skips non-existent keys. Writes atomically."
    )]
    async fn update_comments(
        &self,
        Parameters(params): Parameters<UpdateCommentsParams>,
    ) -> Result<String, String> {
        match handle_update_comments(self.store.as_ref(), &self.cache, &self.write_lock, params)
            .await
        {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "update_comments failed");
                Err(e.to_string())
            }
        }
    }

    /// Delete localization keys and all their translations.
    #[tool(
        name = "delete_keys",
        description = "Delete localization keys and all their translations. Use after get_stale to remove unused keys."
    )]
    async fn delete_keys(
        &self,
        Parameters(params): Parameters<DeleteKeysParams>,
    ) -> Result<String, String> {
        match handle_delete_keys(self.store.as_ref(), &self.cache, &self.write_lock, params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "delete_keys failed");
                Err(e.to_string())
            }
        }
    }

    /// Rename a localization key, preserving all translations.
    #[tool(
        name = "rename_key",
        description = "Rename a localization key, preserving all existing translations across all locales."
    )]
    async fn rename_key(
        &self,
        Parameters(params): Parameters<RenameKeyParams>,
    ) -> Result<String, String> {
        match handle_rename_key(self.store.as_ref(), &self.cache, &self.write_lock, params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "rename_key failed");
                Err(e.to_string())
            }
        }
    }

    /// Get all translations for a specific key across all locales.
    #[tool(
        name = "get_key",
        description = "Get all translations for a specific key across every locale. Returns source text, developer comment, and all locales with recursive leaves, typed paths, values, states, completeness and diagnostics. Use to inspect a single key in detail."
    )]
    async fn get_key(
        &self,
        Parameters(params): Parameters<GetKeyParams>,
    ) -> Result<String, String> {
        match handle_get_key(self.store.as_ref(), &self.cache, params).await {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "get_key failed");
                Err(e.to_string())
            }
        }
    }

    /// Remove translations for specific keys in a locale.
    #[tool(
        name = "delete_translations",
        description = "Remove translations for specific keys in a locale, resetting them to untranslated state. Cannot delete source language translations."
    )]
    async fn delete_translations(
        &self,
        Parameters(params): Parameters<DeleteTranslationsParams>,
    ) -> Result<String, String> {
        match handle_delete_translations(self.store.as_ref(), &self.cache, &self.write_lock, params)
            .await
        {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|e| format!("serialization error: {e}")),
            Err(e) => {
                error!(error = %e, "delete_translations failed");
                Err(e.to_string())
            }
        }
    }
}
