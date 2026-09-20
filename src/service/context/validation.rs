use crate::model::{
    context::*,
    xcstrings::{
        XcStringsFile,
        paths::{LeafStep, collect_leaves, validate_supported_path},
    },
};
use std::collections::HashSet;

pub(super) fn diagnostic(
    code: &str,
    key: &str,
    path: Option<&[LeafStep]>,
    detail: impl Into<String>,
) -> ContextDiagnostic {
    ContextDiagnostic {
        code: code.into(),
        key: key.into(),
        path: path.map(<[LeafStep]>::to_vec),
        detail: detail.into(),
    }
}
pub fn validate_authored_context(key: &str, authored: &AuthoredContext) -> Vec<ContextDiagnostic> {
    let mut out = Vec::new();
    let mut paths = HashSet::new();
    validate_fields(key, None, &authored.context, &mut out);
    for leaf in &authored.leaves {
        if !paths.insert(&leaf.path) {
            out.push(diagnostic(
                "duplicate_context_path",
                key,
                Some(&leaf.path),
                "Only one exact-path override is allowed",
            ));
        }
        if let Err(error) = validate_supported_path(&leaf.path) {
            out.push(diagnostic(
                "invalid_context_path",
                key,
                Some(&leaf.path),
                error,
            ));
        }
        validate_fields(key, Some(&leaf.path), &leaf.context, &mut out);
    }
    out
}
fn validate_fields(
    key: &str,
    path: Option<&[LeafStep]>,
    fields: &ContextFields,
    out: &mut Vec<ContextDiagnostic>,
) {
    for (name, value) in [
        ("screen", &fields.screen),
        ("role", &fields.role),
        ("purpose", &fields.purpose),
    ] {
        if value.as_ref().is_some_and(|s| s.trim().is_empty()) {
            out.push(diagnostic(
                "empty_context_field",
                key,
                path,
                format!("{name} must not be blank"),
            ));
        }
    }
    let mut references = std::collections::BTreeSet::new();
    for variable in fields.variables.iter().flatten() {
        if !references.insert(&variable.reference) {
            out.push(diagnostic(
                "duplicate_context_variable",
                key,
                path,
                "Variable meanings require unique references",
            ));
        }
        if matches!(&variable.reference, VariableReference::Argument(0))
            || matches!(&variable.reference,VariableReference::Substitution(name) if name.trim().is_empty())
        {
            out.push(diagnostic(
                "invalid_context_variable",
                key,
                path,
                "Variable positions start at 1 and names must be nonempty",
            ));
        }
        if variable.meaning.trim().is_empty() {
            out.push(diagnostic(
                "empty_variable_meaning",
                key,
                path,
                "Variable meaning must not be blank",
            ));
        }
    }
    let mut neighbors = HashSet::new();
    for neighbor in fields.neighbors.iter().flatten() {
        if neighbor == key || !neighbors.insert(neighbor) {
            out.push(diagnostic(
                "invalid_context_neighbor",
                key,
                path,
                "Neighbors must be unique and distinct from this key",
            ));
        }
    }
    for screenshot in fields.screenshots.iter().flatten() {
        if !safe_reference(&screenshot.uri) {
            out.push(diagnostic(
                "invalid_screenshot_reference",
                key,
                path,
                "Screenshot must be an inert HTTPS URL or a safe relative path",
            ));
        }
    }
}
fn safe_reference(uri: &str) -> bool {
    if uri.is_empty()
        || uri.chars().any(|c| c.is_control() || c.is_whitespace())
        || uri.contains('\\')
    {
        return false;
    }
    if let Some(rest) = uri.strip_prefix("https://") {
        let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
        return !host.is_empty()
            && !host.contains('@')
            && !host.starts_with('.')
            && !host.ends_with('.')
            && !host.contains(':');
    }
    !uri.starts_with('/')
        && !uri.contains(':')
        && !uri.contains('%')
        && !uri.split('/').any(|part| part == ".." || part.is_empty())
}

pub fn validate_context_bindings(
    file: &XcStringsFile,
    contexts: &CatalogContexts,
) -> Vec<ContextDiagnostic> {
    let mut out = Vec::new();
    for (key, authored) in contexts {
        out.extend(validate_authored_context(key, authored));
        let Some(entry) = file.strings.get(key) else {
            out.push(diagnostic(
                "unknown_context_key",
                key,
                None,
                "Authored context refers to a missing catalog key",
            ));
            continue;
        };
        let source = entry
            .localizations
            .as_ref()
            .and_then(|l| l.get(&file.source_language));
        let mut variables = Vec::new();
        if let Some(source) = source {
            for leaf in collect_leaves(source).leaves {
                let (observed, issues) = super::package::observe_variables(
                    key,
                    &leaf.path,
                    &leaf.unit.value,
                    Some(source),
                    &ResolvedContext::default(),
                );
                variables.extend(observed);
                out.extend(issues);
            }
        } else {
            let (observed, issues) =
                super::package::observe_variables(key, &[], key, None, &ResolvedContext::default());
            variables.extend(observed);
            out.extend(issues);
        }
        check_variables(key, None, &authored.context, &variables, &mut out);
        check_neighbors(file, key, None, &authored.context, &mut out);
        for leaf in &authored.leaves {
            let unit = source.and_then(|s| crate::service::assessment::source_unit(s, &leaf.path));
            let target_path_exists = entry
                .localizations
                .iter()
                .flat_map(|locales| locales.values())
                .any(|node| crate::model::xcstrings::paths::find_leaf(node, &leaf.path).is_some());
            if unit.is_none() && !leaf.path.is_empty() && !target_path_exists {
                out.push(diagnostic(
                    "unknown_context_path",
                    key,
                    Some(&leaf.path),
                    "No source leaf can resolve this context path",
                ));
                continue;
            }
            let text = unit.map_or(key.as_str(), |u| &u.value);
            let (observed, issues) = super::package::observe_variables(
                key,
                &leaf.path,
                text,
                source,
                &ResolvedContext::default(),
            );
            out.extend(issues);
            check_variables(key, Some(&leaf.path), &leaf.context, &observed, &mut out);
            check_neighbors(file, key, Some(&leaf.path), &leaf.context, &mut out);
        }
    }
    out
}
fn check_variables(
    key: &str,
    path: Option<&[LeafStep]>,
    fields: &ContextFields,
    observed: &[ObservedVariable],
    out: &mut Vec<ContextDiagnostic>,
) {
    for binding in fields.variables.iter().flatten() {
        if !observed.iter().any(|variable|variable.reference==binding.reference || matches!((&binding.reference,variable.position),(VariableReference::Argument(expected),Some(actual)) if *expected==actual)) {
            out.push(diagnostic("unknown_context_variable",key,path,format!("Variable {:?} does not occur in the source",binding.reference)));
        }
    }
}
fn check_neighbors(
    file: &XcStringsFile,
    key: &str,
    path: Option<&[LeafStep]>,
    fields: &ContextFields,
    out: &mut Vec<ContextDiagnostic>,
) {
    for neighbor in fields.neighbors.iter().flatten() {
        if !file.strings.contains_key(neighbor) {
            out.push(diagnostic(
                "unknown_context_neighbor",
                key,
                path,
                format!("Neighbor {neighbor} does not exist"),
            ));
        }
    }
}
