use super::{
    apple_export, apple_id, apple_mutation,
    apple_substitutions::{self, Metadata},
};
use crate::{
    error::XcStringsError,
    model::{
        specifier::{compare_format_fragment, compare_formats},
        translation::ValidationIssue,
        xcstrings::{Substitution, TranslationState, XcStringsFile, paths::LeafStep},
        xliff::*,
    },
};
use std::collections::HashSet;

pub(super) struct Work<'a> {
    pub destination: XliffDestination,
    pub unit: &'a XliffUnit,
    pub state: TranslationState,
}

/// Validate the entire selected file scope and return an atomic catalog candidate.
/// Missing targets are no-ops; explicit empty values and draft states are retained.
pub fn plan_import(
    file: &XcStringsFile,
    document: &XliffDocument,
    original: Option<&str>,
) -> Result<XliffImportPlan, XcStringsError> {
    let scopes: HashSet<_> = document
        .files
        .iter()
        .map(|f| f.original.as_deref().unwrap_or(""))
        .collect();
    let selected = match original {
        Some(value) if scopes.contains(value) => value,
        Some(value) => {
            return Err(XcStringsError::XliffParse(format!(
                "file original '{value}' was not found"
            )));
        }
        None if scopes.len() == 1 => scopes
            .iter()
            .next()
            .copied()
            .ok_or_else(|| XcStringsError::XliffParse("document has no file scope".into()))?,
        None => {
            return Err(XcStringsError::XliffParse(
                "multiple file originals require explicit original selection".into(),
            ));
        }
    };
    let mut report = XliffImportReport {
        skipped_scopes: scopes
            .iter()
            .filter(|s| **s != selected)
            .map(|s| (*s).to_string())
            .collect(),
        ..Default::default()
    };
    report.skipped_scopes.sort();
    let mut works = Vec::new();
    let mut seen = HashSet::new();
    for section in document
        .files
        .iter()
        .filter(|f| f.original.as_deref().unwrap_or("") == selected)
    {
        if section
            .source_language
            .as_ref()
            .is_some_and(|s| s != &file.source_language)
        {
            return Err(XcStringsError::XliffParse(
                "selected source-language does not match catalog sourceLanguage".into(),
            ));
        }
        if section.target_language == file.source_language {
            return Err(XcStringsError::XliffParse(
                "XLIFF import cannot overwrite the source locale".into(),
            ));
        }
        if !report.locale.is_empty() && report.locale != section.target_language {
            return Err(XcStringsError::XliffParse(
                "selected file sections use different target languages".into(),
            ));
        }
        report.locale.clone_from(&section.target_language);
        for unit in &section.units {
            let (key, path) = match apple_id::resolve(file, &unit.id) {
                Ok(v) => v,
                Err((code, message)) => {
                    reject(&mut report, code, &unit.id, message, None);
                    continue;
                }
            };
            let destination = XliffDestination {
                original: selected.into(),
                key: key.clone(),
                locale: section.target_language.clone(),
                path,
                unit_id: unit.id.clone(),
            };
            if unit.source.is_none()
                && (!matches!(destination.path.last(), Some(LeafStep::Plural(_)))
                    || file.strings[&key]
                        .localizations
                        .as_ref()
                        .and_then(|locales| locales.get(&destination.locale))
                        .and_then(|root| {
                            crate::model::xcstrings::paths::find_leaf(root, &destination.path)
                        })
                        .is_none())
            {
                reject(
                    &mut report,
                    "missing_source_context",
                    &unit.id,
                    "source-less Apple plural unit requires an existing catalog plural leaf".into(),
                    Some(destination),
                );
                continue;
            }
            if !seen.insert((key.clone(), destination.path.clone())) {
                reject(
                    &mut report,
                    "duplicate_destination",
                    &unit.id,
                    "multiple units address the same catalog leaf".into(),
                    Some(destination),
                );
                continue;
            }
            if !file.strings[&key].should_translate {
                reject(
                    &mut report,
                    "not_translatable",
                    &unit.id,
                    "catalog key has shouldTranslate=false".into(),
                    Some(destination),
                );
                continue;
            }
            if unit.target.is_none() {
                report.missing_targets += 1;
                continue;
            }
            if let Err(message) = validate_source(file, &destination, unit) {
                reject(
                    &mut report,
                    "source_text_mismatch",
                    &unit.id,
                    message,
                    Some(destination),
                );
                continue;
            }
            let state = match state(unit) {
                Ok(state) => state,
                Err(message) => {
                    reject(
                        &mut report,
                        "unsupported_state",
                        &unit.id,
                        message,
                        Some(destination),
                    );
                    continue;
                }
            };
            works.push(Work {
                destination,
                unit,
                state,
            });
        }
    }
    for (index, work) in works.iter().enumerate() {
        if works[..index]
            .iter()
            .any(|previous| overlapping(previous, work))
        {
            reject(
                &mut report,
                "overlapping_destinations",
                &work.unit.id,
                "simple and varied units address incompatible target shapes".into(),
                Some(work.destination.clone()),
            );
        }
    }
    let metadata = match apple_substitutions::gather(file, &works) {
        Ok(value) => value,
        Err(message) => {
            reject(&mut report, "substitution_metadata", "", message, None);
            return Ok(XliffImportPlan {
                candidate: None,
                report,
            });
        }
    };
    for work in &works {
        if let Err(message) = validate_format(file, work, &metadata, &mut report.warnings) {
            reject(
                &mut report,
                "format_mismatch",
                &work.unit.id,
                message,
                Some(work.destination.clone()),
            );
        }
    }
    if !report.rejected.is_empty() {
        return Ok(XliffImportPlan {
            candidate: None,
            report,
        });
    }
    // A separately owned candidate ensures any late shape error cannot partially mutate input.
    let mut candidate = file.clone();
    for work in &works {
        let value = match apple_substitutions::import_text(work, &metadata) {
            Ok(value) => value,
            Err(message) => {
                reject(
                    &mut report,
                    "substitution_metadata",
                    &work.unit.id,
                    message,
                    Some(work.destination.clone()),
                );
                continue;
            }
        };
        if let Err(message) = apple_mutation::apply(&mut candidate, work, value, &metadata) {
            reject(
                &mut report,
                "shape_conflict",
                &work.unit.id,
                message,
                Some(work.destination.clone()),
            );
        }
    }
    if !report.rejected.is_empty() {
        return Ok(XliffImportPlan {
            candidate: None,
            report,
        });
    }
    for work in &works {
        if let Err(message) = apple_mutation::validate_candidate(&candidate, work) {
            reject(
                &mut report,
                "shape_conflict",
                &work.unit.id,
                message,
                Some(work.destination.clone()),
            );
        }
    }
    if !report.rejected.is_empty() {
        return Ok(XliffImportPlan {
            candidate: None,
            report,
        });
    }
    report.accepted = works.len();
    report.accepted_destinations = works.into_iter().map(|work| work.destination).collect();
    Ok(XliffImportPlan {
        candidate: Some(candidate),
        report,
    })
}
fn reject(
    report: &mut XliffImportReport,
    code: &str,
    id: &str,
    message: String,
    destination: Option<XliffDestination>,
) {
    report.rejected.push(XliffDiagnostic {
        code: code.into(),
        unit_id: id.into(),
        message,
        destination,
    });
}
fn overlapping(left: &Work<'_>, right: &Work<'_>) -> bool {
    if left.destination.key != right.destination.key {
        return false;
    }
    let a = &left.destination.path;
    let b = &right.destination.path;
    if a.len() == b.len() {
        return false;
    }
    let (short, long) = if a.len() < b.len() { (a, b) } else { (b, a) };
    long.starts_with(short) && !matches!(long.get(short.len()), Some(LeafStep::Substitution(_)))
}
pub(super) fn state(unit: &XliffUnit) -> Result<TranslationState, String> {
    let state = match unit.state.as_deref() {
        None | Some("translated" | "final" | "signed-off") => TranslationState::Translated,
        Some("new" | "needs-translation" | "needs-l10n" | "needs-adaptation") => {
            TranslationState::New
        }
        Some("needs-review-l10n" | "needs-review-translation" | "needs-review-adaptation") => {
            TranslationState::NeedsReview
        }
        Some(other) => return Err(format!("unsupported XLIFF target state '{other}'")),
    };
    match unit.state_qualifier.as_deref() {
        None | Some("exact-match" | "fuzzy-match") => Ok(state),
        Some("leveraged-mt") if state == TranslationState::Translated => {
            Ok(TranslationState::MachineTranslated)
        }
        Some(other) => Err(format!(
            "unsupported target state/qualifier combination '{other}'"
        )),
    }
}
fn validate_format(
    file: &XcStringsFile,
    work: &Work<'_>,
    metadata: &Metadata,
    warnings: &mut Vec<ValidationIssue>,
) -> Result<(), String> {
    let target = work.unit.target.as_deref().ok_or("missing target")?;
    if target.is_empty() {
        return Ok(());
    }
    let entry = &file.strings[&work.destination.key];
    let source = entry
        .localizations
        .as_ref()
        .and_then(|l| l.get(&file.source_language));
    let mut source_text =
        apple_export::source_text(source, &work.destination.key, &work.destination.path)
            .map_err(|e| e.to_string())?;
    if let Some(index) = work
        .destination
        .path
        .iter()
        .position(|step| matches!(step, LeafStep::Substitution(_)))
    {
        let LeafStep::Substitution(name) = &work.destination.path[index] else {
            return Err("invalid substitution path".into());
        };
        let sub = &metadata[&(
            work.destination.key.clone(),
            work.destination.path[..index].to_vec(),
            name.clone(),
        )];
        let (number, spec) = apple_substitutions::validate_metadata(sub)?;
        if source.is_none_or(|s| apple_export::source_leaf(s, &work.destination.path).is_none()) {
            source_text = format!("%{number}${spec}");
        }
    }
    // Each side has its own argument definitions; target metadata must not shadow source ABI.
    let source_text = expanded_references(&source_text, |name| {
        source.and_then(|root| root.substitutions.as_ref()?.get(name))
    })?;
    let target = expanded_references(target, |name| {
        metadata.get(&(work.destination.key.clone(), Vec::new(), name.to_owned()))
    })?;
    let comparison = if work
        .destination
        .path
        .iter()
        .any(|p| matches!(p, LeafStep::Substitution(_)))
    {
        compare_format_fragment(&source_text, &target)
    } else {
        compare_formats(&source_text, &target)
    };
    for warning in comparison.warnings {
        warnings.push(ValidationIssue {
            key: work.destination.key.clone(),
            issue_type: warning.code.into(),
            message: warning.message,
        });
    }
    if comparison.errors.is_empty() {
        Ok(())
    } else {
        Err(comparison
            .errors
            .into_iter()
            .map(|e| e.message)
            .collect::<Vec<_>>()
            .join("; "))
    }
}
fn expanded_references<'a>(
    value: &str,
    mut metadata: impl FnMut(&str) -> Option<&'a Substitution>,
) -> Result<String, String> {
    let mut result = value.to_string();
    for reference in apple_substitutions::references(value)?.iter().rev() {
        let sub = metadata(&reference.name)
            .ok_or_else(|| format!("undefined substitution '{}'", reference.name))?;
        let (number, spec) = apple_substitutions::validate_metadata(sub)?;
        if reference.position.is_some_and(|n| n != number) {
            return Err("substitution position disagrees with catalog metadata".into());
        }
        result.replace_range(reference.start..reference.end, &format!("%{number}${spec}"));
    }
    Ok(result)
}

fn validate_source(
    file: &XcStringsFile,
    destination: &XliffDestination,
    unit: &XliffUnit,
) -> Result<(), String> {
    let Some(incoming) = unit.source.as_deref() else {
        return Ok(());
    };
    let source = file.strings[&destination.key]
        .localizations
        .as_ref()
        .and_then(|locales| locales.get(&file.source_language));
    let resolved = source.and_then(|root| apple_export::source_leaf(root, &destination.path));
    if resolved.is_none()
        && destination
            .path
            .iter()
            .any(|step| matches!(step, LeafStep::Substitution(_)))
    {
        // Apple uses an ID as source for target-only substitution cases. The
        // workflow operation separately requires the captured source version.
        return Ok(());
    }
    let expected = apple_export::source_text(source, &destination.key, &destination.path)
        .map_err(|error| error.to_string())?;
    let actual = match (source, resolved) {
        (Some(root), Some((path, _))) => apple_substitutions::export_text(incoming, root, &path)?,
        _ => incoming.to_owned(),
    };
    if actual == expected {
        Ok(())
    } else {
        Err("XLIFF source text does not match current catalog source; export again before translating".into())
    }
}
