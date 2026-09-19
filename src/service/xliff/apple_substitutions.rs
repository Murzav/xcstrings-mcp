use super::apple_import::Work;
use crate::model::{
    specifier::analyze_format,
    xcstrings::{Localization, Substitution, XcStringsFile, paths::LeafStep},
};
use std::collections::HashMap;
use unicode_script::{Script, UnicodeScript};

pub(super) type Metadata = HashMap<(String, Vec<LeafStep>, String), Substitution>;
pub(super) use crate::model::xcstrings::references::substitution_references as references;

pub(super) fn node<'a>(root: &'a Localization, path: &[LeafStep]) -> Option<&'a Localization> {
    let mut current = root;
    for step in path {
        current = match step {
            LeafStep::Device(category) => current
                .variations
                .as_ref()?
                .device
                .as_ref()?
                .get(category)?,
            LeafStep::Plural(category) => current
                .variations
                .as_ref()?
                .plural
                .as_ref()?
                .get(category)?,
            LeafStep::Substitution(_) => return None,
        };
    }
    Some(current)
}

pub(super) fn gather(file: &XcStringsFile, works: &[Work<'_>]) -> Result<Metadata, String> {
    let mut result = Metadata::new();
    for work in works {
        for (i, step) in work.destination.path.iter().enumerate() {
            let LeafStep::Substitution(name) = step else {
                continue;
            };
            let parent = work.destination.path[..i].to_vec();
            let key = (work.destination.key.clone(), parent.clone(), name.clone());
            if result.contains_key(&key) {
                continue;
            }
            let entry = &file.strings[&work.destination.key];
            let existing = entry.localizations.as_ref().and_then(|locs| {
                [
                    work.destination.locale.as_str(),
                    file.source_language.as_str(),
                ]
                .into_iter()
                .find_map(|locale| {
                    node(locs.get(locale)?, &parent)?
                        .substitutions
                        .as_ref()?
                        .get(name)
                })
            });
            let sub = if let Some(existing) = existing {
                validate_metadata(existing)?;
                // Clone the template because the new catalog owns its metadata.
                existing.clone()
            } else {
                let mut positions = Vec::new();
                for parent_work in works.iter().filter(|candidate| {
                    candidate.destination.key == work.destination.key
                        && !candidate
                            .destination
                            .path
                            .iter()
                            .any(|step| matches!(step, LeafStep::Substitution(_)))
                }) {
                    if let Some(text) = parent_work.unit.target.as_deref() {
                        positions.extend(
                            references(text)?
                                .into_iter()
                                .filter(|reference| reference.name == *name)
                                .filter_map(|reference| reference.position),
                        );
                    }
                }
                let Some(position) = positions.first().copied() else {
                    return Err(format!(
                        "new substitution '{name}' needs an explicit positional parent reference"
                    ));
                };
                if positions.iter().any(|p| *p != position) {
                    return Err(format!(
                        "inconsistent argument positions for substitution '{name}'"
                    ));
                }
                let mut specifier = None;
                for leaf in works.iter().filter(|w| {
                    w.destination.key == work.destination.key
                        && w.destination.path.starts_with(&work.destination.path[..=i])
                }) {
                    let Some(value) = leaf.unit.target.as_deref() else {
                        continue;
                    };
                    for arg in analyze_format(value).arguments {
                        if arg.position == Some(position) {
                            let candidate = format!(
                                "{}{}",
                                arg.length_modifier.as_deref().unwrap_or(""),
                                arg.conversion
                            );
                            if specifier.as_ref().is_some_and(|s| s != &candidate) {
                                return Err(format!(
                                    "inconsistent argument types for substitution '{name}'"
                                ));
                            }
                            specifier = Some(candidate);
                        }
                    }
                }
                let specifier = specifier.ok_or_else(|| {
                    format!("new substitution '{name}' needs a typed positional leaf argument")
                })?;
                Substitution {
                    arg_num: Some(position),
                    format_specifier: Some(specifier),
                    ..Substitution::default()
                }
            };
            result.insert(key, sub);
        }
    }
    // Parent-only updates still need the existing templates for placeholder validation.
    for work in works.iter().filter(|w| {
        !w.destination
            .path
            .iter()
            .any(|p| matches!(p, LeafStep::Substitution(_)))
    }) {
        let entry = &file.strings[&work.destination.key];
        if let Some(locs) = &entry.localizations {
            for locale in [&work.destination.locale, &file.source_language] {
                if let Some(subs) = locs.get(locale).and_then(|n| n.substitutions.as_ref()) {
                    for (name, sub) in subs {
                        validate_metadata(sub)?;
                        result
                            .entry((work.destination.key.clone(), Vec::new(), name.clone()))
                            .or_insert_with(|| sub.clone());
                    }
                }
            }
        }
    }
    Ok(result)
}
pub(super) fn validate_metadata(sub: &Substitution) -> Result<(u32, &str), String> {
    sub.validate_metadata()
}

pub(super) fn export_text(
    value: &str,
    localization: &Localization,
    path: &[LeafStep],
) -> Result<String, String> {
    let mut result = value.to_string();
    if let Some(index) = path
        .iter()
        .position(|p| matches!(p, LeafStep::Substitution(_)))
    {
        let LeafStep::Substitution(name) = &path[index] else {
            return Err("invalid substitution path".into());
        };
        let sub = node(localization, &path[..index])
            .and_then(|n| n.substitutions.as_ref())
            .and_then(|s| s.get(name))
            .ok_or("missing substitution metadata")?;
        let (number, spec) = validate_metadata(sub)?;
        result = replace_arg(&result, &format!("%{number}${spec}"));
    } else if let Some(subs) = &localization.substitutions {
        let refs = references(value)?;
        for reference in refs.iter().rev() {
            let sub = subs
                .get(&reference.name)
                .ok_or_else(|| format!("undefined substitution '{}'", reference.name))?;
            let (number, _) = validate_metadata(sub)?;
            if reference.position.is_some_and(|n| n != number) {
                return Err("substitution position disagrees with argNum".into());
            }
            result.replace_range(
                reference.start..reference.end,
                &format!("%{number}$#@{}@", reference.name),
            );
        }
    }
    Ok(result)
}
fn replace_arg(text: &str, replacement: &str) -> String {
    let mut result = String::new();
    let mut cursor = 0;
    while let Some(offset) = text[cursor..].find("%arg") {
        let start = cursor + offset;
        let end = start + 4;
        result.push_str(&text[cursor..start]);
        let escaped = text[..start]
            .bytes()
            .rev()
            .take_while(|b| *b == b'%')
            .count()
            % 2
            == 1;
        let continues = text[end..].chars().next().is_some_and(|ch| {
            (ch.is_alphanumeric() || ch == '_')
                && !matches!(
                    ch.script(),
                    Script::Han | Script::Hiragana | Script::Katakana | Script::Hangul
                )
        });
        result.push_str(if escaped || continues {
            "%arg"
        } else {
            replacement
        });
        cursor = end;
    }
    result.push_str(&text[cursor..]);
    result
}

pub(super) fn import_text(work: &Work<'_>, metadata: &Metadata) -> Result<String, String> {
    let mut value = work.unit.target.clone().ok_or("missing target")?;
    if let Some(index) = work
        .destination
        .path
        .iter()
        .position(|p| matches!(p, LeafStep::Substitution(_)))
    {
        let LeafStep::Substitution(name) = &work.destination.path[index] else {
            return Err("invalid substitution path".into());
        };
        let sub = &metadata[&(
            work.destination.key.clone(),
            work.destination.path[..index].to_vec(),
            name.clone(),
        )];
        let (number, spec) = validate_metadata(sub)?;
        for arg in analyze_format(&value).arguments.iter().rev() {
            if arg.position == Some(number)
                && format!(
                    "{}{}",
                    arg.length_modifier.as_deref().unwrap_or(""),
                    arg.conversion
                ) == spec
                && arg.flags.is_empty()
                && arg.width.is_none()
                && arg.precision.is_none()
            {
                value.replace_range(arg.start..arg.end, "%arg");
            }
        }
    } else {
        for reference in references(&value)?.iter().rev() {
            if let Some(sub) = metadata.get(&(
                work.destination.key.clone(),
                Vec::new(),
                reference.name.clone(),
            )) {
                let (position, _) = validate_metadata(sub)?;
                if reference.position.is_some_and(|n| n != position) {
                    return Err("substitution position disagrees with metadata".into());
                }
                value.replace_range(
                    reference.start..reference.end,
                    &format!("%#@{}@", reference.name),
                );
            }
        }
    }
    Ok(value)
}
