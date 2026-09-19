use crate::model::xcstrings::{TranslationState, paths::LeafPath};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LeafSubstitution {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arg_num: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format_specifier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TranslationLeaf {
    pub path: LeafPath,
    pub source_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<TranslationState>,
    pub required: bool,
    pub complete: bool,
    pub substitutions: Vec<LeafSubstitution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct TranslationDestination {
    pub key: String,
    pub locale: String,
    pub path: LeafPath,
}
