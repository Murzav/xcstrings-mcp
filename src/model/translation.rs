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
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RejectedTranslation {
    pub key: String,
    pub reason: String,
}
