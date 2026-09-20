//! Pure source freshness, review and checkpoint plans. No filesystem access.
mod approval;
mod document;
mod queue;
mod read;
mod sync;
mod versions;

pub use approval::{ApprovalPlan, plan_approval};
pub use document::{format_document, parse_document};
pub use queue::review_queue;
pub use read::{WorkflowView, inspect, leaf_status};
pub use sync::{SourceSyncPlan, plan_checkpoint, plan_source_sync};
pub use versions::{source_snapshot, source_version, target_version};

use crate::{
    error::XcStringsError,
    model::{
        translation::TranslationDestination,
        workflow::{WorkflowCode, WorkflowDiagnostic},
    },
};
fn invalid(code: &str, message: impl std::fmt::Display) -> XcStringsError {
    XcStringsError::InvalidFormat(format!("{code}: {message}"))
}
fn identity(value: &str) -> Result<(), XcStringsError> {
    if value.is_empty() {
        Err(invalid(
            "invalid_catalog_identity",
            "canonical catalog identity is empty",
        ))
    } else {
        Ok(())
    }
}
fn diagnostic(
    code: WorkflowCode,
    message: impl Into<String>,
    destination: Option<TranslationDestination>,
    key: Option<String>,
) -> WorkflowDiagnostic {
    WorkflowDiagnostic {
        code,
        message: message.into(),
        destination,
        key,
    }
}
