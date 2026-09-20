use crate::{
    error::XcStringsError,
    guidance_operation::GuidanceSnapshot,
    io::FileStore,
    model::glossary::{GlossaryEdit, GlossaryTerm},
    service::glossary,
    workflow_operation::byte_revision,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use tokio::sync::Mutex;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetGlossaryParams {
    pub source_locale: String,
    pub target_locale: String,
    /// Case-insensitive lookup filter, independent of exact terminology matching.
    #[serde(default)]
    pub filter: Option<String>,
}
pub(crate) async fn handle_get_glossary(
    store: &dyn FileStore,
    path: &Path,
    params: GetGlossaryParams,
) -> Result<Value, XcStringsError> {
    let snapshot = GuidanceSnapshot::load(store, path);
    let Some(document) = &snapshot.document else {
        return Ok(
            json!({"status":snapshot.status,"revision":snapshot.revision,"unavailable":snapshot.unavailable,"source_locale":params.source_locale,"target_locale":params.target_locale,"terms":[],"entries":{},"count":0,"term_count":0}),
        );
    };
    let projection = glossary::project_legacy_entries(
        document,
        &params.source_locale,
        &params.target_locale,
        params.filter.as_deref(),
    );
    let terms: Vec<_> = document
        .terms
        .iter()
        .filter(|term| {
            term.source_locale == params.source_locale && term.target_locale == params.target_locale
        })
        .filter(|term| matches_filter(term, params.filter.as_deref()))
        .collect();
    Ok(
        json!({"status":snapshot.status,"revision":snapshot.revision,"unavailable":snapshot.unavailable,"needs_migration":snapshot.needs_migration,"source_locale":params.source_locale,"target_locale":params.target_locale,"count":projection.entries.len(),"term_count":terms.len(),"entries":projection.entries,"omitted_term_ids":projection.omitted_term_ids,"terms":terms}),
    )
}
fn matches_filter(term: &GlossaryTerm, filter: Option<&str>) -> bool {
    filter.is_none_or(|filter| {
        let filter = filter.to_lowercase();
        std::iter::once(&term.source)
            .chain(&term.source_variants)
            .chain(&term.preferred)
            .chain(&term.accepted_variants)
            .chain(&term.forbidden)
            .any(|form| form.to_lowercase().contains(&filter))
    })
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct UpdateGlossaryParams {
    /// Required together with target_locale for the legacy entries adapter.
    #[serde(default)]
    pub source_locale: Option<String>,
    #[serde(default)]
    pub target_locale: Option<String>,
    /// Legacy source-to-preferred map. Cannot be combined with upsert/remove_ids.
    #[serde(default)]
    pub entries: Option<BTreeMap<String, String>>,
    /// Complete rules to create or replace by stable ID; preserve unknown fields on replacement.
    #[serde(default)]
    pub upsert: Vec<GlossaryTerm>,
    #[serde(default)]
    pub remove_ids: Vec<String>,
    /// Exact policy revision obtained from get_glossary or a dry run. Required on apply.
    #[serde(default)]
    pub expected_revision: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
}
pub(crate) async fn handle_update_glossary(
    store: &dyn FileStore,
    path: &Path,
    write_lock: &Mutex<()>,
    params: UpdateGlossaryParams,
) -> Result<Value, XcStringsError> {
    let _guard = write_lock.lock().await;
    let snapshot = GuidanceSnapshot::load(store, path);
    let document = snapshot.document.as_ref().ok_or_else(|| {
        XcStringsError::GlossaryError(
            snapshot
                .unavailable
                .as_ref()
                .map_or("glossary unavailable".into(), |d| d.detail.clone()),
        )
    })?;
    let revision = snapshot
        .revision
        .as_deref()
        .ok_or_else(|| XcStringsError::GlossaryError("glossary revision unavailable".into()))?;
    if params
        .expected_revision
        .as_deref()
        .is_some_and(|expected| expected != revision)
    {
        return Err(XcStringsError::GlossaryError(
            "stale_glossary_revision".into(),
        ));
    }
    if !params.dry_run && params.expected_revision.is_none() {
        return Err(XcStringsError::GlossaryError(
            "glossary apply requires expected_revision".into(),
        ));
    }
    let (candidate, updated) = if let Some(entries) = &params.entries {
        if !params.upsert.is_empty() || !params.remove_ids.is_empty() {
            return Err(XcStringsError::GlossaryError(
                "conflicting_glossary_edit: entries cannot be combined with rich edits".into(),
            ));
        }
        let source = params.source_locale.as_deref().ok_or_else(|| {
            XcStringsError::GlossaryError("entries requires source_locale".into())
        })?;
        let target = params.target_locale.as_deref().ok_or_else(|| {
            XcStringsError::GlossaryError("entries requires target_locale".into())
        })?;
        (
            glossary::legacy_upsert(document, source, target, entries),
            entries.len(),
        )
    } else {
        if params.upsert.is_empty() && params.remove_ids.is_empty() {
            return Err(XcStringsError::GlossaryError("empty_glossary_edit".into()));
        }
        let count = params.upsert.len() + params.remove_ids.len();
        (
            glossary::apply_glossary_edit(
                document,
                &GlossaryEdit {
                    upsert: params.upsert,
                    remove_ids: params.remove_ids,
                },
            ),
            count,
        )
    };
    let candidate = match candidate {
        Ok(candidate) => candidate,
        Err(rejected) => {
            return Ok(
                json!({"written":false,"dry_run":params.dry_run,"updated":0,"revision":revision,"rejected":rejected}),
            );
        }
    };
    let changed = candidate != *document || snapshot.needs_migration;
    let content = glossary::serialize_glossary_document(&candidate)?;
    if changed && !params.dry_run {
        let identity = snapshot
            .identity
            .as_deref()
            .ok_or_else(|| XcStringsError::GlossaryError("glossary identity unavailable".into()))?;
        store.write_if_matches(identity, snapshot.raw_bytes.as_deref(), &content)?;
    }
    let written = changed && !params.dry_run;
    Ok(
        json!({"updated":updated,"source_locale":params.source_locale,"target_locale":params.target_locale,"written":written,"changed":changed,"dry_run":params.dry_run,"revision":if written {byte_revision(Some(content.as_bytes()))} else {revision.to_owned()},"input_revision":revision,"rejected":[],"terms":candidate.terms}),
    )
}

#[cfg(test)]
#[path = "glossary_tests.rs"]
mod tests;
