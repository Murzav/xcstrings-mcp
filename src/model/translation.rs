use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TranslationUnit {
    pub key: String,
    pub source_text: String,
    pub target_locale: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    pub format_specifiers: Vec<String>,
    pub has_plurals: bool,
    pub has_substitutions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompletedTranslation {
    pub key: String,
    pub locale: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plural_forms: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub substitution_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileSummary {
    pub source_language: String,
    pub total_keys: usize,
    pub translatable_keys: usize,
    pub locales: Vec<String>,
    pub keys_by_state: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SubmitResult {
    pub accepted: usize,
    pub rejected: Vec<RejectedTranslation>,
    pub dry_run: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accepted_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RejectedTranslation {
    pub key: String,
    pub reason: String,
}

/// Per-locale translation coverage statistics.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LocaleCoverage {
    pub locale: String,
    pub total_keys: usize,
    pub translatable_keys: usize,
    pub translated: usize,
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ValidationIssue {
    pub key: String,
    pub issue_type: String,
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
    pub key: String,
    pub source_text: String,
    pub target_locale: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    pub format_specifiers: Vec<String>,
    /// Required plural forms for target locale (from CLDR).
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
    pub key: String,
    pub source_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translated_text: Option<String>,
}

/// Report of differences between cached and on-disk versions.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiffReport {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<ModifiedKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModifiedKey {
    pub key: String,
    pub old_value: String,
    pub new_value: String,
}
