use crate::{
    error::XcStringsError,
    model::workflow::{SourceSnapshot, WorkflowDocument},
};
use serde_json::Value;
use std::collections::HashSet;

pub fn parse_document(json: &str) -> Result<WorkflowDocument, XcStringsError> {
    let unique = crate::service::parser::parse_unique_json(json)?;
    let document: WorkflowDocument = serde_json::from_value(unique)?;
    validate(&document)?;
    Ok(document)
}
pub fn format_document(document: &WorkflowDocument) -> Result<String, XcStringsError> {
    validate(document)?;
    Ok(format!("{}\n", serde_json::to_string_pretty(document)?))
}
pub(super) fn validate(document: &WorkflowDocument) -> Result<(), XcStringsError> {
    if document.version != 1 {
        return Err(super::invalid(
            "unsupported_workflow_version",
            document.version,
        ));
    }
    if let Some(baseline) = &document.source_baseline {
        for (key, snapshot) in &baseline.sources {
            validate_snapshot(key, snapshot)?;
        }
    }
    let mut destinations = HashSet::new();
    for record in &document.pending_review {
        if !destinations.insert(&record.destination) {
            return Err(super::invalid(
                "duplicate_pending_review",
                "duplicate destination",
            ));
        }
        if let Some(old) = &record.old_source {
            validate_snapshot(&record.destination.key, old)?;
        }
        validate_snapshot(&record.destination.key, &record.new_source)?;
        if !record
            .target_content_version
            .strip_prefix("target-content-v1:sha256:")
            .is_some_and(|digest| {
                digest.len() == 64
                    && digest
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
        {
            return Err(super::invalid(
                "invalid_pending_review",
                "invalid target content fingerprint",
            ));
        }
    }
    Ok(())
}
fn validate_snapshot(key: &str, snapshot: &SourceSnapshot) -> Result<(), XcStringsError> {
    let value = &snapshot.0;
    let valid = value.as_object().is_some()
        && value.get("schema").and_then(Value::as_u64) == Some(1)
        && value.get("key").and_then(Value::as_str) == Some(key)
        && value
            .get("source_language")
            .and_then(Value::as_str)
            .is_some_and(|language| !language.is_empty())
        && value.get("should_translate").is_some_and(Value::is_boolean)
        && value
            .get("comment")
            .is_some_and(|v| v.is_null() || v.is_string())
        && value
            .get("source")
            .is_some_and(|v| v.is_null() || v.is_object())
        && value
            .get("context")
            .is_some_and(|v| v.is_null() || v.is_object())
        && value.get("root_extra").is_some_and(Value::is_object)
        && value.get("entry_extra").is_some_and(Value::is_object);
    if !valid {
        return Err(super::invalid(
            "invalid_source_snapshot",
            format!("malformed snapshot for key {key:?}"),
        ));
    }
    if let Some(source) = value.get("source").filter(|source| !source.is_null()) {
        // Snapshot intentionally omits known editorial states. Restore inert
        // states on a temporary value before checking the native typed schema.
        let mut native = source.clone();
        restore_state(&mut native);
        serde_json::from_value::<crate::model::xcstrings::Localization>(native)
            .map_err(|error| super::invalid("invalid_source_snapshot", error))?;
    }
    if let Some(context) = value.get("context").filter(|context| !context.is_null()) {
        serde_json::from_value::<crate::model::context::AuthoredContext>(context.clone())
            .map_err(|error| super::invalid("invalid_source_snapshot", error))?;
    }
    Ok(())
}

fn restore_state(node: &mut Value) {
    if let Some(unit) = node.get_mut("stringUnit").and_then(Value::as_object_mut) {
        unit.insert("state".into(), Value::String("new".into()));
    }
    if let Some(variations) = node.get_mut("variations") {
        restore_variations(variations);
    }
    if let Some(substitutions) = node.get_mut("substitutions").and_then(Value::as_object_mut) {
        for sub in substitutions.values_mut() {
            if let Some(variations) = sub.get_mut("variations") {
                restore_variations(variations);
            }
        }
    }
}
fn restore_variations(variations: &mut Value) {
    for axis in ["device", "plural"] {
        if let Some(branches) = variations.get_mut(axis).and_then(Value::as_object_mut) {
            for branch in branches.values_mut() {
                restore_state(branch);
            }
        }
    }
}
