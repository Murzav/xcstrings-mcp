use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::model::xcstrings::XcStringsFile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConflictChoice {
    Current,
    Incoming,
    Base,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConflictResolution {
    pub conflict_id: String,
    pub choice: ConflictChoice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConflictValue {
    pub present: bool,
    pub preview: String,
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MergeConflict {
    pub id: String,
    pub pointer: String,
    pub key: Option<String>,
    pub locale: Option<String>,
    pub field: Option<String>,
    pub kind: String,
    pub base: ConflictValue,
    pub current: ConflictValue,
    pub incoming: ConflictValue,
    #[serde(skip)]
    pub(crate) resolved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FileFingerprint {
    pub sha256: String,
    pub key_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MergeFingerprints {
    pub base: FileFingerprint,
    pub current: FileFingerprint,
    pub incoming: FileFingerprint,
    pub output_before: Option<FileFingerprint>,
    pub result: FileFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExpectedFingerprints {
    pub base: String,
    pub current: String,
    pub incoming: String,
    /// `null` means that the output must still be absent.
    pub output: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AutoApplied {
    pub current: usize,
    pub incoming: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Hash)]
pub struct MergeValidationIssue {
    pub severity: String,
    pub locale: String,
    pub key: String,
    pub issue_type: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct MergeOptions {
    pub resolutions: Vec<ConflictResolution>,
    pub conflict_offset: usize,
    pub conflict_limit: usize,
}

impl Default for MergeOptions {
    fn default() -> Self {
        Self {
            resolutions: Vec::new(),
            conflict_offset: 0,
            conflict_limit: 50,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MergeReport {
    pub dry_run: bool,
    pub written: bool,
    pub output_path: Option<String>,
    pub fingerprints: MergeFingerprints,
    pub expected_fingerprints: Option<ExpectedFingerprints>,
    pub auto_applied: AutoApplied,
    pub resolutions_applied: usize,
    pub conflict_total: usize,
    pub unresolved_conflict_total: usize,
    pub conflict_offset: usize,
    pub conflict_limit: usize,
    pub has_more: bool,
    pub conflicts: Vec<MergeConflict>,
    pub existing_validation_issues: Vec<MergeValidationIssue>,
    pub introduced_validation_issues: Vec<MergeValidationIssue>,
}

#[derive(Debug)]
pub struct PreparedMerge {
    pub report: MergeReport,
    pub content: String,
    pub(crate) parsed: XcStringsFile,
}
