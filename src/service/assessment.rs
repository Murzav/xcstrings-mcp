//! One completion contract for every native catalog view.
mod reference_validation;
mod shape;
pub(crate) use reference_validation::validate_substitution_references;
pub(crate) use shape::{initialize_locale, required_shape};

use crate::model::plural::plural_categories;
use crate::model::translation::{LeafSubstitution, TranslationLeaf};
use crate::model::xcstrings::paths::{
    LeafDiagnostic, LeafDiagnosticCode, LeafStep, collect_leaves, find_leaf,
    validate_supported_path,
};
use crate::model::xcstrings::{
    DeviceCategory, Localization, StringEntry, StringUnit, TranslationState, Variations,
};

#[derive(Debug, Default)]
pub struct TranslationAssessment {
    pub leaves: Vec<TranslationLeaf>,
    pub diagnostics: Vec<LeafDiagnostic>,
}

impl TranslationAssessment {
    pub fn complete(&self) -> bool {
        self.diagnostics.is_empty()
            && !self.leaves.is_empty()
            && self
                .leaves
                .iter()
                .all(|leaf| !leaf.required || leaf.complete)
    }
}

pub fn ready(state: &TranslationState) -> bool {
    matches!(
        state,
        TranslationState::Translated | TranslationState::MachineTranslated
    )
}

pub fn assess(
    key: &str,
    entry: &StringEntry,
    source_language: &str,
    locale: &str,
) -> TranslationAssessment {
    let mut report = TranslationAssessment::default();
    let categories = match plural_categories(locale) {
        Ok(value) => value,
        Err(error) => {
            report.diagnostics.push(LeafDiagnostic {
                path: vec![],
                code: LeafDiagnosticCode::UnknownLocale,
                detail: error.to_string(),
            });
            Vec::new()
        }
    };
    let source = entry
        .localizations
        .as_ref()
        .and_then(|locs| locs.get(source_language));
    let target = entry
        .localizations
        .as_ref()
        .and_then(|locs| locs.get(locale));
    let fallback = Localization::with_unit(StringUnit::new(TranslationState::New, key));
    let basis = target.or(source).unwrap_or(&fallback);
    let actual = collect_leaves(basis);
    report.diagnostics.extend(actual.diagnostics);
    if let Err(detail) = validate_substitution_references(basis) {
        report.diagnostics.push(LeafDiagnostic {
            path: vec![],
            code: LeafDiagnosticCode::InvalidShape,
            detail,
        });
    }
    let shape = required_shape(basis, &categories, target.is_none());
    let required = collect_leaves(&shape);
    for leaf in required.leaves {
        if let Err(detail) = validate_supported_path(&leaf.path) {
            report.diagnostics.push(LeafDiagnostic {
                path: leaf.path.clone(),
                code: LeafDiagnosticCode::InvalidShape,
                detail,
            });
        }
        let unit = target.and_then(|node| find_leaf(node, &leaf.path));
        let substitutions = leaf
            .substitutions
            .iter()
            .map(|context| LeafSubstitution {
                name: context.name.into(),
                arg_num: context.substitution.arg_num,
                format_specifier: context.substitution.format_specifier.clone(),
            })
            .collect();
        report.leaves.push(TranslationLeaf {
            workflow: None,
            source_text: source
                .and_then(|node| source_unit(node, &leaf.path))
                .map_or_else(|| key.to_string(), |unit| unit.value.clone()),
            path: leaf.path,
            value: unit.map(|u| u.value.clone()),
            state: unit.map(|u| u.state.clone()),
            required: true,
            complete: unit.is_some_and(|u| ready(&u.state)),
            substitutions,
        });
    }
    for leaf in &mut report.leaves {
        if report
            .diagnostics
            .iter()
            .any(|diagnostic| leaf.path.starts_with(&diagnostic.path))
        {
            leaf.complete = false;
        }
    }
    report
}

/// Resolve a source leaf by matching category, then `other`; target-only axes may
/// use a simple source. Never choose an arbitrary first category or substitution.
pub(crate) fn source_unit<'a>(node: &'a Localization, path: &[LeafStep]) -> Option<&'a StringUnit> {
    let Some((step, rest)) = path.split_first() else {
        return node
            .string_unit
            .as_ref()
            .or_else(|| default_variation(node.variations.as_ref()?));
    };
    match step {
        LeafStep::Device(category) => {
            if let Some(devices) = node.variations.as_ref().and_then(|v| v.device.as_ref()) {
                source_unit(
                    devices
                        .get(category)
                        .or_else(|| devices.get(&DeviceCategory::Other))?,
                    rest,
                )
            } else {
                source_unit(node, rest)
            }
        }
        LeafStep::Plural(category) => {
            if let Some(forms) = node.variations.as_ref().and_then(|v| v.plural.as_ref()) {
                source_unit(forms.get(category).or_else(|| forms.get("other"))?, rest)
            } else {
                source_unit(node, rest)
            }
        }
        LeafStep::Substitution(name) => {
            let sub = node.substitutions.as_ref()?.get(name)?;
            source_variation(sub.variations.as_ref()?, rest)
        }
    }
}

fn source_variation<'a>(variations: &'a Variations, path: &[LeafStep]) -> Option<&'a StringUnit> {
    let (step, rest) = path.split_first()?;
    let branch = match step {
        LeafStep::Plural(category) => {
            let forms = variations.plural.as_ref()?;
            forms.get(category).or_else(|| forms.get("other"))?
        }
        LeafStep::Device(category) => {
            let devices = variations.device.as_ref()?;
            devices
                .get(category)
                .or_else(|| devices.get(&DeviceCategory::Other))?
        }
        LeafStep::Substitution(_) => return None,
    };
    source_unit(branch, rest)
}

fn default_variation(variations: &Variations) -> Option<&StringUnit> {
    let branch = variations
        .plural
        .as_ref()
        .and_then(|p| p.get("other"))
        .or_else(|| {
            variations
                .device
                .as_ref()
                .and_then(|d| d.get(&DeviceCategory::Other))
        })?;
    source_unit(branch, &[])
}
