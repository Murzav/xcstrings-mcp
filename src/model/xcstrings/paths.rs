use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{DeviceCategory, Localization, StringUnit, Substitution, Variations};

/// An empty path identifies the localization's root stringUnit.
pub type LeafPath = Vec<LeafStep>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LeafStep {
    Device(DeviceCategory),
    Plural(String),
    Substitution(String),
}

#[derive(Debug, Clone, Copy)]
pub struct SubstitutionContext<'a> {
    pub name: &'a str,
    pub substitution: &'a Substitution,
}

#[derive(Debug)]
pub struct CatalogLeaf<'a> {
    pub path: LeafPath,
    pub unit: &'a StringUnit,
    pub substitutions: Vec<SubstitutionContext<'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LeafDiagnosticCode {
    UnknownAxis,
    UnknownDevice,
    UnknownPlural,
    EmptyAxis,
    EmptyLocalization,
    MissingSubstitutionMetadata,
    UnknownLocale,
    InvalidShape,
    InvalidSubstitutionMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LeafDiagnostic {
    pub path: LeafPath,
    pub code: LeafDiagnosticCode,
    pub detail: String,
}

#[derive(Debug, Default)]
pub struct LeafTraversal<'a> {
    pub leaves: Vec<CatalogLeaf<'a>>,
    pub diagnostics: Vec<LeafDiagnostic>,
}

/// Visit every known semantic leaf; unsupported shapes are never silently lost.
pub fn collect_leaves(localization: &Localization) -> LeafTraversal<'_> {
    let mut result = LeafTraversal::default();
    visit_localization(localization, &mut Vec::new(), &mut Vec::new(), &mut result);
    result
}

fn diagnostic(
    result: &mut LeafTraversal<'_>,
    path: &[LeafStep],
    code: LeafDiagnosticCode,
    detail: impl Into<String>,
) {
    result.diagnostics.push(LeafDiagnostic {
        path: path.to_vec(),
        code,
        detail: detail.into(),
    });
}

fn visit_localization<'a>(
    node: &'a Localization,
    path: &mut LeafPath,
    substitutions: &mut Vec<SubstitutionContext<'a>>,
    result: &mut LeafTraversal<'a>,
) {
    if node.string_unit.is_some() && node.variations.is_some() {
        diagnostic(
            result,
            path,
            LeafDiagnosticCode::InvalidShape,
            "stringUnit and variations cannot coexist safely",
        );
    }
    if let Some(unit) = &node.string_unit {
        result.leaves.push(CatalogLeaf {
            path: path.clone(),
            unit,
            substitutions: substitutions.clone(),
        });
    }
    if let Some(variations) = &node.variations {
        visit_variations(variations, path, substitutions, result);
    }
    if let Some(entries) = &node.substitutions {
        if entries.is_empty() {
            diagnostic(
                result,
                path,
                LeafDiagnosticCode::EmptyAxis,
                "empty substitutions map",
            );
        }
        for (name, substitution) in entries {
            path.push(LeafStep::Substitution(name.clone()));
            if substitution.arg_num.is_none()
                || substitution
                    .format_specifier
                    .as_deref()
                    .is_none_or(str::is_empty)
            {
                diagnostic(
                    result,
                    path,
                    LeafDiagnosticCode::MissingSubstitutionMetadata,
                    "substitution needs argNum and formatSpecifier",
                );
            }
            if substitution.arg_num.is_some()
                && substitution
                    .format_specifier
                    .as_ref()
                    .is_some_and(|value| !value.is_empty())
                && let Err(detail) = substitution.validate_metadata()
            {
                diagnostic(
                    result,
                    path,
                    LeafDiagnosticCode::InvalidSubstitutionMetadata,
                    detail,
                );
            }
            substitutions.push(SubstitutionContext { name, substitution });
            if let Some(variations) = &substitution.variations {
                visit_variations(variations, path, substitutions, result);
            } else {
                diagnostic(
                    result,
                    path,
                    LeafDiagnosticCode::EmptyLocalization,
                    "substitution has no variations",
                );
            }
            substitutions.pop();
            path.pop();
        }
    }
    if node.string_unit.is_none() && node.variations.is_none() && node.substitutions.is_none() {
        diagnostic(
            result,
            path,
            LeafDiagnosticCode::EmptyLocalization,
            "localization has no known translation leaves",
        );
    }
}

