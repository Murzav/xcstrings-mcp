use super::validation::{bounded, diagnostic};
use crate::{error::XcStringsError, model::glossary::*};
use std::collections::{BTreeMap, BTreeSet};

pub fn parse_glossary_document(raw: Option<&str>) -> Result<ParsedGlossary, XcStringsError> {
    let Some(raw) = raw else {
        return Ok(ParsedGlossary {
            document: GlossaryDocument::default(),
            needs_migration: false,
        });
    };
    let value = crate::service::parser::parse_unique_json(raw)
        .map_err(|e| XcStringsError::GlossaryError(e.to_string()))?;
    let (document, needs_migration) = if value.get("schema_version").is_some() {
        (
            serde_json::from_value(value)
                .map_err(|e| XcStringsError::GlossaryError(e.to_string()))?,
            false,
        )
    } else {
        let legacy: super::Glossary = serde_json::from_value(value)
            .map_err(|e| XcStringsError::GlossaryError(e.to_string()))?;
        let mut doc = GlossaryDocument::default();
        for (pair, entries) in legacy {
            let Some((source, target)) = pair.split_once('→') else {
                return Err(XcStringsError::GlossaryError(
                    "legacy locale pair must contain →".into(),
                ));
            };
            if target.contains('→') {
                return Err(XcStringsError::GlossaryError(
                    "invalid legacy locale pair".into(),
                ));
            }
            for (term, translation) in entries {
                doc.terms
                    .push(legacy_term(source, target, &term, &translation));
            }
        }
        (doc, true)
    };
    validated(&document)?;
    Ok(ParsedGlossary {
        document,
        needs_migration,
    })
}
pub fn serialize_glossary_document(doc: &GlossaryDocument) -> Result<String, XcStringsError> {
    validated(doc)?;
    serde_json::to_string_pretty(doc).map_err(|e| XcStringsError::GlossaryError(e.to_string()))
}
fn validated(doc: &GlossaryDocument) -> Result<(), XcStringsError> {
    let issues = super::validate_glossary(doc);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(XcStringsError::GlossaryError(
            issues
                .into_iter()
                .map(|i| format!("{}: {}", i.code, i.detail))
                .collect::<Vec<_>>()
                .join("; "),
        ))
    }
}
pub fn apply_glossary_edit(
    doc: &GlossaryDocument,
    edit: &GlossaryEdit,
) -> Result<GlossaryDocument, Vec<GlossaryDiagnostic>> {
    let mut ids = BTreeSet::new();
    for id in edit.upsert.iter().map(|t| &t.id).chain(&edit.remove_ids) {
        if !ids.insert(id) {
            return Err(vec![diagnostic(
                "duplicate_edit_id",
                Some(id),
                "Each term may be addressed once per edit",
            )]);
        }
    }
    // A candidate retains all unaddressed rules and unknown metadata.
    let mut candidate = doc.clone();
    candidate
        .terms
        .retain(|term| !edit.remove_ids.contains(&term.id));
    for term in &edit.upsert {
        if let Some(existing) = candidate.terms.iter_mut().find(|t| t.id == term.id) {
            *existing = term.clone();
        } else {
            candidate.terms.push(term.clone());
        }
    }
    let issues = super::validate_glossary(&candidate);
    if issues.is_empty() {
        Ok(candidate)
    } else {
        Err(issues)
    }
}
fn legacy_id(source: &str, target: &str, term: &str) -> String {
    // Length delimiters avoid aliases between arbitrary Unicode term strings.
    format!(
        "legacy:{}:{source}:{}:{target}:{}:{term}",
        source.len(),
        target.len(),
        term.len()
    )
}
fn legacy_term(source: &str, target: &str, term: &str, translation: &str) -> GlossaryTerm {
    GlossaryTerm {
        id: legacy_id(source, target, term),
        source_locale: source.into(),
        target_locale: target.into(),
        source: term.into(),
        preferred: vec![translation.into()],
        ..Default::default()
    }
}
pub fn legacy_upsert(
    doc: &GlossaryDocument,
    source: &str,
    target: &str,
    entries: &BTreeMap<String, String>,
) -> Result<GlossaryDocument, Vec<GlossaryDiagnostic>> {
    let mut edit = GlossaryEdit::default();
    for (term, translation) in entries {
        let id = legacy_id(source, target, term);
        let updated = if let Some(existing) = doc.terms.iter().find(|t| t.id == id) {
            if existing.source != *term
                || existing.source_locale != source
                || existing.target_locale != target
                || bounded(&existing.scope)
            {
                return Err(vec![diagnostic(
                    "legacy_identity_collision",
                    Some(&id),
                    "Existing rule does not represent this legacy entry",
                )]);
            }
            let mut updated = existing.clone();
            updated.preferred = vec![translation.clone()];
            updated
        } else {
            legacy_term(source, target, term, translation)
        };
        edit.upsert.push(updated);
    }
    apply_glossary_edit(doc, &edit)
}
pub fn project_legacy_entries(
    doc: &GlossaryDocument,
    source: &str,
    target: &str,
    filter: Option<&str>,
) -> GlossaryProjection {
    let mut result = GlossaryProjection::default();
    let mut candidates: BTreeMap<&str, Vec<&GlossaryTerm>> = BTreeMap::new();
    for term in doc
        .terms
        .iter()
        .filter(|t| t.source_locale == source && t.target_locale == target)
    {
        candidates.entry(&term.source).or_default().push(term);
    }
    for (source, terms) in candidates {
        let simple: Vec<_> = terms.iter().filter(|t| representable(t)).collect();
        for term in &terms {
            if !representable(term) || simple.len() != 1 {
                result.omitted_term_ids.push(term.id.clone());
            }
        }
        if let [term] = simple.as_slice() {
            let value = &term.preferred[0];
            if filter.is_none_or(|f| {
                source.to_lowercase().contains(&f.to_lowercase())
                    || value.to_lowercase().contains(&f.to_lowercase())
            }) {
                result.entries.insert(source.into(), value.clone());
            }
        }
    }
    result.omitted_term_ids.sort();
    result
}
fn representable(t: &GlossaryTerm) -> bool {
    !bounded(&t.scope)
        && t.scope.extra.is_empty()
        && t.preferred.len() == 1
        && t.source_variants.is_empty()
        && t.accepted_variants.is_empty()
        && t.forbidden.is_empty()
        && !t.do_not_translate
        && t.exceptions.is_empty()
        && t.match_mode == TermMatchMode::Word
}
