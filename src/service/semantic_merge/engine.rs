use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use crate::error::XcStringsError;

use super::{AutoApplied, ConflictChoice, ConflictValue, MergeConflict, fingerprint};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NodeKind {
    Root,
    Strings,
    StringEntry,
    Localizations,
    Localization,
    Variations,
    Plural,
    PluralEntry,
    Device,
    DeviceEntry,
    Substitutions,
    Substitution,
    SubstitutionVariations,
    SubstitutionPlural,
    SubstitutionPluralEntry,
    Scalar,
    Atomic,
}

pub(super) struct MergeContext {
    resolutions: HashMap<String, ConflictChoice>,
    resolution_order: Vec<String>,
    used_resolutions: HashSet<String>,
    pub(super) conflicts: Vec<MergeConflict>,
    pub(super) auto_applied: AutoApplied,
    pub(super) resolutions_applied: usize,
}

impl MergeContext {
    pub(super) fn new(
        resolutions: HashMap<String, ConflictChoice>,
        resolution_order: Vec<String>,
    ) -> Self {
        Self {
            resolutions,
            resolution_order,
            used_resolutions: HashSet::new(),
            conflicts: Vec::new(),
            auto_applied: AutoApplied::default(),
            resolutions_applied: 0,
        }
    }

    pub(super) fn reject_unused_resolutions(&self) -> Result<(), XcStringsError> {
        if let Some(id) = self
            .resolution_order
            .iter()
            .find(|id| !self.used_resolutions.contains(*id))
        {
            return Err(XcStringsError::InvalidFormat(format!(
                "resolution references unknown conflict {id}"
            )));
        }
        Ok(())
    }
}

pub(super) fn merge_root(
    base: &Value,
    current: &Value,
    incoming: &Value,
    context: &mut MergeContext,
) -> Result<Value, XcStringsError> {
    let Some(base_map) = base.as_object() else {
        return Err(invalid_object("base catalog"));
    };
    let Some(current_map) = current.as_object() else {
        return Err(invalid_object("current catalog"));
    };
    let Some(incoming_map) = incoming.as_object() else {
        return Err(invalid_object("incoming catalog"));
    };
    merge_object(
        base_map,
        current_map,
        incoming_map,
        NodeKind::Root,
        "",
        context,
    )
}

fn merge_object(
    base: &Map<String, Value>,
    current: &Map<String, Value>,
    incoming: &Map<String, Value>,
    kind: NodeKind,
    pointer: &str,
    context: &mut MergeContext,
) -> Result<Value, XcStringsError> {
    let mut result = Map::with_capacity(current.len().max(incoming.len()));
    for key in current
        .keys()
        .chain(incoming.keys().filter(|key| !current.contains_key(*key)))
    {
        let child_pointer = join_pointer(pointer, key);
        let child_kind = child_kind(kind, key);
        if let Some(value) = merge_node(
            base.get(key),
            current.get(key),
            incoming.get(key),
            child_kind,
            &child_pointer,
            context,
        )? {
            result.insert(key.clone(), value);
        }
    }
    Ok(Value::Object(result))
}

fn merge_node(
    base: Option<&Value>,
    current: Option<&Value>,
    incoming: Option<&Value>,
    kind: NodeKind,
    pointer: &str,
    context: &mut MergeContext,
) -> Result<Option<Value>, XcStringsError> {
    if current == incoming {
        return Ok(current.cloned());
    }
    if current == base {
        context.auto_applied.incoming += 1;
        return Ok(incoming.cloned());
    }
    if incoming == base {
        context.auto_applied.current += 1;
        return Ok(current.cloned());
    }

    if base.is_some()
        && current.is_some()
        && incoming.is_some()
        && kind != NodeKind::Atomic
        && let (Some(base), Some(current), Some(incoming)) = (
            base.and_then(Value::as_object),
            current.and_then(Value::as_object),
            incoming.and_then(Value::as_object),
        )
    {
        return merge_object(base, current, incoming, kind, pointer, context).map(Some);
    }

    Ok(resolve_conflict(
        base, current, incoming, kind, pointer, context,
    ))
}

