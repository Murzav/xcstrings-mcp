//! Normalize native requests before validation or mutation.
pub(crate) mod formats;
pub(crate) mod mutation;
use crate::model::plural::plural_categories;
use crate::model::translation::{
    CompletedTranslation, RejectedTranslation, TranslationDestination,
};
use crate::model::xcstrings::{
    XcStringsFile,
    paths::{LeafStep, validate_supported_path},
};
use std::collections::HashMap;

#[derive(Debug)]
pub(crate) struct PlannedRequest {
    pub index: usize,
    pub leaves: Vec<(TranslationDestination, String)>,
}

pub(crate) fn reject(
    request: &CompletedTranslation,
    code: &str,
    reason: impl Into<String>,
) -> RejectedTranslation {
    RejectedTranslation {
        key: request.key.clone(),
        locale: Some(request.locale.clone()),
        path: request.path.clone(),
        code: Some(code.into()),
        reason: reason.into(),
    }
}

pub(crate) fn prepare(
    file: &XcStringsFile,
    requests: &[CompletedTranslation],
) -> (Vec<PlannedRequest>, Vec<Option<RejectedTranslation>>) {
    let mut rejected = vec![None; requests.len()];
    let mut plans = Vec::new();
    let mut owners: HashMap<TranslationDestination, Vec<usize>> = HashMap::new();
    for (index, request) in requests.iter().enumerate() {
        match normalize(file, request) {
            Ok(leaves) => {
                for (destination, _) in &leaves {
                    owners.entry(destination.clone()).or_default().push(index);
                }
                plans.push(PlannedRequest { index, leaves });
            }
            Err(error) => rejected[index] = Some(error),
        }
    }
    for indexes in owners.values().filter(|indexes| indexes.len() > 1) {
        for &index in indexes {
            rejected[index] = Some(reject(
                &requests[index],
                "duplicate_destination",
                "duplicate translation destination",
            ));
        }
    }
    let destinations: Vec<_> = owners.keys().collect();
    for (i, left) in destinations.iter().enumerate() {
        for right in &destinations[i + 1..] {
            if left.key == right.key
                && left.locale == right.locale
                && overlapping_paths(&left.path, &right.path)
            {
                for index in owners[*left].iter().chain(&owners[*right]) {
                    rejected[*index] = Some(reject(
                        &requests[*index],
                        "duplicate_destination",
                        "duplicate translation destination",
                    ));
                }
            }
        }
    }
    plans.retain(|plan| rejected[plan.index].is_none());
    (plans, rejected)
}

fn normalize(
    file: &XcStringsFile,
    request: &CompletedTranslation,
) -> Result<Vec<(TranslationDestination, String)>, RejectedTranslation> {
    if request.path.is_some()
        && (request.plural_forms.is_some() || request.substitution_name.is_some())
    {
        return Err(reject(
            request,
            "conflicting_selectors",
            "path cannot be combined with plural_forms or substitution_name",
        ));
    }
    let entry = file
        .strings
        .get(&request.key)
        .ok_or_else(|| reject(request, "unknown_key", "key not found in file"))?;
    if !entry.should_translate {
        return Err(reject(
            request,
            "not_translatable",
            "key is marked as shouldTranslate=false",
        ));
    }
    if request.locale == file.source_language {
        return Err(reject(
            request,
            "source_locale",
            "cannot submit a translation to the source locale",
        ));
    }
    plural_categories(&request.locale)
        .map_err(|error| reject(request, "unknown_locale", error.to_string()))?;
    let paths: Vec<_> = if let Some(path) = &request.path {
        vec![(path.clone(), request.value.clone())]
    } else if let Some(forms) = &request.plural_forms {
        if forms.is_empty() {
            return Err(reject(
                request,
                "invalid_translation",
                "plural_forms must contain at least one destination",
            ));
        }
        forms
            .iter()
            .map(|(category, value)| {
                let mut path = Vec::new();
                if let Some(name) = &request.substitution_name {
                    path.push(LeafStep::Substitution(name.clone()));
                }
                path.push(LeafStep::Plural(category.clone()));
                (path, value.clone())
            })
            .collect()
    } else {
        if request.substitution_name.is_some() {
            return Err(reject(
                request,
                "conflicting_selectors",
                "substitution_name requires plural_forms",
            ));
        }
        vec![(Vec::new(), request.value.clone())]
    };
    let mut leaves = Vec::with_capacity(paths.len());
    for (path, value) in paths {
        validate_supported_path(&path).map_err(|reason| reject(request, "invalid_path", reason))?;
        if request.path.is_some() {
            for (index, step) in path.iter().enumerate() {
                if !matches!(step, LeafStep::Substitution(_)) {
                    continue;
                }
                let parent = entry.localizations.as_ref().and_then(|locs| {
                    locs.get(&request.locale)
                        .and_then(|node| formats::node(node, &path[..index]))
                        .filter(|node| has_parent(node, step))
                        .or_else(|| {
                            locs.get(&file.source_language)
                                .and_then(|node| formats::node(node, &path[..index]))
                        })
                });
                if parent.is_none_or(|node| !has_parent(node, step)) {
                    return Err(reject(
                        request,
                        "unsupported_shape",
                        "substitution leaf requires an explicit parent stringUnit or a source parent template",
                    ));
                }
            }
        }
        leaves.push((
            TranslationDestination {
                key: request.key.clone(),
                locale: request.locale.clone(),
                path,
            },
            value,
        ));
    }
    Ok(leaves)
}

fn overlapping_paths(left: &[LeafStep], right: &[LeafStep]) -> bool {
    let common = left.iter().zip(right).take_while(|(a, b)| a == b).count();
    match (left.get(common), right.get(common)) {
        (None, Some(step)) | (Some(step), None) => !matches!(step, LeafStep::Substitution(_)),
        (Some(LeafStep::Device(_)), Some(LeafStep::Device(_)))
        | (Some(LeafStep::Plural(_)), Some(LeafStep::Plural(_)))
        | (Some(LeafStep::Substitution(_)), Some(LeafStep::Substitution(_))) => false,
        (Some(LeafStep::Substitution(_)), Some(_)) | (Some(_), Some(LeafStep::Substitution(_))) => {
            false
        }
        (Some(_), Some(_)) | (None, None) => true,
    }
}

fn has_parent(node: &crate::model::xcstrings::Localization, step: &LeafStep) -> bool {
    if node.string_unit.is_some() {
        return true;
    }
    let LeafStep::Substitution(name) = step else {
        return false;
    };
    crate::model::xcstrings::paths::collect_leaves(node)
        .leaves
        .iter()
        .any(|leaf| {
            !leaf
                .path
                .iter()
                .any(|step| matches!(step, LeafStep::Substitution(_)))
                && crate::model::xcstrings::references::substitution_references(&leaf.unit.value)
                    .is_ok_and(|refs| refs.iter().any(|reference| &reference.name == name))
        })
}
