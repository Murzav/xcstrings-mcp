use crate::model::xcstrings::{
    DeviceCategory, XcStringsFile,
    paths::{LeafPath, LeafStep},
};

pub(super) fn encode(key: &str, path: &[LeafStep]) -> Result<String, String> {
    validate_path(path)?;
    if path.is_empty() {
        return Ok(key.into());
    }
    let mut parts = Vec::with_capacity(path.len());
    for step in path {
        parts.push(match step {
            LeafStep::Device(value) => format!("device.{}", device_name(value)?),
            LeafStep::Plural(value) => format!("plural.{value}"),
            LeafStep::Substitution(name) => {
                valid_name(name)?;
                if name.contains('.') {
                    format!("substitutions.@{name}@")
                } else {
                    format!("substitutions.{name}")
                }
            }
        });
    }
    Ok(format!("{key}|==|{}", parts.join(".")))
}

/// Xcode interprets a recognized variation suffix even when it is a literal catalog key.
pub(super) fn validate_export_literal(key: &str) -> Result<(), String> {
    if key
        .match_indices("|==|")
        .any(|(index, _)| decode_path(&key[index + 4..]).is_ok())
    {
        return Err(format!(
            "literal key '{key}' ends in an Apple variation path; Xcode cannot import its exported updates safely"
        ));
    }
    Ok(())
}

pub(super) fn is_plural_unit_id(id: &str) -> bool {
    id.match_indices("|==|").any(|(index, _)| {
        decode_path(&id[index + 4..])
            .is_ok_and(|path| matches!(path.last(), Some(LeafStep::Plural(_))))
    })
}

/// Every catalog key is a possible boundary; exact literal IDs have no priority.
pub(super) fn resolve(
    file: &XcStringsFile,
    id: &str,
) -> Result<(String, LeafPath), (&'static str, String)> {
    let mut candidates = Vec::new();
    let mut invalid = None;
    for key in file.strings.keys() {
        if key == id {
            candidates.push((key.clone(), Vec::new()));
        }
        if let Some(suffix) = id.strip_prefix(key).and_then(|v| v.strip_prefix("|==|")) {
            match decode_path(suffix) {
                Ok(path) => candidates.push((key.clone(), path)),
                Err(message) => invalid = Some(message),
            }
        }
    }
    match candidates.len() {
        1 => Ok(candidates.remove(0)),
        0 => Err((
            if invalid.is_some() {
                "unsupported_destination"
            } else {
                "unknown_key"
            },
            invalid.unwrap_or_else(|| format!("no catalog destination for '{id}'")),
        )),
        _ => Err((
            "ambiguous_destination",
            format!("unit '{id}' matches both literal and varied catalog destinations"),
        )),
    }
}

fn decode_path(mut rest: &str) -> Result<LeafPath, String> {
    if rest.is_empty() || rest.ends_with('.') {
        return Err("empty Apple variation path segment".into());
    }
    let mut path = Vec::new();
    while !rest.is_empty() {
        if let Some(value) = rest.strip_prefix("device.") {
            let (name, tail) = segment(value);
            let device = match name {
                "iphone" => DeviceCategory::IPhone,
                "ipad" => DeviceCategory::IPad,
                "mac" => DeviceCategory::Mac,
                "applewatch" => DeviceCategory::AppleWatch,
                "appletv" => DeviceCategory::AppleTv,
                "applevision" => DeviceCategory::AppleVision,
                "other" => DeviceCategory::Other,
                _ => return Err(format!("unknown Apple device '{name}'")),
            };
            path.push(LeafStep::Device(device));
            rest = tail;
        } else if let Some(value) = rest.strip_prefix("plural.") {
            let (name, tail) = segment(value);
            path.push(LeafStep::Plural(name.into()));
            rest = tail;
        } else if let Some(value) = rest.strip_prefix("substitutions.") {
            let (name, tail) = if let Some(quoted) = value.strip_prefix('@') {
                let end = quoted
                    .find('@')
                    .ok_or("unterminated quoted substitution name")?;
                let tail = quoted[end + 1..]
                    .strip_prefix('.')
                    .ok_or("substitution requires a plural path")?;
                (&quoted[..end], tail)
            } else {
                segment(value)
            };
            valid_name(name)?;
            path.push(LeafStep::Substitution(name.into()));
            rest = tail;
        } else {
            return Err(format!("unknown Apple variation path '{rest}'"));
        }
    }
    validate_path(&path)?;
    Ok(path)
}
fn segment(value: &str) -> (&str, &str) {
    value.split_once('.').unwrap_or((value, ""))
}
fn valid_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.contains('@') || name.contains("|==|") {
        return Err(format!(
            "substitution name '{name}' cannot roundtrip safely through Apple XLIFF"
        ));
    }
    Ok(())
}
pub(super) fn validate_path(path: &[LeafStep]) -> Result<(), String> {
    crate::model::xcstrings::paths::validate_supported_path(path)?;
    for step in path {
        if let LeafStep::Substitution(name) = step {
            valid_name(name)?;
        }
    }
    Ok(())
}
pub(super) fn device_name(value: &DeviceCategory) -> Result<&str, String> {
    match value {
        DeviceCategory::IPhone => Ok("iphone"),
        DeviceCategory::IPad => Ok("ipad"),
        DeviceCategory::Mac => Ok("mac"),
        DeviceCategory::AppleWatch => Ok("applewatch"),
        DeviceCategory::AppleTv => Ok("appletv"),
        DeviceCategory::AppleVision => Ok("applevision"),
        DeviceCategory::Other => Ok("other"),
        DeviceCategory::Unknown(name) => Err(format!("unknown Apple device '{name}'")),
    }
}
