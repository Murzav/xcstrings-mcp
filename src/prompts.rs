use rmcp::{
    handler::server::wrapper::Parameters,
    model::{GetPromptResult, PromptMessage, Role},
    prompt, prompt_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::server::XcStringsMcpServer;

const DEFAULT_TRANSLATE_BATCH_COUNT: u32 = 20;
const MAX_TRANSLATE_BATCH_COUNT: u32 = 100;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct TranslateBatchParams {
    /// The target locale code (e.g. "uk", "fr", "de")
    locale: String,
    /// Number of strings to translate per batch as an MCP string (1-100, default: 20)
    count: Option<String>,
}

fn parse_translate_batch_count(count: Option<&str>) -> Result<u32, rmcp::ErrorData> {
    let Some(raw_count) = count else {
        return Ok(DEFAULT_TRANSLATE_BATCH_COUNT);
    };
    let count = raw_count.parse::<u32>().map_err(|_| {
        rmcp::ErrorData::invalid_params(
            format!("count must be an integer in 1..=100, got {raw_count:?}"),
            None,
        )
    })?;
    if !(1..=MAX_TRANSLATE_BATCH_COUNT).contains(&count) {
        return Err(rmcp::ErrorData::invalid_params(
            format!("count must be in 1..=100, got {count}"),
            None,
        ));
    }
    Ok(count)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ReviewTranslationsParams {
    /// The locale code to review translations for
    locale: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct LocalizationAuditParams {
    /// The locale code to audit
    locale: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct FixValidationErrorsParams {
    /// The locale code to fix validation errors for
    locale: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct AddLanguageParams {
    /// The target locale code to add
    locale: String,
    /// Path to the .xcstrings file (optional if already parsed)
    file_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct FullTranslateParams {
    /// The target locale code
    locale: String,
    /// Path to the .xcstrings file
    file_path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct CleanupStaleParams {
    /// Path to the .xcstrings file (optional if already parsed)
    file_path: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ExtractStringsParams {
    /// Source language code (e.g. "en")
    source_language: String,
    /// Path to the .xcstrings file to create or update
    file_path: String,
}

#[prompt_router(vis = "pub(crate)")]
impl XcStringsMcpServer {
    /// Instructions for translating a batch of strings to a target locale
    #[prompt(
        name = "translate_batch",
        description = "Instructions for translating a batch of strings to a target locale"
    )]
    fn translate_batch(
        &self,
        Parameters(params): Parameters<TranslateBatchParams>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        let count = parse_translate_batch_count(params.count.as_deref())?;
        let content = format!(
            "You are translating iOS app strings to {locale}.\n\
            \n\
            Instructions:\n\
            1. Call get_untranslated with locale=\"{locale}\" and batch_size={count}\n\
            2. For each string, translate naturally \u{2014} not word-for-word\n\
            3. Preserve the conversion, length modifier, flags, width, and precision of all definite Foundation format arguments; valid positional reordering is allowed\n\
            4. Inspect leaves and diagnostics; get_plurals details plural/device/substitution chains and required CLDR forms for {locale}\n\
            5. Use get_context for current source, authored screen/role/purpose, variable meanings, applicable glossary rules and neighbors; absent facts remain unknown\n\
            6. Submit each required incomplete leaf with its exact returned path; path=[] selects the root. Do not mix path with legacy plural_forms/substitution_name\n\
            7. Include expected_source_version captured with the input key; never refresh a token to force an outdated translation through. Inspect rejected[], warnings[], and advisory terminology issues\n\
            8. Each accepted submission is needs_review. Process this captured batch once; report drafts for separate review and do not resubmit them merely because coverage remains incomplete\n\
            \n\
            Guidelines:\n\
            - Keep translations concise \u{2014} mobile UI has limited space\n\
            - Maintain consistent terminology \u{2014} use get_glossary to check existing terms\n\
            - Don't translate brand names or technical identifiers\n\
            - Preserve the tone and formality level of the source text\n\
            - Fix rejected definite-argument errors before retranslating; review warnings without changing intentional percentage prose merely to silence them",
            locale = params.locale,
            count = count,
        );

        Ok(
            GetPromptResult::new(vec![PromptMessage::new_text(Role::User, content)])
                .with_description(format!(
                    "Translate a batch of {count} strings to {}",
                    params.locale
                )),
        )
    }

    /// Instructions for reviewing existing translations for quality
    #[prompt(
        name = "review_translations",
        description = "Instructions for reviewing existing translations for quality"
    )]
    fn review_translations(
        &self,
        Parameters(params): Parameters<ReviewTranslationsParams>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        let content = format!(
            "You are reviewing existing translations for locale \"{locale}\".\n\
            \n\
            Instructions:\n\
            1. Call validate_translations with locale=\"{locale}\" to find blocking errors and non-blocking warnings\n\
            2. Call get_coverage to see overall progress across locales, then inspect \"{locale}\"\n\
            3. For each validation issue, assess severity:\n\
            \x20  - Definite Foundation argument mismatches or invalid positions: BLOCKING \u{2014} fix before submit\n\
            \x20  - Ambiguous percent-in-prose differences: WARNING \u{2014} review context; valid prose may remain unchanged\n\
            \x20  - Missing required variation leaves: HIGH \u{2014} incomplete by CLDR recommendations, distinct from compiler validity\n\
            \x20  - Missing/draft translations: MEDIUM; intentional blank translated leaves are complete\n\
            4. Review a sample of translated strings for quality:\n\
            \x20  - Natural language flow (not word-for-word translation)\n\
            \x20  - Consistent terminology\n\
            \x20  - Appropriate length for mobile UI\n\
            \x20  - Correct gender/number agreement\n\
            5. Preview sync_source_changes; apply with returned expected (from input_revisions) to checkpoint source/context changes. First initialization defaults to review; choose adopt_existing only for an explicitly trusted baseline\n\
            6. Read get_review_queue for exact paths and expected_source_version/expected_target_version. Check current source, old source evidence, terminology and get_context before approving each draft\n\
            7. Only after actual review, preview approve_translations then apply using those captured versions. It changes state only. Never approve solely to increase coverage\n\
            8. After any mutation restart the queue at offset=0; later unchanged pages require expected_queue_version. Report unresolved items and specific suggested fixes",
            locale = params.locale,
        );

        Ok(
            GetPromptResult::new(vec![PromptMessage::new_text(Role::User, content)])
                .with_description(format!(
                    "Review translations for locale \"{}\"",
                    params.locale
                )),
        )
    }

    /// Complete workflow for translating an entire file
    #[prompt(
        name = "full_translate",
        description = "Complete workflow for translating an entire file"
    )]
    fn full_translate(
        &self,
        Parameters(params): Parameters<FullTranslateParams>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        let content = format!(
            "Complete translation workflow for {file_path} \u{2192} {locale}.\n\
            \n\
            Step 1: Parse the file\n\
            \x20 Call parse_xcstrings with file_path=\"{file_path}\"\n\
            \n\
            Step 2: Check current state\n\
            \x20 Call get_coverage to see existing translation progress for {locale}\n\
            \x20 Call list_locales to verify {locale} exists (add_locale if needed)\n\
            \x20 Call get_glossary for existing terminology guidance\n\
            \n\
            Step 3: Translate required leaves in batches\n\
            \x20 Read all get_untranslated pages for locale=\"{locale}\" before writes to capture a finite worklist and source versions\n\
            \x20 Inspect leaves and diagnostics; submit each translation with its exact path (path=[] for the root)\n\
            \x20 Preview with dry_run=true and each captured expected_source_version; submit each planned leaf once as needs_review. Preserve existing drafts for review; report unsupported diagnostics\n\
            \n\
            Step 4: Check all variation branches\n\
            \x20 Call get_plurals with locale=\"{locale}\"\n\
            \x20 Complete required plural/device/substitution leaves, including supported chains, using returned typed paths\n\
            \x20 Legacy plural_forms/substitution_name require omitting path; native accepted counts input requests, accepted_destinations lists concrete leaves\n\
            \x20 CLDR 48.2.1 requirements measure completeness, not Xcode's compiler minimum\n\
            \n\
            Step 5: Validate\n\
            \x20 Call validate_translations to check blocking errors and non-blocking warnings\n\
            \x20 Fix blocking problems; review ambiguous percent-in-prose warnings in context\n\
            \n\
            Step 6: Separate review and final check\n\
            \x20 Report the drafted work and get_review_queue. Use review_translations for explicit acceptance; never auto-approve to reach 100%. Call get_coverage; only claim completion after required leaves are approved and current\n\
            \x20 Intentional blank translated leaves are complete; missing/new/needs_review leaves and unknown shapes/locales are incomplete\n\
            \x20 Call get_diff to see all changes made",
            file_path = params.file_path,
            locale = params.locale,
        );

        Ok(
            GetPromptResult::new(vec![PromptMessage::new_text(Role::User, content)])
                .with_description(format!(
                    "Full translation workflow for {} to {}",
                    params.file_path, params.locale
                )),
        )
    }

    /// Complete localization audit for a locale
    #[prompt(
        name = "localization_audit",
        description = "Run a complete localization audit: coverage, validation, stale keys, glossary consistency"
    )]
    fn localization_audit(
        &self,
        Parameters(params): Parameters<LocalizationAuditParams>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        let content = format!(
            "Complete localization audit for locale \"{locale}\".\n\
            \n\
            Step 1: Check coverage\n\
            \x20 Call get_coverage to see translation progress for {locale}\n\
            \n\
            Step 2: Validate existing translations\n\
            \x20 Call validate_translations to find technical issues for {locale}\n\
            \x20 Categorize by severity:\n\
            \x20   BLOCKING: definite Foundation argument mismatch or invalid position\n\
            \x20   WARNING: ambiguous percent-in-prose difference \u{2014} review, but do not rewrite valid prose solely to silence it\n\
            \x20   HIGH: missing required variation leaves \u{2014} incomplete by CLDR recommendations\n\
            \x20   MEDIUM: missing/draft translations; intentional blank translated leaves are complete\n\
            \n\
            Step 3: Check for stale keys\n\
            \x20 Call get_stale with locale=\"{locale}\" to find removed strings\n\
            \x20 These can be safely ignored or cleaned up\n\
            \n\
            Step 4: Check glossary consistency\n\
            \x20 Call get_glossary for the source/target locale pair\n\
            \x20 Use search_keys to spot-check that key terms match glossary\n\
            \n\
            Step 5: Summary report\n\
            \x20 Report: coverage %, validation errors by severity,\n\
            \x20 stale key count, and any glossary inconsistencies found",
            locale = params.locale,
        );

        Ok(
            GetPromptResult::new(vec![PromptMessage::new_text(Role::User, content)])
                .with_description(format!("Localization audit for \"{}\"", params.locale)),
        )
    }

    /// Fix all validation errors for a locale
    #[prompt(
        name = "fix_validation_errors",
        description = "Guided workflow to find and fix all validation errors for a locale"
    )]
    fn fix_validation_errors(
        &self,
        Parameters(params): Parameters<FixValidationErrorsParams>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        let content = format!(
            "Fix validation errors for locale \"{locale}\".\n\
            \n\
            Step 1: Get all validation issues\n\
            \x20 Call validate_translations with locale=\"{locale}\"\n\
            \n\
            Step 2: Fix blocking format errors first\n\
            \x20 For each definite Foundation argument mismatch or invalid position:\n\
            \x20 - Call get_context to understand the string's purpose\n\
            \x20 - Preserve conversion, length modifier, flags, width, and precision; positional reordering is allowed when argument numbers remain correct\n\
            \x20 - Submit with submit_translations using expected_source_version captured with the source (dry_run=true first to verify); this saves needs_review\n\
            \n\
            Step 3: Review warnings[]\n\
            \x20 Ambiguous percent-in-prose differences are non-blocking; confirm the wording is intentional instead of forcing it to resemble a format argument\n\
            \n\
            Step 4: Fix HIGH issues (missing plural forms)\n\
            \x20 For each missing plural/device/substitution leaf:\n\
            \x20 - Call get_plurals to see required CLDR forms for {locale}\n\
            \x20 - Provide all required forms (one, few, many, other etc.)\n\
            \x20 - Submit with the exact returned path; use plural_forms only for a legacy aggregate with no path\n\
            \n\
            Step 5: Complete missing or draft translations\n\
            \x20 Preserve intentional blank translated/machine_translated leaves; new/needs_review leaves remain incomplete\n\
            \x20 Call get_untranslated and translate in batches\n\
            \n\
            Step 6: Verify\n\
            \x20 Call validate_translations again; inspect reports and advisory terminology. Report drafts awaiting separate get_review_queue/approve_translations review",
            locale = params.locale,
        );

        Ok(
            GetPromptResult::new(vec![PromptMessage::new_text(Role::User, content)])
                .with_description(format!("Fix validation errors for \"{}\"", params.locale)),
        )
    }

    /// Extract hardcoded strings from Swift source code into .xcstrings
    #[prompt(
        name = "extract_strings",
        description = "Guided workflow to extract hardcoded strings from Swift source code into an .xcstrings file"
    )]
    fn extract_strings(
        &self,
        Parameters(params): Parameters<ExtractStringsParams>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        let content = format!(
            "Extract hardcoded strings from Swift source code into {file_path}.\n\
            \n\
            Step 1: Create or parse the .xcstrings file\n\
            \x20 If {file_path} does not exist:\n\
            \x20   Call create_xcstrings with file_path=\"{file_path}\" and \
            source_language=\"{source_language}\"\n\
            \x20 If it already exists:\n\
            \x20   Call parse_xcstrings with file_path=\"{file_path}\"\n\
            \n\
            Step 2: Scan Swift files for hardcoded strings\n\
            \x20 Look for patterns like:\n\
            \x20   - Text(\"...\") and Label(\"...\")\n\
            \x20   - String literals in .alert(), .navigationTitle(), etc.\n\
            \x20   - NSLocalizedString(\"...\", comment: \"...\")\n\
            \x20   - Any user-visible string literal\n\
            \x20 Skip: debug logs, print(), assert messages, identifiers\n\
            \n\
            Step 3: Generate key names\n\
            \x20 Use dot.separated.convention based on context:\n\
            \x20   - screen.element.description (e.g., settings.title, login.button.submit)\n\
            \x20   - Keep keys short but descriptive\n\
            \x20   - Group related keys with shared prefixes\n\
            \n\
            Step 4: Add keys to the .xcstrings file\n\
            \x20 Call add_keys with the generated keys and source text\n\
            \x20 Include developer comments describing the context\n\
            \n\
            Step 5: Replace hardcoded strings in Swift code\n\
            \x20 Replace each hardcoded string with String(localized: \"key.name\")\n\
            \x20 For strings with definite Foundation format arguments, use appropriate interpolation\n\
            \n\
            Step 6: Validate\n\
            \x20 Call parse_xcstrings to verify the file is valid\n\
            \x20 Ensure all replaced strings have corresponding keys",
            file_path = params.file_path,
            source_language = params.source_language,
        );

        Ok(
            GetPromptResult::new(vec![PromptMessage::new_text(Role::User, content)])
                .with_description(format!(
                    "Extract strings from Swift code into {}",
                    params.file_path
                )),
        )
    }

    /// Find and remove stale/unused localization keys
    #[prompt(
        name = "cleanup_stale",
        description = "Find and remove stale/unused localization keys"
    )]
    fn cleanup_stale(
        &self,
        Parameters(params): Parameters<CleanupStaleParams>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        let file_instruction = params
            .file_path
            .as_ref()
            .map(|fp| format!("\n  Call parse_xcstrings with file_path=\"{fp}\""))
            .unwrap_or_else(|| {
                "\n  Ensure a file is already parsed (call parse_xcstrings if needed)".to_string()
            });

        let content = format!(
            "Find and remove stale/unused localization keys.\n\
            \n\
            1. Parse the file{file_instruction}\n\
            2. Call list_locales and choose the locale to inspect\n\
            3. Call get_stale(locale=\"<locale>\", batch_size=100) to find keys removed from source code\n\
            4. Review each stale key with get_key and get_context to confirm it is unused\n\
            5. Call delete_keys with confirmed stale keys\n\
            6. Call get_coverage and get_stale(locale=\"<locale>\", batch_size=100) to verify cleanup",
        );

        Ok(
            GetPromptResult::new(vec![PromptMessage::new_text(Role::User, content)])
                .with_description("Find and remove stale/unused localization keys"),
        )
    }

    /// Add a new language and begin translating
    #[prompt(
        name = "add_language",
        description = "Guided workflow to add a new locale and translate all strings"
    )]
    fn add_language(
        &self,
        Parameters(params): Parameters<AddLanguageParams>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        let file_instruction = params
            .file_path
            .as_ref()
            .map(|fp| format!("\n  Call parse_xcstrings with file_path=\"{fp}\""))
            .unwrap_or_else(|| {
                "\n  Ensure a file is already parsed (call parse_xcstrings if needed)".to_string()
            });

        let content = format!(
            "Add and translate a new language: {locale}.\n\
            \n\
            Step 1: Parse the file{file_instruction}\n\
            \n\
            Step 2: Add the locale\n\
            \x20 Call add_locale with locale=\"{locale}\"\n\
            \n\
            Step 3: Check scope\n\
            \x20 Call get_coverage to see how many strings need translation\n\
            \x20 Call get_untranslated with locale=\"{locale}\" to preview the first batch\n\
            \n\
            Step 4: Check glossary\n\
            \x20 Call get_glossary to see existing terminology guidance\n\
            \x20 Use consistent terminology throughout\n\
            \n\
            Step 5: Translate required leaves\n\
            \x20 Read get_untranslated pages (batch_size=20) into a finite worklist before writing; preserve existing drafts for review\n\
            \x20 Translate each batch naturally, preserving definite Foundation argument components; valid positional reordering is allowed\n\
            \x20 Submit with each leaf's exact returned path and captured expected_source_version; path=[] selects the root; accepted values remain needs_review\n\
            \x20 Process each planned leaf once; stop after drafting and report get_review_queue for separate review, including unsupported diagnostics\n\
            \n\
            Step 6: Check plural/device/substitution chains\n\
            \x20 Call get_plurals with locale=\"{locale}\"\n\
            \x20 Complete every required typed leaf and preserve its own format arguments and substitution metadata\n\
            \x20 Use CLDR 48.2.1 requirements; legacy plural_forms/substitution_name must not be combined with path\n\
            \n\
            Step 7: Validate and finalize\n\
            \x20 Call validate_translations to fix blocking errors and review non-blocking warnings\n\
            \x20 Call get_coverage and report draft counts. Use review_translations for separate acceptance; missing/draft leaves and unknown shapes/locales remain incomplete",
            locale = params.locale,
            file_instruction = file_instruction,
        );

        Ok(
            GetPromptResult::new(vec![PromptMessage::new_text(Role::User, content)])
                .with_description(format!("Add language \"{}\" and translate", params.locale)),
        )
    }
}

#[cfg(test)]
#[path = "prompts/tests.rs"]
mod tests;
