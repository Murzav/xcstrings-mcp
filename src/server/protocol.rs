use super::XcStringsMcpServer;
use rmcp::{
    ServerHandler,
    model::{ProtocolVersion, ServerCapabilities, ServerConfig},
    prompt_handler, tool_handler,
};

#[tool_handler(router = self.tool_router)]
#[prompt_handler(router = self.prompt_router)]
impl ServerHandler for XcStringsMcpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
        )
        .with_protocol_version(ProtocolVersion::V_2025_06_18)
        .with_instructions(
            "MCP server for iOS/macOS .xcstrings String Catalog localization.\n\
                 \n\
                 SETUP: discover_files to find files, parse_xcstrings to load \
                 (required before other tools unless file_path is passed). \
                 create_xcstrings for new files.\n\
                 \n\
                 TRANSLATE: get_untranslated → translate → submit_translations \
                 (use dry_run=true first). Inspect leaves and diagnostics; copy each leaf path \
                 into its submission (path=[] explicitly selects the root), with the captured expected_source_version. Accepted values are needs_review drafts; do not resubmit them to increase coverage. get_plurals covers \
                 plural/device/substitution chains. Legacy plural_forms/substitution_name \
                 require omitting path. Use continue_on_error=false for an atomic native batch. \
                 Use get_context for nearby keys, get_glossary for term consistency, with advisory checks and explicit unavailable status. update_context stores authored facts with revision guards.\n\
                 \n\
                 REVIEW: preview/apply sync_source_changes with expected (from input_revisions); inspect get_review_queue and get_context, then approve_translations only after reviewing the exact captured source/target versions. Restart queue offset=0 after mutations; unchanged later pages require expected_queue_version. get_coverage for statistics, validate_translations for blocking errors \
                 and non-blocking warnings (format arguments, ambiguous percent prose, missing plurals), get_stale for removed keys, \
                 get_diff for changes since last parse.\n\
                 COMPLETENESS: every required leaf must be translated or machine_translated; \
                 intentional empty text in those states is complete. Draft/missing leaves, \
                 unsupported shapes, and unknown locales remain incomplete. CLDR 48.2.1 \
                 recommendations are not Xcode's compiler minimum. Report diagnostics; \
                 never delete unknown data or invent translations to force 100% coverage.\n\
                 \n\
                 MANAGE: list_locales, add_locale/remove_locale, \
                 add_keys/delete_keys/rename_key/get_key, search_keys, \
                 update_comments, delete_translations, list_files.\n\
                 MERGE: merge_xcstrings performs a conservative three-way catalog merge. \
                 Dry-run first, resolve conflicts, then apply with returned fingerprints.\n\
                 \n\
                 MIGRATE: import_strings for legacy .strings/.stringsdict → .xcstrings. \
                 export_xliff/import_xliff support Apple XLIFF 1.2 variations and exact original \
                 scopes. Imports preserve draft states and explicit empty targets; missing \
                 targets are no-ops. Any rejection prevents the entire selected import. \
                 Inspect accepted_destinations: native accepted counts requests, XLIFF counts \
                 trans-units. Unsafe Apple interoperability cases are rejected explicitly; \
                 compare changed content after Xcode import.\n\
                 \n\
                 GLOSSARY: get_glossary/update_glossary — persists across sessions for \
                 term consistency, with advisory checks and explicit unavailable status. update_context stores authored facts with revision guards.\n\
                 \n\
                 Pagination: batched tools return has_more/offset/total — repeat with \
                 offset += batch_size while has_more is true, without concurrent mutations. Capture drafting worklists before writes; drafts require separate review. \
                 Source locale translations cannot be submitted or deleted.",
        )
    }
}
