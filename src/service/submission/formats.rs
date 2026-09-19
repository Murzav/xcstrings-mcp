use crate::model::specifier::{FormatComparison, FormatComparisonIssue};
use crate::model::xcstrings::{
    Localization, Substitution, paths::LeafStep, references::substitution_references,
};

pub(crate) fn check_references(
    comparison: &mut FormatComparison,
    source: Option<&Localization>,
    target: Option<&Localization>,
    path: &[LeafStep],
    source_text: &str,
    value: &str,
) {
    if path
        .iter()
        .any(|step| matches!(step, LeafStep::Substitution(_)))
    {
        return;
    }
    if let Err(message) = validate_references(source, target, source_text)
        .and_then(|()| validate_references(target, source, value))
    {
        comparison.errors.push(FormatComparisonIssue {
            code: "substitution_reference_mismatch",
            message,
        });
    }
}

fn metadata<'a>(
    primary: Option<&'a Localization>,
    fallback: Option<&'a Localization>,
    name: &str,
) -> Result<&'a Substitution, String> {
    primary
        .and_then(|root| root.substitutions.as_ref())
        .and_then(|subs| subs.get(name))
        .or_else(|| {
            fallback
                .and_then(|root| root.substitutions.as_ref())
                .and_then(|subs| subs.get(name))
        })
        .ok_or_else(|| format!("undefined substitution '{name}'"))
}

fn validate_references(
    primary: Option<&Localization>,
    fallback: Option<&Localization>,
    text: &str,
) -> Result<(), String> {
    for reference in substitution_references(text)? {
        let (position, _) = metadata(primary, fallback, &reference.name)?.validate_metadata()?;
        if reference.position.is_some_and(|actual| actual != position) {
            return Err(format!(
                "substitution '{}' argument position disagrees with metadata",
                reference.name
            ));
        }
    }
    Ok(())
}

pub(super) fn node<'a>(root: &'a Localization, path: &[LeafStep]) -> Option<&'a Localization> {
    let Some((step, rest)) = path.split_first() else {
        return Some(root);
    };
    let branch = match step {
        LeafStep::Device(category) => root.variations.as_ref()?.device.as_ref()?.get(category)?,
        LeafStep::Plural(category) => root.variations.as_ref()?.plural.as_ref()?.get(category)?,
        LeafStep::Substitution(_) => return None,
    };
    node(branch, rest)
}

pub(crate) fn expand_references(
    primary: Option<&Localization>,
    fallback: Option<&Localization>,
    _path: &[LeafStep],
    text: &str,
) -> Result<String, String> {
    let mut expanded = text.to_owned();
    for reference in substitution_references(text)?.iter().rev() {
        let (position, specifier) =
            metadata(primary, fallback, &reference.name)?.validate_metadata()?;
        expanded.replace_range(
            reference.start..reference.end,
            &format!("%{position}${specifier}"),
        );
    }
    Ok(expanded)
}
