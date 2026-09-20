use crate::model::{
    context::*,
    xcstrings::{XcStringsFile, paths::LeafStep},
};
use serde_json::Value;
use std::collections::BTreeSet;

pub fn resolve_context(
    contexts: &CatalogContexts,
    key: &str,
    path: &[LeafStep],
) -> Result<ResolvedContext, Vec<ContextDiagnostic>> {
    let Some(authored) = contexts.get(key) else {
        return Ok(ResolvedContext::default());
    };
    let diagnostics = super::validate_authored_context(key, authored);
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    Ok(resolve_validated(authored, path))
}
pub(super) fn resolve_validated(authored: &AuthoredContext, path: &[LeafStep]) -> ResolvedContext {
    let mut result = ResolvedContext::default();
    overlay(&mut result, &authored.context, ContextOrigin::Key);
    if let Some(leaf) = authored.leaves.iter().find(|leaf| leaf.path == path) {
        overlay(&mut result, &leaf.context, ContextOrigin::Leaf);
    }
    result
}
fn overlay(result: &mut ResolvedContext, fields: &ContextFields, origin: ContextOrigin) {
    macro_rules! field {
        ($name:ident) => {
            if fields.$name.is_some() {
                result.fields.$name = fields.$name.clone();
                result.provenance.insert(stringify!($name).into(), origin);
            }
        };
    }
    field!(screen);
    field!(role);
    field!(purpose);
    field!(variables);
    field!(neighbors);
    field!(screenshots);
    result.fields.extra.extend(fields.extra.clone());
}
/// Semantic fingerprint input, deliberately excluding derived neighbors and glossary.
pub fn authored_snapshot(contexts: &CatalogContexts, key: &str) -> Value {
    let Some(authored) = contexts.get(key) else {
        return Value::Null;
    };
    // Sorting a small authored record avoids storing duplicate catalog state.
    let mut canonical = authored.clone();
    canonical.leaves.sort_by_key(|leaf| path_key(&leaf.path));
    canonicalize_fields(&mut canonical.context);
    for leaf in &mut canonical.leaves {
        canonicalize_fields(&mut leaf.context);
    }
    // All values are JSON-compatible by construction; this fallback cannot erase a parse error.
    serde_json::to_value(canonical).unwrap_or(Value::Null)
}
fn canonicalize_fields(fields: &mut ContextFields) {
    if let Some(variables) = &mut fields.variables {
        variables.sort_by(|a, b| a.reference.cmp(&b.reference));
    }
}
fn path_key(path: &[LeafStep]) -> String {
    serde_json::to_string(path).unwrap_or_default()
}

pub fn apply_context_edits(
    file: &XcStringsFile,
    contexts: &CatalogContexts,
    edits: &[ContextEdit],
) -> Result<CatalogContexts, Vec<ContextDiagnostic>> {
    let mut seen = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for edit in edits {
        let key = match edit {
            ContextEdit::Set { key, .. } | ContextEdit::Remove { key } => key,
        };
        if !seen.insert(key) {
            diagnostics.push(super::validation::diagnostic(
                "duplicate_context_edit",
                key,
                None,
                "Each key may be addressed once",
            ));
        }
        if let ContextEdit::Set { context, .. } = edit {
            if !file.strings.contains_key(key) {
                diagnostics.push(super::validation::diagnostic(
                    "unknown_context_key",
                    key,
                    None,
                    "Catalog key does not exist",
                ));
            }
            diagnostics.extend(super::validate_authored_context(key, context));
        }
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    let mut candidate = contexts.clone();
    for edit in edits {
        match edit {
            ContextEdit::Set { key, context } => {
                candidate.insert(key.clone(), context.as_ref().clone());
            }
            ContextEdit::Remove { key } => {
                candidate.remove(key);
            }
        }
    }
    // Validate newly supplied bindings only; stale unrelated context stays removable.
    let changed: CatalogContexts = edits
        .iter()
        .filter_map(|edit| match edit {
            ContextEdit::Set { key, context } => Some((key.clone(), context.as_ref().clone())),
            _ => None,
        })
        .collect();
    diagnostics.extend(super::validate_context_bindings(file, &changed));
    if diagnostics.is_empty() {
        Ok(candidate)
    } else {
        Err(diagnostics)
    }
}
