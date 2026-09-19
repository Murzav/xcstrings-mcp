use super::{
    apple_import::Work,
    apple_substitutions::{self, Metadata},
};
use crate::model::xcstrings::{
    Localization, Substitution, Variations, XcStringsFile, paths::LeafStep,
};

pub(super) fn apply(
    file: &mut XcStringsFile,
    work: &Work<'_>,
    value: String,
    metadata: &Metadata,
) -> Result<(), String> {
    let source = file
        .strings
        .get(&work.destination.key)
        .and_then(|entry| entry.localizations.as_ref())
        .and_then(|locales| locales.get(&file.source_language));
    let needs_substitution = work
        .destination
        .path
        .iter()
        .any(|step| matches!(step, LeafStep::Substitution(_)))
        || !apple_substitutions::references(&value)?.is_empty();
    // The candidate owns the initialized source shape independently of its source locale.
    let template = if needs_substitution {
        source
            .map(|source| {
                crate::service::assessment::initialize_locale(
                    Some(source),
                    &work.destination.locale,
                )
            })
            .transpose()
            .map_err(|error| error.to_string())?
    } else {
        None
    };
    let entry = file
        .strings
        .get_mut(&work.destination.key)
        .ok_or("catalog key disappeared")?;
    let loc = entry
        .localizations
        .get_or_insert_with(Default::default)
        .entry(work.destination.locale.clone())
        .or_insert_with(|| template.clone().unwrap_or_default());
    let node = ensure_node(
        loc,
        &work.destination.path,
        &work.destination.key,
        &mut Vec::new(),
        metadata,
        template.as_ref(),
    )?;
    if node.variations.is_some() {
        return Err("simple target write would flatten an existing variation tree".into());
    }
    let references = apple_substitutions::references(&value)?;
    node.set_translation(work.state.clone(), value);
    for reference in references {
        if let Some(sub) = template
            .as_ref()
            .and_then(|root| root.substitutions.as_ref())
            .and_then(|subs| subs.get(&reference.name))
        {
            loc.substitutions
                .get_or_insert_with(Default::default)
                .entry(reference.name)
                .or_insert_with(|| sub.clone());
        }
    }
    Ok(())
}
fn ensure_node<'a>(
    node: &'a mut Localization,
    path: &[LeafStep],
    key: &str,
    parent: &mut Vec<LeafStep>,
    metadata: &Metadata,
    source: Option<&Localization>,
) -> Result<&'a mut Localization, String> {
    let Some((step, rest)) = path.split_first() else {
        return Ok(node);
    };
    if let LeafStep::Substitution(name) = step {
        if node.string_unit.is_none()
            && let Some(parent_unit) = source
                .and_then(|s| apple_substitutions::node(s, parent))
                .and_then(|n| n.string_unit.as_ref())
        {
            node.string_unit = Some(parent_unit.clone());
        }
        let template = metadata
            .get(&(key.into(), parent.clone(), name.clone()))
            .ok_or("missing substitution metadata")?;
        let sub = node
            .substitutions
            .get_or_insert_with(Default::default)
            .entry(name.clone())
            .or_insert_with(|| {
                // Preserve metadata, but initialize only the target leaves actually submitted.
                let mut sub = template.clone();
                sub.variations = None;
                sub.layout.forget("variations");
                sub
            });
        check_metadata(sub, template)?;
        parent.push(step.clone());
        let result = ensure_variation(
            sub.variations.get_or_insert_with(Variations::default),
            rest,
            key,
            parent,
            metadata,
            source,
        );
        parent.pop();
        return result;
    }
    if let Some(unit) = &node.string_unit {
        if !unit.extra.is_empty()
            || !unit.value.is_empty()
            || unit.state != crate::model::xcstrings::TranslationState::New
        {
            return Err("variation write would replace an existing simple translation".into());
        }
        node.string_unit = None;
        node.layout.forget("stringUnit");
    }
    ensure_variation(
        node.variations.get_or_insert_with(Variations::default),
        path,
        key,
        parent,
        metadata,
        source,
    )
}
fn check_metadata(actual: &Substitution, expected: &Substitution) -> Result<(), String> {
    if actual.arg_num != expected.arg_num || actual.format_specifier != expected.format_specifier {
        return Err("substitution metadata conflicts with the existing target".into());
    }
    Ok(())
}
fn ensure_variation<'a>(
    variations: &'a mut Variations,
    path: &[LeafStep],
    key: &str,
    parent: &mut Vec<LeafStep>,
    metadata: &Metadata,
    source: Option<&Localization>,
) -> Result<&'a mut Localization, String> {
    if !variations.extra.is_empty() {
        return Err("cannot modify an unsupported variation axis".into());
    }
    let Some((step, rest)) = path.split_first() else {
        return Err("incomplete variation path".into());
    };
    let node = match step {
        LeafStep::Device(device) => {
            if variations.plural.is_some() {
                return Err("device path conflicts with a plural axis".into());
            }
            variations
                .device
                .get_or_insert_with(Default::default)
                .entry(device.clone())
                .or_default()
        }
        LeafStep::Plural(category) => {
            if variations.device.is_some() {
                return Err("plural path conflicts with a device axis".into());
            }
            variations
                .plural
                .get_or_insert_with(Default::default)
                .entry(category.clone())
                .or_default()
        }
        LeafStep::Substitution(_) => {
            return Err("substitution cannot directly follow another substitution".into());
        }
    };
    parent.push(step.clone());
    let result = ensure_node(node, rest, key, parent, metadata, source);
    parent.pop();
    result
}

/// Every touched substitution must remain connected to a representable parent macro.
pub(super) fn validate_candidate(file: &XcStringsFile, work: &Work<'_>) -> Result<(), String> {
    let root = file
        .strings
        .get(&work.destination.key)
        .and_then(|e| e.localizations.as_ref())
        .and_then(|l| l.get(&work.destination.locale))
        .ok_or("missing target localization")?;
    crate::service::assessment::validate_substitution_references(root)
}