fn visit_variations<'a>(
    variations: &'a Variations,
    path: &mut LeafPath,
    substitutions: &mut Vec<SubstitutionContext<'a>>,
    result: &mut LeafTraversal<'a>,
) {
    if variations.plural.is_some() && variations.device.is_some() {
        diagnostic(
            result,
            path,
            LeafDiagnosticCode::InvalidShape,
            "plural and device axes cannot coexist safely",
        );
    }
    for name in variations.extra.keys() {
        diagnostic(
            result,
            path,
            LeafDiagnosticCode::UnknownAxis,
            format!("unsupported variation axis '{name}'"),
        );
    }
    if let Some(plurals) = &variations.plural {
        if plurals.is_empty() {
            diagnostic(
                result,
                path,
                LeafDiagnosticCode::EmptyAxis,
                "empty plural map",
            );
        }
        for (category, node) in plurals {
            path.push(LeafStep::Plural(category.clone()));
            if !matches!(
                category.as_str(),
                "zero" | "one" | "two" | "few" | "many" | "other"
            ) {
                diagnostic(
                    result,
                    path,
                    LeafDiagnosticCode::UnknownPlural,
                    format!("unsupported plural category '{category}'"),
                );
            }
            visit_localization(node, path, substitutions, result);
            path.pop();
        }
    }
    if let Some(devices) = &variations.device {
        if devices.is_empty() {
            diagnostic(
                result,
                path,
                LeafDiagnosticCode::EmptyAxis,
                "empty device map",
            );
        }
        for (category, node) in devices {
            path.push(LeafStep::Device(category.clone()));
            if matches!(category, DeviceCategory::Unknown(_)) {
                diagnostic(
                    result,
                    path,
                    LeafDiagnosticCode::UnknownDevice,
                    "unsupported device category",
                );
            }
            visit_localization(node, path, substitutions, result);
            path.pop();
        }
    }
    if variations.plural.is_none() && variations.device.is_none() && variations.extra.is_empty() {
        diagnostic(
            result,
            path,
            LeafDiagnosticCode::EmptyAxis,
            "variations has no axes",
        );
    }
}

/// Resolve an existing leaf without creating or guessing any branch metadata.
pub fn find_leaf<'a>(localization: &'a Localization, path: &[LeafStep]) -> Option<&'a StringUnit> {
    let Some((step, rest)) = path.split_first() else {
        return localization.string_unit.as_ref();
    };
    match step {
        LeafStep::Substitution(name) => find_variation_leaf(
            localization
                .substitutions
                .as_ref()?
                .get(name)?
                .variations
                .as_ref()?,
            rest,
        ),
        _ => find_variation_leaf(localization.variations.as_ref()?, path),
    }
}

fn find_variation_leaf<'a>(
    variations: &'a Variations,
    path: &[LeafStep],
) -> Option<&'a StringUnit> {
    let (step, rest) = path.split_first()?;
    let node = match step {
        LeafStep::Device(category) => variations.device.as_ref()?.get(category)?,
        LeafStep::Plural(category) => variations.plural.as_ref()?.get(category)?,
        LeafStep::Substitution(_) => return None,
    };
    find_leaf(node, rest)
}

/// Resolve a mutable payload, retaining its surrounding metadata and layout.
pub fn find_leaf_mut<'a>(
    localization: &'a mut Localization,
    path: &[LeafStep],
) -> Option<&'a mut StringUnit> {
    let Some((step, rest)) = path.split_first() else {
        return localization.string_unit.as_mut();
    };
    match step {
        LeafStep::Substitution(name) => find_variation_leaf_mut(
            localization
                .substitutions
                .as_mut()?
                .get_mut(name)?
                .variations
                .as_mut()?,
            rest,
        ),
        _ => find_variation_leaf_mut(localization.variations.as_mut()?, path),
    }
}

fn find_variation_leaf_mut<'a>(
    variations: &'a mut Variations,
    path: &[LeafStep],
) -> Option<&'a mut StringUnit> {
    let (step, rest) = path.split_first()?;
    let node = match step {
        LeafStep::Device(category) => variations.device.as_mut()?.get_mut(category)?,
        LeafStep::Plural(category) => variations.plural.as_mut()?.get_mut(category)?,
        LeafStep::Substitution(_) => return None,
    };
    find_leaf_mut(node, rest)
}

/// Supported native catalog branch ordering, independent of XLIFF ID restrictions.
pub fn validate_supported_path(path: &[LeafStep]) -> Result<(), String> {
    for (i, step) in path.iter().enumerate() {
        match step {
            LeafStep::Device(device) => {
                if matches!(device, DeviceCategory::Unknown(_)) {
                    return Err("unknown device category".into());
                }
                if i != 0 || (*device == DeviceCategory::Other && i + 1 != path.len()) {
                    return Err(
                        "device variation order is not supported by the Apple compiler".into(),
                    );
                }
            }
            LeafStep::Plural(category) => {
                if !matches!(
                    category.as_str(),
                    "zero" | "one" | "two" | "few" | "many" | "other"
                ) {
                    return Err(format!("unknown plural category '{category}'"));
                }
                if i + 1 != path.len() {
                    return Err("plural case cannot be further varied".into());
                }
            }
            LeafStep::Substitution(name) => {
                if name.is_empty()
                    || i != 0
                    || !matches!(path.get(i + 1), Some(LeafStep::Plural(_)))
                {
                    return Err("substitution must end in a plural case".into());
                }
            }
        }
    }
    Ok(())
}
