use crate::error::XcStringsError;
use crate::model::plural::{PluralCategory, plural_categories};
use crate::model::xcstrings::paths::{collect_leaves, find_leaf_mut, validate_supported_path};
use crate::model::xcstrings::{Localization, StringUnit, TranslationState, Variations};

pub(crate) fn required_shape(
    basis: &Localization,
    categories: &[PluralCategory],
    missing_target: bool,
) -> Localization {
    // A temporary shape retains per-branch metadata while deriving missing destinations.
    let mut shape = basis.clone();
    expand(&mut shape, categories, missing_target);
    shape
}

fn expand(node: &mut Localization, categories: &[PluralCategory], missing_target: bool) {
    if let Some(variations) = &mut node.variations {
        expand_variations(variations, categories, missing_target);
    }
    if let Some(subs) = &mut node.substitutions {
        for sub in subs.values_mut() {
            if let Some(variations) = &mut sub.variations {
                expand_variations(variations, categories, missing_target);
            }
        }
    }
}

fn expand_variations(
    variations: &mut Variations,
    categories: &[PluralCategory],
    missing_target: bool,
) {
    if let Some(plural) = &mut variations.plural {
        // Clone only the fallback branch because each synthesized category owns its metadata.
        let fallback = plural
            .get("other")
            .cloned()
            .unwrap_or_else(|| Localization::with_unit(StringUnit::new(TranslationState::New, "")));
        if missing_target {
            plural.retain(|name, _| categories.iter().any(|c| c.as_str() == name));
        }
        for category in categories {
            plural
                .entry(category.as_str().into())
                .or_insert_with(|| fallback.clone());
        }
        for node in plural.values_mut() {
            expand(node, categories, missing_target);
        }
    }
    if let Some(devices) = &mut variations.device {
        for node in devices.values_mut() {
            expand(node, categories, missing_target);
        }
    }
}

pub(crate) fn initialize_locale(
    source: Option<&Localization>,
    locale: &str,
) -> Result<Localization, XcStringsError> {
    let categories = plural_categories(locale)?;
    let default = Localization::with_unit(StringUnit::new(TranslationState::New, ""));
    let source = source.unwrap_or(&default);
    let traversal = collect_leaves(source);
    if let Some(issue) = traversal.diagnostics.first() {
        return Err(XcStringsError::InvalidFormat(issue.detail.clone()));
    }
    for leaf in &traversal.leaves {
        validate_supported_path(&leaf.path).map_err(XcStringsError::InvalidFormat)?;
    }
    let mut result = required_shape(source, &categories, true);
    let paths: Vec<_> = collect_leaves(&result)
        .leaves
        .into_iter()
        .map(|leaf| leaf.path)
        .collect();
    for path in &paths {
        if let Some(unit) = find_leaf_mut(&mut result, path) {
            unit.state = TranslationState::New;
            if crate::model::xcstrings::references::substitution_references(&unit.value)
                .map_err(XcStringsError::InvalidFormat)?
                .is_empty()
            {
                unit.value.clear();
            }
        }
    }
    Ok(result)
}
