//! Portable, explicit terminology rules. Diagnostics are always advisory.
use super::xcstrings::paths::LeafPath;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GlossaryDocument {
    pub schema_version: u32,
    pub terms: Vec<GlossaryTerm>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl Default for GlossaryDocument {
    fn default() -> Self {
        Self {
            schema_version: 2,
            terms: Vec::new(),
            extra: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GlossaryTerm {
    pub id: String,
    pub source_locale: String,
    pub target_locale: String,
    pub source: String,
    #[serde(default)]
    pub source_variants: Vec<String>,
    #[serde(default)]
    pub preferred: Vec<String>,
    #[serde(default)]
    pub accepted_variants: Vec<String>,
    #[serde(default)]
    pub forbidden: Vec<String>,
    #[serde(default)]
    pub do_not_translate: bool,
    #[serde(default)]
    pub match_mode: TermMatchMode,
    #[serde(default)]
    pub scope: TermScope,
    #[serde(default)]
    pub exceptions: Vec<TermException>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TermMatchMode {
    #[default]
    Word,
    Literal,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TermScope {
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default)]
    pub paths: Vec<LeafPath>,
    #[serde(default)]
    pub screens: Vec<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TermException {
    pub scope: TermScope,
    pub reason: String,
    #[serde(default)]
    pub accepted_variants: Vec<String>,
    #[serde(default)]
    pub ignore_checks: Vec<TermCheck>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TermCheck {
    Preferred,
    Forbidden,
    DoNotTranslate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ParsedGlossary {
    pub document: GlossaryDocument,
    pub needs_migration: bool,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GlossaryProjection {
    pub entries: BTreeMap<String, String>,
    pub omitted_term_ids: Vec<String>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GlossaryEdit {
    #[serde(default)]
    pub upsert: Vec<GlossaryTerm>,
    #[serde(default)]
    pub remove_ids: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GlossaryDiagnostic {
    pub code: String,
    pub term_id: Option<String>,
    pub detail: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TermApplicability {
    Applicable,
    Unevaluated,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RelevantTerm {
    pub term: GlossaryTerm,
    pub applicability: TermApplicability,
    pub accepted_variants: Vec<String>,
    pub ignored_checks: Vec<TermCheck>,
    pub matched_source: Vec<String>,
    pub explanation: Option<String>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TermSelection {
    pub terms: Vec<RelevantTerm>,
    pub diagnostics: Vec<GlossaryDiagnostic>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum TerminologyCode {
    #[serde(rename = "glossary_preferred_missing")]
    PreferredMissing,
    #[serde(rename = "glossary_forbidden_used")]
    ForbiddenUsed,
    #[serde(rename = "glossary_untranslatable_changed")]
    UntranslatableChanged,
    #[serde(rename = "glossary_rule_conflict")]
    RuleConflict,
    #[serde(rename = "qa_unevaluated")]
    Unevaluated,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TerminologyIssue {
    pub code: TerminologyCode,
    pub term_id: String,
    pub key: String,
    pub locale: String,
    pub path: LeafPath,
    pub expected: Vec<String>,
    pub observed: Vec<String>,
    pub detail: String,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TerminologyReport {
    pub issues: Vec<TerminologyIssue>,
    pub evaluated_term_ids: Vec<String>,
    pub unevaluated_term_ids: Vec<String>,
}
