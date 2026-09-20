//! One coherent terminology snapshot shared by native, review, and XLIFF operations.
use crate::{
    FileStore, XcStringsError,
    model::{
        context::CatalogContexts,
        glossary::{GlossaryDocument, TerminologyIssue},
        translation::TranslationDestination,
        xcstrings::XcStringsFile,
    },
    service::{assessment, context, glossary},
    workflow_operation::byte_revision,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GuidanceStatus {
    Available,
    Absent,
    Unavailable,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GuidanceDiagnostic {
    pub code: String,
    pub detail: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GuidanceReport {
    pub status: GuidanceStatus,
    pub revision: Option<String>,
    pub issues: Vec<TerminologyIssue>,
    pub unavailable: Option<GuidanceDiagnostic>,
}

pub struct GuidanceSnapshot {
    pub document: Option<GlossaryDocument>,
    pub status: GuidanceStatus,
    pub revision: Option<String>,
    pub unavailable: Option<GuidanceDiagnostic>,
    pub needs_migration: bool,
    pub(crate) identity: Option<PathBuf>,
    pub(crate) raw_bytes: Option<Vec<u8>>,
}
impl GuidanceSnapshot {
    /// Policy failure is reported as unavailable; it never vetoes a translation.
    pub fn load(store: &dyn FileStore, path: &Path) -> Self {
        let mut snapshot = Self {
            document: None,
            status: GuidanceStatus::Unavailable,
            revision: None,
            unavailable: None,
            needs_migration: false,
            identity: None,
            raw_bytes: None,
        };
        match policy_identity(store, path) {
            Ok(identity) => snapshot.identity = Some(identity),
            Err(error) => {
                snapshot.unavailable = Some(unavailable("glossary_path_unavailable", error));
                return snapshot;
            }
        }
        let identity = snapshot.identity.as_deref().unwrap_or(path);
        if !store.exists(identity) {
            snapshot.document = Some(GlossaryDocument::default());
            snapshot.status = GuidanceStatus::Absent;
            snapshot.revision = Some(byte_revision(None));
            return snapshot;
        }
        match store.read_bytes(identity) {
            Ok(bytes) => {
                snapshot.revision = Some(byte_revision(Some(&bytes)));
                snapshot.raw_bytes = Some(bytes);
            }
            Err(error) => {
                snapshot.unavailable = Some(unavailable("glossary_read_unavailable", error));
                return snapshot;
            }
        }
        let parsed = snapshot
            .raw_bytes
            .as_deref()
            .map(crate::workflow_operation::utf8_text)
            .transpose()
            .and_then(|text| glossary::parse_glossary_document(text));
        match parsed {
            Ok(parsed) => {
                snapshot.document = Some(parsed.document);
                snapshot.needs_migration = parsed.needs_migration;
                snapshot.status = GuidanceStatus::Available;
            }
            Err(error) => {
                snapshot.unavailable = Some(unavailable("glossary_parse_unavailable", error))
            }
        }
        snapshot
    }
    pub fn report(&self) -> GuidanceReport {
        GuidanceReport {
            status: self.status,
            revision: self.revision.clone(),
            issues: Vec::new(),
            unavailable: self.unavailable.clone(),
        }
    }
    pub fn check_destinations(
        &self,
        catalog: &XcStringsFile,
        contexts: &CatalogContexts,
        destinations: &[TranslationDestination],
    ) -> GuidanceReport {
        let mut report = self.report();
        let Some(document) = &self.document else {
            return report;
        };
        let mut seen = HashSet::new();
        for destination in destinations {
            if !seen.insert((&destination.key, &destination.locale, &destination.path)) {
                continue;
            }
            let Some(entry) = catalog.strings.get(&destination.key) else {
                continue;
            };
            let assessment = assessment::assess(
                &destination.key,
                entry,
                &catalog.source_language,
                &destination.locale,
            );
            let Some(leaf) = assessment
                .leaves
                .iter()
                .find(|leaf| leaf.path == destination.path)
            else {
                continue;
            };
            let resolved =
                match context::resolve_context(contexts, &destination.key, &destination.path) {
                    Ok(resolved) => resolved,
                    Err(issues) => {
                        report.status = GuidanceStatus::Unavailable;
                        report.unavailable = Some(GuidanceDiagnostic {
                            code: "context_qa_unavailable".into(),
                            detail: issues
                                .iter()
                                .map(|i| i.detail.as_str())
                                .collect::<Vec<_>>()
                                .join("; "),
                        });
                        continue;
                    }
                };
            let checked = glossary::check_terminology(
                document,
                &glossary::TerminologyInput {
                    key: &destination.key,
                    source_locale: &catalog.source_language,
                    target_locale: &destination.locale,
                    path: &destination.path,
                    source_text: &leaf.source_text,
                    target_text: leaf.value.as_deref(),
                    context: &resolved,
                },
            );
            report.issues.extend(checked.issues);
        }
        report
    }
    pub fn check_catalog(
        &self,
        catalog: &XcStringsFile,
        contexts: &CatalogContexts,
        locale: Option<&str>,
    ) -> GuidanceReport {
        let mut destinations = Vec::new();
        for (key, entry) in &catalog.strings {
            if !entry.should_translate {
                continue;
            }
            for target in entry
                .localizations
                .iter()
                .flat_map(|l| l.keys())
                .filter(|l| {
                    *l != &catalog.source_language
                        && locale.is_none_or(|selected| selected == l.as_str())
                })
            {
                destinations.extend(
                    assessment::assess(key, entry, &catalog.source_language, target)
                        .leaves
                        .into_iter()
                        .filter(|leaf| leaf.value.is_some())
                        .map(|leaf| TranslationDestination {
                            key: key.clone(),
                            locale: target.clone(),
                            path: leaf.path,
                        }),
                );
            }
        }
        self.check_destinations(catalog, contexts, &destinations)
    }
}
fn unavailable(code: &str, error: impl std::fmt::Display) -> GuidanceDiagnostic {
    GuidanceDiagnostic {
        code: code.into(),
        detail: error.to_string(),
    }
}

/// Resolve the parent but refuse final-component aliases that could redirect policy writes.
pub(crate) fn policy_identity(
    store: &dyn FileStore,
    path: &Path,
) -> Result<PathBuf, XcStringsError> {
    let name = path
        .file_name()
        .ok_or_else(|| XcStringsError::InvalidPath {
            path: path.into(),
            reason: "glossary path requires a filename".into(),
        })?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let expected = store.file_identity(parent)?.join(name);
    let actual = store.file_identity(path)?;
    if actual != expected {
        return Err(XcStringsError::InvalidPath {
            path: path.into(),
            reason: "glossary aliases are not supported".into(),
        });
    }
    Ok(actual)
}

#[cfg(test)]
mod tests;
