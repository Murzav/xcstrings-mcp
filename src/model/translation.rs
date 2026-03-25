use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A string needing translation, returned by get_untranslated, get_stale, and search_keys.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TranslationUnit {
    /// Localization key name
    pub key: String,
    /// Source language text to translate from
    pub source_text: String,
    /// Locale code this unit needs translation for
    pub target_locale: String,
    /// Developer comment providing context for translators
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Format specifiers found in source (e.g., ["%@", "%lld"]). Translations MUST preserve these exactly.
    pub format_specifiers: Vec<String>,
    /// True if key uses plural variations. Use get_plurals for full details before translating.
    pub has_plurals: bool,
    /// True if key uses substitution variables (%#@VAR@). Use get_plurals for details.
    pub has_substitutions: bool,
}

/// A completed translation to submit via submit_translations.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompletedTranslation {
    /// Localization key exactly as returned by get_untranslated or get_plurals
    pub key: String,
    /// Target locale code (e.g., "uk", "de"). Must not be the source language.
    pub locale: String,
    /// Translated text for simple strings. Must preserve all format specifiers from source. Ignored when plural_forms is set.
    pub value: String,
    /// Plural translations keyed by CLDR category, e.g. {"one": "1 item", "other": "%lld items"}. Required categories vary by locale — use get_plurals to see which forms are needed. When set, value is ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plural_forms: Option<BTreeMap<String, String>>,
    /// Substitution variable name for multi-variable plurals (from %#@VAR@ in source). Only needed when PluralUnit.has_substitutions is true. Omit for simple plurals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub substitution_name: Option<String>,
}

/// Summary of a parsed .xcstrings file, returned by parse_xcstrings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileSummary {
    /// Source language code (e.g., "en")
    pub source_language: String,
    /// Total number of keys in the file (including non-translatable)
    pub total_keys: usize,
    /// Number of keys that should be translated (shouldTranslate=true)
    pub translatable_keys: usize,
    /// All locale codes present in the file
    pub locales: Vec<String>,
    /// Key count per extraction state (e.g., {"extracted_with_value": 42, "manual": 3})
    pub keys_by_state: BTreeMap<String, usize>,
}

/// Result of submit_translations or import_xliff.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SubmitResult {
    /// Number of translations that passed validation and were written (or would be written in dry_run)
    pub accepted: usize,
    /// Translations that failed validation. Check reason field for details, fix, and resubmit.
    pub rejected: Vec<RejectedTranslation>,
    /// True if this was a validation-only run (nothing written to disk)
    pub dry_run: bool,
    /// List of accepted key names for reference
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted_keys: Vec<String>,
}

/// A translation that failed validation during submit.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RejectedTranslation {
    /// The key that was rejected
    pub key: String,
    /// Human-readable rejection reason (e.g., missing format specifier, wrong plural forms)
    pub reason: String,
}

/// Per-locale translation coverage statistics.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LocaleCoverage {
    /// Locale code
    pub locale: String,
    /// Total keys in the file (including non-translatable)
    pub total_keys: usize,
    /// Number of keys that should be translated
    pub translatable_keys: usize,
    /// Number of keys with translations in this locale
    pub translated: usize,
    /// Translation completion percentage (0.0–100.0)
    pub percentage: f64,
}

/// Full coverage report across all locales.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CoverageReport {
    pub source_language: String,
    pub total_keys: usize,
    pub translatable_keys: usize,
    pub locales: Vec<LocaleCoverage>,
}

/// Validation result with errors and warnings for a single locale.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ValidationReport {
    pub locale: String,
    pub errors: Vec<ValidationIssue>,
    pub warnings: Vec<ValidationIssue>,
}

/// A single validation problem found in a translation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ValidationIssue {
    /// Localization key with the issue
    pub key: String,
    /// Issue category (e.g., "missing_format_specifier", "wrong_plural_forms", "empty_value")
    pub issue_type: String,
    /// Human-readable description of the problem
    pub message: String,
}

/// Locale info for list_locales output.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LocaleInfo {
    pub locale: String,
    pub translated: usize,
    pub total: usize,
    pub percentage: f64,
}

/// A key requiring plural translation (returned by get_plurals).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluralUnit {
    /// Localization key name
    pub key: String,
    /// Source language text
    pub source_text: String,
    /// Locale code to translate for
    pub target_locale: String,
    /// Developer comment for context
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Format specifiers to preserve in all plural forms (e.g., ["%lld"])
    pub format_specifiers: Vec<String>,
    /// Required CLDR plural categories for target locale (e.g., ["one", "few", "many", "other"] for Ukrainian). All forms must be provided in submit_translations.
    pub required_forms: Vec<String>,
    /// Source language plural forms (if available).
    pub source_forms: BTreeMap<String, String>,
    /// Existing translations per plural form (if partially translated).
    pub existing_translations: BTreeMap<String, String>,
    /// True if this key uses substitutions (%#@VAR@).
    pub has_substitutions: bool,
    /// Device variant forms needed (if any).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub device_forms: Vec<String>,
}

/// A nearby key sharing a common prefix, used for translator context.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContextKey {
    /// Related key name
    pub key: String,
    /// Source language text
    pub source_text: String,
    /// Existing translation in the target locale, if available
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translated_text: Option<String>,
}

/// Report of differences between cached and on-disk versions.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiffReport {
    /// Keys added to the file since last parse
    pub added: Vec<String>,
    /// Keys removed from the file since last parse
    pub removed: Vec<String>,
    /// Keys whose source language text changed
    pub modified: Vec<ModifiedKey>,
}

/// A key whose source text changed between cached and on-disk versions.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModifiedKey {
    /// Localization key name
    pub key: String,
    /// Previous source text (from cache)
    pub old_value: String,
    /// Current source text (from disk)
    pub new_value: String,
}
