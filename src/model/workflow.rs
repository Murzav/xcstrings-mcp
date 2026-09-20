//! Portable source checkpoints and explicit translation review contracts.
use super::{
    context::CatalogContexts,
    translation::{TranslationDestination, ValidationIssue},
    xcstrings::{TranslationState, paths::LeafPath},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowDocument {
    pub version: u32,
    #[serde(default)]
    pub source_baseline: Option<SourceBaseline>,
    #[serde(default)]
    pub contexts: CatalogContexts,
    #[serde(default)]
    pub pending_review: Vec<PendingReview>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl Default for WorkflowDocument {
    fn default() -> Self {
        Self {
            version: 1,
            source_baseline: None,
            contexts: BTreeMap::new(),
            pending_review: Vec::new(),
            extra: BTreeMap::new(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
pub struct SourceBaseline {
    pub sources: BTreeMap<String, SourceSnapshot>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct SourceSnapshot(pub Value);

/// Bounded source-change evidence for observed content, not a creation receipt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PendingReview {
    pub destination: TranslationDestination,
    pub old_source: Option<SourceSnapshot>,
    pub new_source: SourceSnapshot,
    pub target_content_version: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceFreshness {
    Current,
    SourceChanged,
    Untracked,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrackingStatus {
    Uninitialized,
    Initialized,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct KeySourceStatus {
    pub freshness: SourceFreshness,
    pub source_version: String,
    pub current: SourceSnapshot,
    pub baseline: Option<SourceSnapshot>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LeafWorkflowStatus {
    pub source_version: String,
    pub target_version: Option<String>,
    pub freshness: SourceFreshness,
    pub native_state: Option<TranslationState>,
}
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ReviewReason {
    Draft,
    SourceChanged,
    Untracked,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewItem {
    pub destination: TranslationDestination,
    pub source_text: String,
    pub value: String,
    pub native_state: TranslationState,
    pub effective_state: TranslationState,
    pub source_version: String,
    pub target_version: String,
    pub freshness: SourceFreshness,
    pub reasons: Vec<ReviewReason>,
    pub old_source: Option<SourceSnapshot>,
    pub current_source: SourceSnapshot,
    pub diagnostics: Vec<WorkflowDiagnostic>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewQuery {
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default)]
    pub reasons: Vec<ReviewReason>,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub expected_queue_version: Option<String>,
}
fn default_batch_size() -> usize {
    50
}
impl Default for ReviewQuery {
    fn default() -> Self {
        Self {
            locale: None,
            reasons: Vec::new(),
            batch_size: 50,
            offset: 0,
            expected_queue_version: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewPage {
    pub tracking: TrackingStatus,
    pub items: Vec<ReviewItem>,
    pub total: usize,
    pub queue_version: String,
    pub offset: usize,
    pub next_offset: Option<usize>,
    pub diagnostics: Vec<WorkflowDiagnostic>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApprovalRequest {
    pub key: String,
    pub locale: String,
    pub path: LeafPath,
    pub expected_source_version: String,
    pub expected_target_version: String,
}
impl ApprovalRequest {
    pub fn destination(&self) -> TranslationDestination {
        TranslationDestination {
            key: self.key.clone(),
            locale: self.locale.clone(),
            path: self.path.clone(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct ApprovalReport {
    pub accepted: usize,
    pub accepted_destinations: Vec<TranslationDestination>,
    pub rejected: Vec<WorkflowDiagnostic>,
    pub warnings: Vec<ValidationIssue>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum SyncMode {
    #[default]
    Review,
    AdoptExisting,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceSyncReport {
    pub tracking_before: TrackingStatus,
    pub mode: SyncMode,
    pub changed_keys: Vec<String>,
    pub untracked_keys: Vec<String>,
    pub removed_keys: Vec<String>,
    pub invalidated_destinations: Vec<TranslationDestination>,
    pub checkpoint_needed: bool,
    pub historical_freshness_unknown: bool,
    pub diagnostics: Vec<WorkflowDiagnostic>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkflowDiagnostic {
    pub code: WorkflowCode,
    pub message: String,
    pub destination: Option<TranslationDestination>,
    pub key: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowCode {
    SourceVersionMismatch,
    TargetVersionMismatch,
    SourceCheckpointRequired,
    DuplicateDestination,
    OverlappingDestination,
    UnknownKey,
    SourceLocale,
    NotTranslatable,
    MissingTarget,
    NotDraft,
    UnsupportedShape,
    FormatMismatch,
    UnknownLocale,
    QueueVersionRequired,
    QueueVersionMismatch,
    SourceChangedDuringSync,
    ReadyTargetsRequireInvalidation,
    AdoptRequiresUninitialized,
    StaleBaselineKey,
    StaleContextKey,
}
