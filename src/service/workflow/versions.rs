use crate::{
    error::XcStringsError,
    model::{
        translation::TranslationDestination,
        workflow::{SourceSnapshot, WorkflowDocument},
        xcstrings::{
            Localization, Variations, XcStringsFile,
            paths::{LeafStep, find_leaf},
        },
    },
};
use serde_json::{Value, json};

pub(super) fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut keys: Vec<_> = object.keys().collect();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|key| (key.clone(), canonical(&object[key])))
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        _ => value.clone(),
    }
}
pub(super) fn hash(domain: &str, value: &Value) -> String {
    let bytes = format!("{domain}\0{}", canonical(value));
    format!(
        "{domain}:{}",
        crate::service::semantic_merge::fingerprint(bytes.as_bytes())
    )
}

/// Portable whole-key translation input, excluding known editorial state only.
pub fn source_snapshot(
    file: &XcStringsFile,
    document: &WorkflowDocument,
    key: &str,
) -> Result<SourceSnapshot, XcStringsError> {
    let entry = file
        .strings
        .get(key)
        .ok_or_else(|| XcStringsError::KeyNotFound(key.into()))?;
    let mut source = entry
        .localizations
        .as_ref()
        .and_then(|locales| locales.get(&file.source_language))
        .map(serde_json::to_value)
        .transpose()?
        .unwrap_or(Value::Null);
    strip_state(&mut source);
    Ok(SourceSnapshot(canonical(&json!({
        "schema":1,"source_language":file.source_language,"key":key,
        "should_translate":entry.should_translate,"comment":entry.comment,
        "source":source,"root_extra":file.extra,"entry_extra":entry.extra,
        "context":crate::service::context::authored_snapshot(&document.contexts,key)
    }))))
}
fn strip_state(node: &mut Value) {
    if let Some(unit) = node.get_mut("stringUnit").and_then(Value::as_object_mut) {
        unit.remove("state");
    }
    if let Some(variations) = node.get_mut("variations") {
        strip_variations(variations);
    }
    if let Some(substitutions) = node.get_mut("substitutions").and_then(Value::as_object_mut) {
        for sub in substitutions.values_mut() {
            if let Some(variations) = sub.get_mut("variations") {
                strip_variations(variations);
            }
        }
    }
}
fn strip_variations(variations: &mut Value) {
    for axis in ["device", "plural"] {
        if let Some(branches) = variations.get_mut(axis).and_then(Value::as_object_mut) {
            for branch in branches.values_mut() {
                strip_state(branch);
            }
        }
    }
}
/// Bind a portable input snapshot to the caller's canonical catalog identity.
pub fn source_version(catalog_identity: &str, key: &str, snapshot: &SourceSnapshot) -> String {
    hash("source-v1", &json!([catalog_identity, key, snapshot]))
}
fn target_payload(
    file: &XcStringsFile,
    destination: &TranslationDestination,
    include_state: bool,
) -> Result<Option<Value>, XcStringsError> {
    let entry = file
        .strings
        .get(&destination.key)
        .ok_or_else(|| XcStringsError::KeyNotFound(destination.key.clone()))?;
    let Some(root) = entry
        .localizations
        .as_ref()
        .and_then(|locales| locales.get(&destination.locale))
    else {
        return Ok(None);
    };
    let Some(unit) = find_leaf(root, &destination.path) else {
        return Ok(None);
    };
    let mut payload = serde_json::to_value(unit)?;
    if !include_state && let Some(object) = payload.as_object_mut() {
        object.remove("state");
    }
    let metadata=root.substitutions.as_ref().map(|subs| {
        subs.iter().map(|(name,sub)| (name.clone(),json!({"arg_num":sub.arg_num,"format_specifier":sub.format_specifier,"extra":sub.extra}))).collect::<serde_json::Map<_,_>>()
    });
    Ok(Some(
        json!({"destination":destination,"unit":payload,"lineage":lineage(root,&destination.path,include_state),"substitutions":metadata}),
    ))
}
pub fn target_version(
    catalog_identity: &str,
    file: &XcStringsFile,
    destination: &TranslationDestination,
) -> Result<Option<String>, XcStringsError> {
    super::identity(catalog_identity)?;
    Ok(target_payload(file, destination, true)?
        .map(|payload| hash("target-v1", &json!([catalog_identity, payload]))))
}
pub(super) fn content_version(
    file: &XcStringsFile,
    destination: &TranslationDestination,
) -> Result<Option<String>, XcStringsError> {
    Ok(
        target_payload(file, destination, false)?
            .map(|payload| hash("target-content-v1", &payload)),
    )
}

// Record only the addressed ancestors and structural branch identities. Other
// leaves' text/state do not invalidate an approval for this physical leaf.
fn lineage(root: &Localization, path: &[LeafStep], include_shape: bool) -> Vec<Value> {
    let mut result = Vec::new();
    localization_lineage(root, path, include_shape, &mut result);
    result
}
fn localization_lineage(
    node: &Localization,
    path: &[LeafStep],
    include_shape: bool,
    result: &mut Vec<Value>,
) {
    result.push(if include_shape {json!({"extra":node.extra,"string_unit_present":node.string_unit.is_some(),
        "variations_present":node.variations.is_some(),"substitutions_present":node.substitutions.is_some()})}else{json!({"extra":node.extra})});
    match path.split_first() {
        Some((LeafStep::Substitution(name), rest)) => {
            if let Some(sub) = node.substitutions.as_ref().and_then(|subs| subs.get(name)) {
                result.push(json!({"arg_num":sub.arg_num,"format_specifier":sub.format_specifier,"extra":sub.extra}));
                if let Some(variations) = &sub.variations {
                    variation_lineage(variations, rest, include_shape, result);
                }
            }
        }
        Some(_) => {
            if let Some(variations) = &node.variations {
                variation_lineage(variations, path, include_shape, result);
            }
        }
        None => {}
    }
}
fn variation_lineage(
    variations: &Variations,
    path: &[LeafStep],
    include_shape: bool,
    result: &mut Vec<Value>,
) {
    let mut plural = variations
        .plural
        .as_ref()
        .map(|branches| branches.keys().collect::<Vec<_>>());
    let mut device = variations
        .device
        .as_ref()
        .map(|branches| branches.keys().collect::<Vec<_>>());
    if let Some(keys) = &mut plural {
        keys.sort();
    }
    if let Some(keys) = &mut device {
        keys.sort();
    }
    result.push(if include_shape {
        json!({"extra":variations.extra,"plural":plural,"device":device})
    } else {
        json!({"extra":variations.extra})
    });
    let Some((step, rest)) = path.split_first() else {
        return;
    };
    let child = match step {
        LeafStep::Plural(category) => variations
            .plural
            .as_ref()
            .and_then(|branches| branches.get(category)),
        LeafStep::Device(category) => variations
            .device
            .as_ref()
            .and_then(|branches| branches.get(category)),
        LeafStep::Substitution(_) => None,
    };
    if let Some(child) = child {
        localization_lineage(child, rest, include_shape, result);
    }
}
