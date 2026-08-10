mod engine;
mod report;
mod validation;

use std::collections::HashMap;

use serde_json::Value;

use crate::error::XcStringsError;
use crate::service::{formatter, parser};

pub use report::{
    AutoApplied, ConflictChoice, ConflictResolution, ConflictValue, ExpectedFingerprints,
    FileFingerprint, MergeConflict, MergeFingerprints, MergeOptions, MergeReport,
    MergeValidationIssue, PreparedMerge,
};

use engine::{MergeContext, NodeKind, merge_root};
use validation::validation_delta;

/// Return a SHA-256 fingerprint of the exact bytes, including a possible BOM.
pub fn fingerprint(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(bytes);
    let mut result = String::with_capacity(71);
    result.push_str("sha256:");
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        result.push(char::from(HEX[usize::from(byte >> 4)]));
        result.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    result
}

/// Perform a deterministic semantic three-way merge without touching the filesystem.
pub fn prepare_merge(
    base_bytes: &[u8],
    current_bytes: &[u8],
    incoming_bytes: &[u8],
    options: &MergeOptions,
) -> Result<PreparedMerge, XcStringsError> {
    if options.conflict_limit == 0 || options.conflict_limit > 500 {
        return Err(XcStringsError::InvalidFormat(
            "conflict_limit must be between 1 and 500".into(),
        ));
    }

    let base = parse_raw(base_bytes, "base")?;
    let current = parse_raw(current_bytes, "current")?;
    let incoming = parse_raw(incoming_bytes, "incoming")?;
    validate_source_languages(&base, &current, &incoming)?;

    // Typed parsing is an intentional second gate: raw JSON preserves future fields,
    // while the known schema still has to be a valid String Catalog.
    let current_typed = parser::parse(strip_bom_text(current_bytes, "current")?)?;
    let (resolution_map, resolution_order) = resolution_map(&options.resolutions)?;
    let mut context = MergeContext::new(resolution_map, resolution_order);
    let merged = merge_root(&base, &current, &incoming, &mut context)?;
    context.reject_unused_resolutions()?;

    let content = format_raw(&merged)?;
    let result_typed = parser::parse(&content)?;
    let (existing_validation_issues, introduced_validation_issues) =
        validation_delta(&current_typed, &result_typed);
    let unresolved = context
        .conflicts
        .iter()
        .filter(|conflict| !conflict.resolved)
        .cloned()
        .collect::<Vec<_>>();
    let conflict_total = context.conflicts.len();
    let unresolved_conflict_total = unresolved.len();
    let conflicts = unresolved
        .into_iter()
        .skip(options.conflict_offset)
        .take(options.conflict_limit)
        .collect::<Vec<_>>();
    let has_more =
        options.conflict_offset.saturating_add(conflicts.len()) < unresolved_conflict_total;

    let report = MergeReport {
        dry_run: true,
        written: false,
        output_path: None,
        fingerprints: MergeFingerprints {
            base: file_fingerprint(base_bytes, &base),
            current: file_fingerprint(current_bytes, &current),
            incoming: file_fingerprint(incoming_bytes, &incoming),
            output_before: None,
            result: FileFingerprint {
                sha256: fingerprint(content.as_bytes()),
                key_count: key_count(&merged),
            },
        },
        expected_fingerprints: None,
        auto_applied: context.auto_applied,
        resolutions_applied: context.resolutions_applied,
        conflict_total,
        unresolved_conflict_total,
        conflict_offset: options.conflict_offset,
        conflict_limit: options.conflict_limit,
        has_more,
        conflicts,
        existing_validation_issues,
        introduced_validation_issues,
    };

    Ok(PreparedMerge {
        report,
        content,
        parsed: result_typed,
    })
}

pub(crate) fn parse_raw(bytes: &[u8], label: &str) -> Result<Value, XcStringsError> {
    let text = strip_bom_text(bytes, label)?;
    serde_json::from_str(text)
        .map_err(|error| XcStringsError::JsonParse(format!("{label}: {error}")))
}

fn strip_bom_text<'a>(bytes: &'a [u8], label: &str) -> Result<&'a str, XcStringsError> {
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    std::str::from_utf8(bytes)
        .map_err(|error| XcStringsError::InvalidFormat(format!("{label} is not UTF-8: {error}")))
}

fn validate_source_languages(
    base: &Value,
    current: &Value,
    incoming: &Value,
) -> Result<(), XcStringsError> {
    let values = [
        source_language(base),
        source_language(current),
        source_language(incoming),
    ];
    if values[0].is_none() || values[0] != values[1] || values[0] != values[2] {
        return Err(XcStringsError::InvalidFormat(
            "sourceLanguage must exist and match in base, current, and incoming".into(),
        ));
    }
    Ok(())
}

fn source_language(value: &Value) -> Option<&str> {
    value.get("sourceLanguage").and_then(Value::as_str)
}

fn resolution_map(
    resolutions: &[ConflictResolution],
) -> Result<(HashMap<String, ConflictChoice>, Vec<String>), XcStringsError> {
    let mut map = HashMap::with_capacity(resolutions.len());
    let mut order = Vec::with_capacity(resolutions.len());
    for resolution in resolutions {
        if map
            .insert(resolution.conflict_id.clone(), resolution.choice)
            .is_some()
        {
            return Err(XcStringsError::InvalidFormat(format!(
                "duplicate resolution for conflict {}",
                resolution.conflict_id
            )));
        }
        order.push(resolution.conflict_id.clone());
    }
    Ok((map, order))
}

pub(crate) fn file_fingerprint(bytes: &[u8], value: &Value) -> FileFingerprint {
    FileFingerprint {
        sha256: fingerprint(bytes),
        key_count: key_count(value),
    }
}

fn key_count(value: &Value) -> usize {
    value
        .get("strings")
        .and_then(Value::as_object)
        .map_or(0, serde_json::Map::len)
}

fn format_raw(value: &Value) -> Result<String, XcStringsError> {
    let json = serde_json::to_string_pretty(value)?;
    let mut formatted = formatter::fixup_colon_spacing(&json);
    formatted.push('\n');
    Ok(formatted)
}

#[allow(dead_code)]
const _: NodeKind = NodeKind::Root;
