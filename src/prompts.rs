use rmcp::{
    handler::server::wrapper::Parameters,
    model::{GetPromptResult, PromptMessage, PromptMessageRole},
    prompt, prompt_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::server::XcStringsMcpServer;

pub(crate) fn build_prompt_router()
-> rmcp::handler::server::router::prompt::PromptRouter<XcStringsMcpServer> {
    XcStringsMcpServer::prompt_router()
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct TranslateBatchParams {
    /// The target locale code (e.g. "uk", "fr", "de")
    locale: String,
    /// Number of strings to translate per batch (default: 20)
    count: Option<u32>,
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

#[prompt_router]
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
        let count = params.count.unwrap_or(20);
        let content = format!(
            "You are translating iOS app strings to {locale}.\n\
            \n\
            Instructions:\n\
            1. Call get_untranslated with locale=\"{locale}\" and batch_size={count}\n\
            2. For each string, translate naturally \u{2014} not word-for-word\n\
            3. Preserve all format specifiers (%@, %d, %lld, etc.) exactly as they appear\n\
            4. For plural forms, use get_plurals to see required CLDR forms for {locale}\n\
            5. Use get_context to understand nearby related strings\n\
            6. Submit translations using submit_translations\n\
            7. If there are more untranslated strings, repeat from step 1\n\
            \n\
            Guidelines:\n\
            - Keep translations concise \u{2014} mobile UI has limited space\n\
            - Maintain consistent terminology \u{2014} use get_glossary to check existing terms\n\
            - Don't translate brand names or technical identifiers\n\
            - Preserve the tone and formality level of the source text",
            locale = params.locale,
            count = count,
        );

        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            content,
        )])
        .with_description(format!(
            "Translate a batch of {count} strings to {}",
            params.locale
        )))
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
            1. Call validate_translations with locale=\"{locale}\" to find technical issues\n\
            2. Call get_coverage with locale=\"{locale}\" to see overall progress\n\
            3. For each validation issue, assess severity:\n\
            \x20  - Format specifier mismatches: CRITICAL \u{2014} fix immediately\n\
            \x20  - Missing plural forms: HIGH \u{2014} will cause runtime issues\n\
            \x20  - Empty translations: MEDIUM \u{2014} incomplete but not broken\n\
            4. Review a sample of translated strings for quality:\n\
            \x20  - Natural language flow (not word-for-word translation)\n\
            \x20  - Consistent terminology\n\
            \x20  - Appropriate length for mobile UI\n\
            \x20  - Correct gender/number agreement\n\
            5. Report findings with specific key names and suggested fixes",
            locale = params.locale,
        );

        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            content,
        )])
        .with_description(format!(
            "Review translations for locale \"{}\"",
            params.locale
        )))
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
            \n\
            Step 3: Translate simple strings\n\
            \x20 Call get_untranslated with locale=\"{locale}\"\n\
            \x20 Translate each batch and submit with submit_translations\n\
            \x20 Repeat until no untranslated strings remain\n\
            \n\
            Step 4: Translate plural forms\n\
            \x20 Call get_plurals with locale=\"{locale}\"\n\
            \x20 For each plural key, provide all required CLDR forms\n\
            \x20 Submit using submit_translations with plural_forms\n\
            \n\
            Step 5: Validate\n\
            \x20 Call validate_translations to check for issues\n\
            \x20 Fix any problems found\n\
            \n\
            Step 6: Final check\n\
            \x20 Call get_coverage to confirm 100% for {locale}\n\
            \x20 Call get_diff to see all changes made",
            file_path = params.file_path,
            locale = params.locale,
        );

        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            content,
        )])
        .with_description(format!(
            "Full translation workflow for {} to {}",
            params.file_path, params.locale
        )))
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
            \x20   CRITICAL: format specifier mismatches \u{2014} will crash at runtime\n\
            \x20   HIGH: missing plural forms \u{2014} will show wrong text\n\
            \x20   MEDIUM: empty translations \u{2014} incomplete but not broken\n\
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

        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            content,
        )])
        .with_description(format!("Localization audit for \"{}\"", params.locale)))
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
            Step 2: Fix CRITICAL issues first (format specifier mismatches)\n\
            \x20 For each specifier mismatch:\n\
            \x20 - Call get_context to understand the string's purpose\n\
            \x20 - Fix the translation to include the correct specifiers\n\
            \x20 - Submit with submit_translations (dry_run=true first to verify)\n\
            \n\
            Step 3: Fix HIGH issues (missing plural forms)\n\
            \x20 For each missing plural form:\n\
            \x20 - Call get_plurals to see required CLDR forms for {locale}\n\
            \x20 - Provide all required forms (one, few, many, other etc.)\n\
            \x20 - Submit with submit_translations using plural_forms\n\
            \n\
            Step 4: Fix MEDIUM issues (empty translations)\n\
            \x20 These are untranslated strings \u{2014} use the translate_batch workflow\n\
            \x20 Call get_untranslated and translate in batches\n\
            \n\
            Step 5: Verify\n\
            \x20 Call validate_translations again to confirm zero issues remain",
            locale = params.locale,
        );

        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            content,
        )])
        .with_description(format!("Fix validation errors for \"{}\"", params.locale)))
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
            Step 5: Translate simple strings\n\
            \x20 Call get_untranslated in batches (batch_size=20)\n\
            \x20 Translate each batch naturally, preserving format specifiers\n\
            \x20 Submit with submit_translations\n\
            \x20 Repeat until no untranslated strings remain\n\
            \n\
            Step 6: Translate plural forms\n\
            \x20 Call get_plurals with locale=\"{locale}\"\n\
            \x20 For each plural key, provide all required CLDR forms\n\
            \x20 Submit using submit_translations with plural_forms\n\
            \n\
            Step 7: Validate and finalize\n\
            \x20 Call validate_translations to check for issues\n\
            \x20 Call get_coverage to confirm 100% for {locale}",
            locale = params.locale,
            file_instruction = file_instruction,
        );

        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            content,
        )])
        .with_description(format!("Add language \"{}\" and translate", params.locale)))
    }
}
