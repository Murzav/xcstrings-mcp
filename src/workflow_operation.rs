//! Fresh catalog/workflow snapshots and guarded single-file commits.
//! Source snapshots are portable; operation tokens use canonical catalog identity.
pub mod read;
pub mod sync;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::XcStringsError;
use crate::io::{FilePrecondition, FileStore};
use crate::model::{workflow::WorkflowDocument, xcstrings::XcStringsFile};
use crate::service::{formatter, parser, workflow};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InputRevisions {
    pub catalog: String,
    pub workflow: String,
}

pub struct CatalogSnapshot {
    pub display_path: PathBuf,
    pub identity: PathBuf,
    pub catalog_bytes: Vec<u8>,
    pub catalog: XcStringsFile,
    pub workflow_path: PathBuf,
    pub workflow_bytes: Option<Vec<u8>>,
    pub workflow: WorkflowDocument,
}

impl CatalogSnapshot {
    pub fn load(store: &dyn FileStore, path: &Path) -> Result<Self, XcStringsError> {
        if path.extension().and_then(|value| value.to_str()) != Some("xcstrings") {
            return Err(XcStringsError::NotXcStrings { path: path.into() });
        }
        let identity = store.file_identity(path)?;
        identity_text(&identity)?;
        let catalog_bytes = store.read_bytes(&identity)?;
        let catalog = parser::parse(utf8_text(&catalog_bytes)?)?;
        let workflow_path = sidecar_path(&identity)?;
        let actual_sidecar = store.file_identity(&workflow_path)?;
        if actual_sidecar != workflow_path {
            return Err(XcStringsError::InvalidPath {
                path: workflow_path,
                reason: "workflow sidecar must not redirect to another file".into(),
            });
        }
        let workflow_bytes = if store.exists(&workflow_path) {
            Some(store.read_bytes(&workflow_path)?)
        } else {
            None
        };
        let workflow = match &workflow_bytes {
            Some(bytes) => workflow::parse_document(utf8_text(bytes)?)?,
            None => WorkflowDocument::default(),
        };
        Ok(Self {
            display_path: path.into(),
            identity,
            catalog_bytes,
            catalog,
            workflow_path,
            workflow_bytes,
            workflow,
        })
    }

    pub fn identity_text(&self) -> Result<&str, XcStringsError> {
        identity_text(&self.identity)
    }

    pub fn view(&self) -> Result<workflow::WorkflowView<'_>, XcStringsError> {
        workflow::inspect(self.identity_text()?, &self.catalog, &self.workflow)
    }

    pub fn revisions(&self) -> InputRevisions {
        InputRevisions {
            catalog: byte_revision(Some(&self.catalog_bytes)),
            workflow: byte_revision(self.workflow_bytes.as_deref()),
        }
    }

    pub fn check_revisions(&self, expected: &InputRevisions) -> Result<(), XcStringsError> {
        let actual = self.revisions();
        if expected.catalog != actual.catalog {
            return Err(XcStringsError::InvalidFormat(
                "stale workflow input: catalog".into(),
            ));
        }
        if expected.workflow != actual.workflow {
            return Err(XcStringsError::InvalidFormat(
                "stale workflow input: workflow".into(),
            ));
        }
        Ok(())
    }

    /// Returns the exact bytes written so a subsequent checkpoint can guard them.
    pub fn write_catalog(
        &self,
        store: &dyn FileStore,
        candidate: &XcStringsFile,
    ) -> Result<Vec<u8>, XcStringsError> {
        let content = formatter::format_xcstrings(candidate)?;
        store.write_if_inputs_match(
            &self.identity,
            Some(&self.catalog_bytes),
            &[FilePrecondition {
                path: &self.workflow_path,
                expected: self.workflow_bytes.as_deref(),
            }],
            &content,
        )?;
        Ok(content.into_bytes())
    }

    pub fn write_workflow(
        &self,
        store: &dyn FileStore,
        candidate: &WorkflowDocument,
        current_catalog_bytes: &[u8],
    ) -> Result<(), XcStringsError> {
        let content = workflow::format_document(candidate)?;
        store.write_if_inputs_match(
            &self.workflow_path,
            self.workflow_bytes.as_deref(),
            &[FilePrecondition {
                path: &self.identity,
                expected: Some(current_catalog_bytes),
            }],
            &content,
        )
    }
}

pub fn sidecar_path(identity: &Path) -> Result<PathBuf, XcStringsError> {
    let name = identity
        .file_name()
        .ok_or_else(|| XcStringsError::InvalidPath {
            path: identity.into(),
            reason: "catalog has no filename".into(),
        })?;
    let mut sidecar = name.to_os_string();
    sidecar.push(".xcstrings-mcp.json");
    Ok(identity.with_file_name(sidecar))
}

pub fn byte_revision(bytes: Option<&[u8]>) -> String {
    let mut payload = Vec::with_capacity(24 + bytes.map_or(0, <[u8]>::len));
    payload.extend_from_slice(b"xcstrings-mcp:input:v1:");
    payload.push(u8::from(bytes.is_some()));
    payload.extend_from_slice(bytes.unwrap_or_default());
    format!(
        "input-v1:{}",
        crate::service::semantic_merge::fingerprint(&payload)
    )
}

pub(crate) fn utf8_text(bytes: &[u8]) -> Result<&str, XcStringsError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    Ok(text.strip_prefix('\u{feff}').unwrap_or(text))
}

fn identity_text(path: &Path) -> Result<&str, XcStringsError> {
    path.to_str().ok_or_else(|| XcStringsError::InvalidPath {
        path: path.into(),
        reason: "catalog identity is not valid UTF-8".into(),
    })
}
