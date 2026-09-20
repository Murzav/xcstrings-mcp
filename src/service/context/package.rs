use super::validation::diagnostic;
use crate::{
    error::XcStringsError,
    model::{
        context::*,
        glossary::{GlossaryDocument, TermSelection},
        specifier::{FormatArgumentRole, observed_format_arguments},
        xcstrings::{
            Localization, XcStringsFile, paths::LeafStep, references::substitution_references,
        },
    },
};
use std::collections::BTreeSet;

pub fn build_context_package(
    file: &XcStringsFile,
    contexts: &CatalogContexts,
    glossary: Option<&GlossaryDocument>,
    key: &str,
    locale: &str,
    neighbor_count: usize,
) -> Result<ContextPackage, XcStringsError> {
    let entry = file
        .strings
        .get(key)
        .ok_or_else(|| XcStringsError::KeyNotFound(key.into()))?;
    let current = crate::service::extractor::build_translation_unit(
        key,
        entry,
        &file.source_language,
        locale,
    );
    let source = entry
        .localizations
        .as_ref()
        .and_then(|locs| locs.get(&file.source_language));
    let source_origin =
        if source.is_some_and(|s| crate::service::assessment::source_unit(s, &[]).is_some()) {
            SourceTextOrigin::Catalog
        } else {
            SourceTextOrigin::KeyFallback
        };
    let selected: CatalogContexts = contexts
        .get(key)
        .map(|a| std::collections::BTreeMap::from([(key.into(), a.clone())]))
        .unwrap_or_default();
    let mut diagnostics = super::validate_context_bindings(file, &selected);
    let authored = resolve_for_package(contexts, key, &[], &mut diagnostics);
    let mut leaf_contexts = Vec::with_capacity(current.leaves.len());
    for leaf in &current.leaves {
        let resolved = resolve_for_package(contexts, key, &leaf.path, &mut diagnostics);
        let (variables, issues) =
            observe_variables(key, &leaf.path, &leaf.source_text, source, &resolved);
        diagnostics.extend(issues);
        for screenshot in resolved.fields.screenshots.iter().flatten() {
            diagnostics.push(diagnostic(
                "screenshot_availability_unverified",
                key,
                Some(&leaf.path),
                format!(
                    "Authored screenshot reference {} has not been opened or fetched",
                    screenshot.uri
                ),
            ));
        }
        let terminology = glossary.map_or_else(TermSelection::default, |doc| {
            crate::service::glossary::relevant_terms(
                doc,
                &crate::service::glossary::TerminologyInput {
                    key,
                    source_locale: &file.source_language,
                    target_locale: locale,
                    path: &leaf.path,
                    source_text: &leaf.source_text,
                    target_text: leaf.value.as_deref(),
                    context: &resolved,
                },
            )
        });
        leaf_contexts.push(LeafContext {
            path: leaf.path.clone(),
            authored: resolved,
            variables,
            terminology,
        });
    }
    let candidates = neighbors(file, contexts, key, &authored, &leaf_contexts);
    let limit = neighbor_count.min(50);
    let neighbors_truncated = candidates.len() > limit;
    let neighbors = candidates
        .into_iter()
        .take(limit)
        .map(|(neighbor, relation)| ContextNeighbor {
            unit: crate::service::extractor::build_translation_unit(
                neighbor,
                &file.strings[neighbor],
                &file.source_language,
                locale,
            ),
            relation,
        })
        .collect();
    diagnostics.dedup();
    Ok(ContextPackage {
        current,
        source_origin,
        authored,
        leaf_contexts,
        neighbors,
        neighbors_truncated,
        diagnostics,
    })
}
fn resolve_for_package(
    contexts: &CatalogContexts,
    key: &str,
    path: &[LeafStep],
    diagnostics: &mut Vec<ContextDiagnostic>,
) -> ResolvedContext {
    match super::resolve_context(contexts, key, path) {
        Ok(context) => context,
        Err(issues) => {
            diagnostics.extend(issues);
            ResolvedContext::default()
        }
    }
}
fn neighbors<'a>(
    file: &'a XcStringsFile,
    contexts: &CatalogContexts,
    key: &str,
    authored: &ResolvedContext,
    leaves: &[LeafContext],
) -> Vec<(&'a str, NeighborRelation)> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    seen.insert(key);
    let mut ordered_leaves: Vec<_> = leaves.iter().collect();
    ordered_leaves.sort_by_key(|leaf| serde_json::to_string(&leaf.path).unwrap_or_default());
    let contexts_in_order: Vec<_> = std::iter::once(authored)
        .chain(ordered_leaves.iter().map(|leaf| &leaf.authored))
        .collect();
    for explicit in contexts_in_order
        .iter()
        .flat_map(|context| context.fields.neighbors.iter().flatten())
    {
        if let Some((stored, _)) = file.strings.get_key_value(explicit)
            && seen.insert(stored)
        {
            result.push((stored.as_str(), NeighborRelation::Explicit));
        }
    }
    let screens: BTreeSet<_> = contexts_in_order
        .iter()
        .filter_map(|context| context.fields.screen.as_ref())
        .collect();
    let mut same_screen: Vec<_> = contexts
        .iter()
        .filter(|(_, context)| {
            context
                .context
                .screen
                .as_ref()
                .is_some_and(|screen| screens.contains(screen))
                || context.leaves.iter().any(|leaf| {
                    leaf.context
                        .screen
                        .as_ref()
                        .is_some_and(|screen| screens.contains(screen))
                })
        })
        .map(|(key, _)| key)
        .collect();
    same_screen.sort();
    for neighbor in same_screen {
        if let Some((stored, _)) = file.strings.get_key_value(neighbor)
            && seen.insert(stored)
        {
            result.push((stored.as_str(), NeighborRelation::SameScreen));
        }
    }
    let segments: Vec<_> = key.split('.').collect();
    let mut prefix: Vec<_> = file
        .strings
        .keys()
        .filter_map(|k| {
            let score = super::shared_prefix_length(&segments, k);
            (score > 0).then_some((score, k))
        })
        .collect();
    prefix.sort_by(|(a, ka), (b, kb)| b.cmp(a).then_with(|| ka.cmp(kb)));
    for (_, neighbor) in prefix {
        if seen.insert(neighbor) {
            result.push((neighbor.as_str(), NeighborRelation::PrefixHeuristic));
        }
    }
    result
}