fn resolve_conflict(
    base: Option<&Value>,
    current: Option<&Value>,
    incoming: Option<&Value>,
    kind: NodeKind,
    pointer: &str,
    context: &mut MergeContext,
) -> Option<Value> {
    let conflict_kind = conflict_kind(base, current, incoming, kind);
    let id = conflict_id(pointer, conflict_kind);
    let resolution = context.resolutions.get(&id).copied();
    let resolved = resolution.is_some();
    if resolved {
        context.used_resolutions.insert(id.clone());
        context.resolutions_applied += 1;
    }
    let (key, locale, field) = conflict_metadata(pointer);
    context.conflicts.push(MergeConflict {
        id,
        pointer: pointer.to_string(),
        key,
        locale,
        field,
        kind: conflict_kind.to_string(),
        base: conflict_value(base),
        current: conflict_value(current),
        incoming: conflict_value(incoming),
        resolved,
    });
    match resolution.unwrap_or(ConflictChoice::Base) {
        ConflictChoice::Current => current.cloned(),
        ConflictChoice::Incoming => incoming.cloned(),
        ConflictChoice::Base => base.cloned(),
    }
}

fn child_kind(parent: NodeKind, field: &str) -> NodeKind {
    match parent {
        NodeKind::Root if field == "strings" => NodeKind::Strings,
        NodeKind::Root if field == "version" => NodeKind::Scalar,
        NodeKind::Strings => NodeKind::StringEntry,
        NodeKind::StringEntry if field == "localizations" => NodeKind::Localizations,
        NodeKind::Localizations => NodeKind::Localization,
        NodeKind::Localization if field == "variations" => NodeKind::Variations,
        NodeKind::Localization if field == "substitutions" => NodeKind::Substitutions,
        NodeKind::Localization if field == "stringUnit" => NodeKind::Atomic,
        NodeKind::Variations if field == "plural" => NodeKind::Plural,
        NodeKind::Variations if field == "device" => NodeKind::Device,
        NodeKind::Plural => NodeKind::PluralEntry,
        NodeKind::PluralEntry if field == "stringUnit" => NodeKind::Atomic,
        NodeKind::Device => NodeKind::DeviceEntry,
        NodeKind::DeviceEntry if field == "stringUnit" => NodeKind::Atomic,
        NodeKind::Substitutions => NodeKind::Substitution,
        NodeKind::Substitution if field == "variations" => NodeKind::SubstitutionVariations,
        NodeKind::SubstitutionVariations if field == "plural" => NodeKind::SubstitutionPlural,
        NodeKind::SubstitutionPlural => NodeKind::SubstitutionPluralEntry,
        NodeKind::SubstitutionPluralEntry if field == "stringUnit" => NodeKind::Atomic,
        _ => NodeKind::Atomic,
    }
}

fn conflict_kind(
    base: Option<&Value>,
    current: Option<&Value>,
    incoming: Option<&Value>,
    kind: NodeKind,
) -> &'static str {
    match (base, current, incoming) {
        (None, Some(_), Some(_)) => "divergent_add",
        (Some(_), None, Some(_)) | (Some(_), Some(_), None) => "delete_modify",
        _ if kind == NodeKind::Atomic => "atomic_divergence",
        _ => "scalar_divergence",
    }
}

fn conflict_id(pointer: &str, kind: &str) -> String {
    fingerprint(format!("merge-conflict-v1\0{pointer}\0{kind}").as_bytes()).replacen(
        "sha256:",
        "merge-v1:",
        1,
    )
}

fn conflict_value(value: Option<&Value>) -> ConflictValue {
    let Some(value) = value else {
        return ConflictValue {
            present: false,
            preview: "<absent>".into(),
            fingerprint: None,
        };
    };
    let compact = serde_json::to_string(value).unwrap_or_else(|_| "<unserializable>".into());
    let mut preview = compact.chars().take(200).collect::<String>();
    if compact.chars().count() > 200 {
        preview.push_str("...");
    }
    ConflictValue {
        present: true,
        preview,
        fingerprint: Some(fingerprint(compact.as_bytes())),
    }
}

fn join_pointer(parent: &str, segment: &str) -> String {
    format!("{parent}/{}", escape_pointer(segment))
}

fn escape_pointer(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

fn unescape_pointer(value: &str) -> String {
    value.replace("~1", "/").replace("~0", "~")
}

fn conflict_metadata(pointer: &str) -> (Option<String>, Option<String>, Option<String>) {
    let segments = pointer
        .split('/')
        .skip(1)
        .map(unescape_pointer)
        .collect::<Vec<_>>();
    let key = (segments.first().map(String::as_str) == Some("strings"))
        .then(|| segments.get(1).cloned())
        .flatten();
    let locale = (segments.get(2).map(String::as_str) == Some("localizations"))
        .then(|| segments.get(3).cloned())
        .flatten();
    let field = segments.last().cloned();
    (key, locale, field)
}

fn invalid_object(label: &str) -> XcStringsError {
    XcStringsError::InvalidFormat(format!("{label} root must be a JSON object"))
}
