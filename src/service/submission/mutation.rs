use crate::model::translation::TranslationDestination;
use crate::model::xcstrings::{
    Localization, Substitution, TranslationState, Variations, XcStringsFile, paths::LeafStep,
};
use crate::service::assessment;

/// Stage one request on a cloned entry; a failure never partially mutates the file.
pub(crate) fn apply(
    file: &mut XcStringsFile,
    leaves: &[(TranslationDestination, String)],
) -> Result<(), String> {
    if let Some((key, candidate)) = prepare_entry(file, leaves)? {
        file.strings.insert(key, candidate);
    }
    Ok(())
}

pub(crate) fn validate(
    file: &XcStringsFile,
    leaves: &[(TranslationDestination, String)],
) -> Result<(), String> {
    prepare_entry(file, leaves).map(|_| ())
}

fn prepare_entry(
    file: &XcStringsFile,
    leaves: &[(TranslationDestination, String)],
) -> Result<Option<(String, crate::model::xcstrings::StringEntry)>, String> {
    let Some((first, _)) = leaves.first() else {
        return Ok(None);
    };
    let entry = file.strings.get(&first.key).ok_or("key not found")?;
    // Clone just the affected entry for atomic per-request validation and mutation.
    let mut candidate = entry.clone();
    let source = candidate
        .localizations
        .as_ref()
        .and_then(|locs| locs.get(&file.source_language))
        .cloned();
    let locs = candidate.localizations.get_or_insert_with(Default::default);
    if !locs.contains_key(&first.locale) {
        let needs_parent = source
            .as_ref()
            .is_some_and(|node| node.string_unit.is_some() && node.substitutions.is_some())
            && leaves.iter().any(|(dest, _)| {
                dest.path.is_empty() || matches!(dest.path.first(), Some(LeafStep::Substitution(_)))
            });
        let needs_shape = !first.path.is_empty()
            && source
                .as_ref()
                .is_some_and(|node| node.variations.is_some());
        let node = if needs_parent || needs_shape {
            assessment::initialize_locale(source.as_ref(), &first.locale)
                .map_err(|e| e.to_string())?
        } else {
            Localization::default()
        };
        locs.insert(first.locale.clone(), node);
    }
    let target = locs
        .get_mut(&first.locale)
        .ok_or("target locale disappeared")?;
    for (destination, value) in leaves {
        let node = ensure_node(target, source.as_ref(), &destination.path)?;
        if node.variations.is_some() {
            return Err("simple target write would flatten an existing variation tree".into());
        }
        node.set_translation(TranslationState::Translated, value);
    }
    assessment::validate_substitution_references(target)?;
    Ok(Some((first.key.clone(), candidate)))
}

fn ensure_node<'a>(
    node: &'a mut Localization,
    source: Option<&Localization>,
    path: &[LeafStep],
) -> Result<&'a mut Localization, String> {
    let Some((step, rest)) = path.split_first() else {
        return Ok(node);
    };
    if let LeafStep::Substitution(name) = step {
        let source_sub = source
            .and_then(|source| source.substitutions.as_ref())
            .and_then(|subs| subs.get(name));
        let subs = node.substitutions.get_or_insert_with(Default::default);
        if !subs.contains_key(name) {
            let mut template = source_sub
                .ok_or_else(|| format!("missing substitution metadata for '{name}'"))?
                .clone();
            if let Some(variations) = &mut template.variations {
                // Keep physical metadata while initializing only submitted categories.
                if let Some(plural) = &mut variations.plural {
                    plural.clear();
                }
                if let Some(device) = &mut variations.device {
                    device.clear();
                }
            }
            subs.insert(name.clone(), template);
        }
        let sub = subs.get_mut(name).ok_or("substitution disappeared")?;
        validate_metadata(sub)?;
        return ensure_variations(
            sub.variations.get_or_insert_with(Default::default),
            source_sub.and_then(|s| s.variations.as_ref()),
            rest,
        );
    }
    if let Some(unit) = &node.string_unit {
        if node.substitutions.is_some() {
            return Err("variation write conflicts with existing substitution parent".into());
        }
        if !unit.extra.is_empty() || !unit.value.is_empty() || unit.state != TranslationState::New {
            return Err("variation write would replace an existing simple translation".into());
        }
        node.string_unit = None;
        node.layout.forget("stringUnit");
    }
    ensure_variations(
        node.variations.get_or_insert_with(Default::default),
        source.and_then(|s| s.variations.as_ref()),
        path,
    )
}

fn ensure_variations<'a>(
    variations: &'a mut Variations,
    source: Option<&Variations>,
    path: &[LeafStep],
) -> Result<&'a mut Localization, String> {
    let Some((step, rest)) = path.split_first() else {
        return Err("incomplete variation path".into());
    };
    let (node, template) = match step {
        LeafStep::Plural(category) => {
            if variations.device.is_some() {
                return Err("plural path conflicts with a device axis".into());
            }
            let template = source
                .and_then(|v| v.plural.as_ref())
                .and_then(|p| p.get(category).or_else(|| p.get("other")));
            let node = variations
                .plural
                .get_or_insert_with(Default::default)
                .entry(category.clone())
                .or_insert_with(|| leaf_template(template));
            (node, template)
        }
        LeafStep::Device(category) => {
            if variations.plural.is_some() {
                return Err("device path conflicts with a plural axis".into());
            }
            let template = source
                .and_then(|v| v.device.as_ref())
                .and_then(|p| p.get(category));
            let node = variations
                .device
                .get_or_insert_with(Default::default)
                .entry(category.clone())
                .or_default();
            (node, template)
        }
        LeafStep::Substitution(_) => {
            return Err("substitution cannot directly follow another substitution".into());
        }
    };
    ensure_node(node, template, rest)
}

fn leaf_template(source: Option<&Localization>) -> Localization {
    // Matching physical leaf metadata belongs to the newly translated leaf.
    let mut node = source.cloned().unwrap_or_default();
    if let Some(unit) = &mut node.string_unit {
        unit.set_translation(TranslationState::New, "");
    }
    node
}

pub(crate) fn validate_metadata(sub: &Substitution) -> Result<(), String> {
    sub.validate_metadata().map(|_| ())
}