pub(super) fn observe_variables(
    key: &str,
    path: &[LeafStep],
    text: &str,
    source: Option<&Localization>,
    resolved: &ResolvedContext,
) -> (Vec<ObservedVariable>, Vec<ContextDiagnostic>) {
    let mut out = Vec::new();
    let mut diagnostics = Vec::new();
    let mut expanded = String::new();
    let mut cursor = 0;
    let references = match substitution_references(text) {
        Ok(refs) => refs,
        Err(error) => {
            diagnostics.push(diagnostic(
                "invalid_source_variables",
                key,
                Some(path),
                error,
            ));
            return (out, diagnostics);
        }
    };
    for reference in references {
        expanded.push_str(&text[cursor..reference.start]);
        let metadata = source
            .and_then(|s| s.substitutions.as_ref())
            .and_then(|s| s.get(&reference.name));
        if let Some(metadata) = metadata.filter(|m| m.validate_metadata().is_ok()) {
            if let (Some(position), Some(format)) =
                (metadata.arg_num, metadata.format_specifier.as_ref())
            {
                expanded.push_str(&format!("%{position}${format}"));
                out.push(observed(
                    VariableReference::Substitution(reference.name),
                    Some(position),
                    ArgumentRole::Substitution,
                    Some(format!("%{format}")),
                    resolved,
                ));
            }
        } else {
            expanded.push(' ');
            diagnostics.push(diagnostic(
                "unknown_source_substitution",
                key,
                Some(path),
                format!("Missing valid metadata for {}", reference.name),
            ));
        }
        cursor = reference.end;
    }
    expanded.push_str(&text[cursor..]);
    if let Some(LeafStep::Substitution(name)) = path.first() {
        let metadata = source
            .and_then(|s| s.substitutions.as_ref())
            .and_then(|s| s.get(name));
        if let Some(metadata) = metadata.filter(|m| m.validate_metadata().is_ok())
            && let (Some(position), Some(format)) =
                (metadata.arg_num, metadata.format_specifier.as_ref())
        {
            // Exact token replacement is delegated to the catalog's substitution parser.
            expanded = replace_arg(&expanded, &format!("%{position}${format}"));
            out.push(observed(
                VariableReference::Substitution(name.clone()),
                Some(position),
                ArgumentRole::Substitution,
                Some(format!("%{format}")),
                resolved,
            ));
        }
    }
    match observed_format_arguments(&expanded) {
        Ok(arguments) => {
            for argument in arguments {
                if out.iter().any(|v| {
                    v.position == Some(argument.position) && v.role == ArgumentRole::Substitution
                }) {
                    continue;
                }
                let role = match argument.role {
                    FormatArgumentRole::Value => ArgumentRole::Value,
                    FormatArgumentRole::Width => ArgumentRole::Width,
                    FormatArgumentRole::Precision => ArgumentRole::Precision,
                };
                out.push(observed(
                    VariableReference::Argument(argument.position),
                    Some(argument.position),
                    role,
                    Some(argument.format),
                    resolved,
                ));
            }
        }
        Err(issues) => diagnostics.extend(
            issues
                .into_iter()
                .map(|i| diagnostic("invalid_source_variables", key, Some(path), i.message)),
        ),
    }
    out.sort_by(|a, b| {
        a.position
            .cmp(&b.position)
            .then_with(|| a.reference.cmp(&b.reference))
    });
    out.dedup_by(|a, b| a.reference == b.reference && a.role == b.role && a.format == b.format);
    (out, diagnostics)
}
fn observed(
    reference: VariableReference,
    position: Option<u32>,
    role: ArgumentRole,
    format: Option<String>,
    resolved: &ResolvedContext,
) -> ObservedVariable {
    let binding=resolved.fields.variables.iter().flatten().find(|b|b.reference==reference).or_else(||resolved.fields.variables.iter().flatten().find(|b|matches!((&b.reference,position),(VariableReference::Argument(expected),Some(actual)) if *expected==actual)));
    ObservedVariable {
        reference,
        position,
        role,
        format,
        meaning: binding.map(|b| b.meaning.clone()),
        meaning_origin: binding.and_then(|_| resolved.provenance.get("variables").copied()),
    }
}
fn replace_arg(text: &str, replacement: &str) -> String {
    let mut out = String::new();
    let mut offset = 0;
    for range in crate::model::specifier::substitution_placeholder_ranges(text) {
        out.push_str(&text[offset..range.start]);
        out.push_str(replacement);
        offset = range.end;
    }
    out.push_str(&text[offset..]);
    out
}
