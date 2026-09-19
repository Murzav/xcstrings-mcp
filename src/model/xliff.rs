//! Lossless semantic XML input and catalog-aware import reports.
use super::xcstrings::{XcStringsFile, paths::LeafPath};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct XliffDocument {
    pub files: Vec<XliffFile>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct XliffFile {
    pub original: Option<String>,
    pub source_language: Option<String>,
    pub target_language: String,
    pub units: Vec<XliffUnit>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct XliffUnit {
    pub id: String,
    pub source: String,
    pub target: Option<String>,
    pub state: Option<String>,
    pub state_qualifier: Option<String>,
    pub notes: Vec<XliffNote>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct XliffNote {
    pub text: String,
    pub from: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XliffDestination {
    pub original: String,
    pub key: String,
    pub locale: String,
    pub path: LeafPath,
    pub unit_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XliffDiagnostic {
    pub code: String,
    pub unit_id: String,
    pub message: String,
    pub destination: Option<XliffDestination>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XliffImportReport {
    pub accepted: usize,
    pub accepted_destinations: Vec<XliffDestination>,
    pub rejected: Vec<XliffDiagnostic>,
    pub skipped_scopes: Vec<String>,
    pub missing_targets: usize,
    pub locale: String,
    pub warnings: Vec<super::translation::ValidationIssue>,
}
#[derive(Debug)]
pub struct XliffImportPlan {
    pub candidate: Option<XcStringsFile>,
    pub report: XliffImportReport,
}
