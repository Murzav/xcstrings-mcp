//! Authored context is separate from Xcode-owned catalog data.
use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{glossary::TermSelection, translation::TranslationUnit, xcstrings::paths::LeafPath};

pub type CatalogContexts = BTreeMap<String, AuthoredContext>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AuthoredContext {
    #[serde(default)]
    pub context: ContextFields,
    #[serde(default)]
    pub leaves: Vec<LeafContextOverride>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LeafContextOverride {
    pub path: LeafPath,
    pub context: ContextFields,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ContextFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<Vec<VariableMeaning>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub neighbors: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshots: Option<Vec<ScreenshotReference>>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VariableReference {
    Argument(u32),
    Substitution(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VariableMeaning {
    pub reference: VariableReference,
    pub meaning: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScreenshotReference {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextOrigin {
    Key,
    Leaf,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ResolvedContext {
    pub fields: ContextFields,
    pub provenance: BTreeMap<String, ContextOrigin>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ContextDiagnostic {
    pub code: String,
    pub key: String,
    pub path: Option<LeafPath>,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NeighborRelation {
    Explicit,
    SameScreen,
    PrefixHeuristic,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContextNeighbor {
    pub unit: TranslationUnit,
    pub relation: NeighborRelation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceTextOrigin {
    Catalog,
    KeyFallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArgumentRole {
    Value,
    Width,
    Precision,
    Substitution,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ObservedVariable {
    pub reference: VariableReference,
    pub position: Option<u32>,
    pub role: ArgumentRole,
    pub format: Option<String>,
    pub meaning: Option<String>,
    pub meaning_origin: Option<ContextOrigin>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LeafContext {
    pub path: LeafPath,
    pub authored: ResolvedContext,
    pub variables: Vec<ObservedVariable>,
    pub terminology: TermSelection,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContextPackage {
    pub current: TranslationUnit,
    pub source_origin: SourceTextOrigin,
    pub authored: ResolvedContext,
    pub leaf_contexts: Vec<LeafContext>,
    pub neighbors: Vec<ContextNeighbor>,
    pub neighbors_truncated: bool,
    pub diagnostics: Vec<ContextDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ContextEdit {
    Set {
        key: String,
        context: Box<AuthoredContext>,
    },
    Remove {
        key: String,
    },
}
