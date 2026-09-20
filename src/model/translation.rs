pub mod leaf;
use crate::model::xcstrings::paths::LeafDiagnostic;
pub use leaf::{LeafSubstitution, TranslationDestination, TranslationLeaf};

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A string needing translation, returned by get_untranslated, get_stale, and search_keys.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct TranslationUnit {
    /// Captured input version for guarded submissions; absent in catalog-only helpers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_freshness: Option<crate::model::workflow::SourceFreshness>,
    /// Localization key name
    pub key: String,
    /// Source language text to translate from
    pub source_text: String,
    /// Locale code this unit needs translation for
    pub target_locale: String,
    /// Developer comment providing context for translators
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Definite format arguments found in source (e.g., `["%@", "%lld"]`). Ambiguous percent-in-prose sequences are excluded and reported as warnings during validation.
    pub format_specifiers: Vec<String>,
    /// True if any leaf uses plural variations; leaves include the exact scopes.
    pub has_plurals: bool,
    /// True if key uses substitution variables (%#@VAR@). Use get_plurals for details.
    pub has_substitutions: bool,
    #[serde(default)]
    pub leaves: Vec<TranslationLeaf>,
    #[serde(default)]
    pub diagnostics: Vec<LeafDiagnostic>,
}

/// A completed translation to submit via submit_translations.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct CompletedTranslation {
    /// Localization key exactly as returned by get_untranslated or get_plurals
    pub key: String,
    /// Required source/context version captured when this translation was requested.
    /// A current version must never be invented while submitting an older draft.
    pub expected_source_version: String,
    /// Target locale code (e.g., "uk", "de"). Must not be the source language.
    pub locale: String,
    /// Translated text for simple strings. Must preserve each definite format argument's conversion, length modifier, flags, width, and precision. Positional reordering is allowed. Ignored when plural_forms is set.
    pub value: String,
    /// Plural translations keyed by CLDR category, e.g. {"one": "1 item", "other": "%lld items"}. Required categories vary by locale — use get_plurals to see which forms are needed. When set, value is ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plural_forms: Option<BTreeMap<String, String>>,
    /// Substitution variable name for multi-variable plurals (from %#@VAR@ in source). Each submitted form must preserve the exact `%arg` placeholder token; longer Unicode words such as `%argument` are not placeholders, while direct Han, Hiragana, Katakana, or Hangul adjacency is supported. Only needed when PluralUnit.has_substitutions is true. Omit for simple plurals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub substitution_name: Option<String>,
    /// Explicit leaf destination; [] denotes the root. Cannot be combined with aggregate selectors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<crate::model::xcstrings::paths::LeafPath>,
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

/// Stable blocking-result fields returned by submit_translations or import_xliff. Runtime responses add `warnings` when non-blocking format diagnostics exist.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
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
    #[serde(default)]
    pub accepted_destinations: Vec<TranslationDestination>,
}

/// Additive response used by submit and import surfaces when validation emits warnings.
/// The result includes concrete accepted destinations for scoped updates.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DetailedSubmitResult {
    #[serde(flatten)]
    pub result: SubmitResult,
    /// Non-blocking, machine-readable validation diagnostics.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<ValidationIssue>,
}

/// A translation that failed validation during submit.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct RejectedTranslation {
    /// The key that was rejected
    pub key: String,
    /// Human-readable rejection reason (e.g., missing format specifier, wrong plural forms)
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<crate::model::xcstrings::paths::LeafPath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct PluralUnit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_freshness: Option<crate::model::workflow::SourceFreshness>,
    /// Localization key name
    pub key: String,
    /// Source language text
    pub source_text: String,
    /// Locale code to translate for
    pub target_locale: String,
    /// Developer comment for context
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Definite format arguments to preserve in plural forms (e.g., ["%lld"]); percent-in-prose prefixes are not advertised as required arguments.
    pub format_specifiers: Vec<String>,
    /// Required CLDR plural categories for target locale (e.g., ["one", "few", "many", "other"] for Ukrainian). These define completeness; submit_translations also accepts partial updates.
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
    #[serde(default)]
    pub leaves: Vec<TranslationLeaf>,
    #[serde(default)]
    pub diagnostics: Vec<LeafDiagnostic>,
}

/// A nearby key sharing a common prefix, used for translator context.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct ContextKey {
    /// Related key name
    pub key: String,
    /// Source language text
    pub source_text: String,
    /// Existing translation in the target locale, if available
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translated_text: Option<String>,
    #[serde(default)]
    pub leaves: Vec<TranslationLeaf>,
    #[serde(default)]
    pub diagnostics: Vec<LeafDiagnostic>,
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
