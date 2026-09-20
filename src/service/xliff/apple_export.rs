use super::{apple_id, apple_substitutions};
use crate::{
    error::XcStringsError,
    model::{
        plural::plural_categories,
        xcstrings::{
            Localization, StringUnit, TranslationState, XcStringsFile,
            paths::{LeafPath, LeafStep, collect_leaves, find_leaf},
        },
    },
};
use quick_xml::{
    Writer,
    events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event},
};
use std::{collections::HashSet, io::Cursor};

/// Export supported Apple catalog leaves, rejecting any lossy or ambiguous mapping.
pub fn export_xliff(
    file: &XcStringsFile,
    locale: &str,
    original: &str,
    untranslated_only: bool,
) -> Result<(String, usize), XcStringsError> {
    export_xliff_with_keys(file, locale, original, untranslated_only)
        .map(|(xml, count, _)| (xml, count))
}

/// Export and capture exactly the catalog keys represented in the emitted XML.
pub fn export_xliff_with_keys(
    file: &XcStringsFile,
    locale: &str,
    original: &str,
    untranslated_only: bool,
) -> Result<(String, usize, Vec<String>), XcStringsError> {
    if locale.trim().is_empty() {
        return Err(XcStringsError::XliffFormat("target locale is empty".into()));
    }
    valid_xml(locale)?;
    valid_xml(original)?;
    valid_xml(&file.source_language)?;
    let mut writer = Writer::new_with_indent(Cursor::new(Vec::new()), b' ', 2);
    write(
        &mut writer,
        Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)),
    )?;
    let mut root = BytesStart::new("xliff");
    root.push_attribute(("version", "1.2"));
    root.push_attribute(("xmlns", "urn:oasis:names:tc:xliff:document:1.2"));
    write(&mut writer, Event::Start(root))?;
    let mut section = BytesStart::new("file");
    section.push_attribute(("original", original));
    section.push_attribute(("source-language", file.source_language.as_str()));
    section.push_attribute(("target-language", locale));
    section.push_attribute(("datatype", "plaintext"));
    write(&mut writer, Event::Start(section))?;
    write(&mut writer, Event::Start(BytesStart::new("body")))?;
    let mut entries: Vec<_> = file.strings.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    let mut seen = HashSet::new();
    let mut count = 0;
    let mut exported_keys = Vec::new();
    for (key, entry) in entries {
        if !entry.should_translate {
            continue;
        }
        let source = entry
            .localizations
            .as_ref()
            .and_then(|l| l.get(&file.source_language));
        let target = entry.localizations.as_ref().and_then(|l| l.get(locale));
        let shape = target.or(source);
        let paths = if let Some(shape) = shape {
            export_paths(shape, locale)?
        } else {
            vec![Vec::new()]
        };
        let mut units = Vec::with_capacity(paths.len());
        for path in paths {
            if path.is_empty() {
                apple_id::validate_export_literal(key).map_err(XcStringsError::XliffFormat)?;
            }
            let id = apple_id::encode(key, &path).map_err(XcStringsError::XliffFormat)?;
            // Check the entire catalog, including literal keys and not-yet-created target branches.
            let resolved =
                apple_id::resolve(file, &id).map_err(|(_, m)| XcStringsError::XliffFormat(m))?;
            if resolved != (key.clone(), path.clone()) || !seen.insert(id.clone()) {
                return Err(XcStringsError::XliffFormat(format!(
                    "duplicate Apple XLIFF destination '{id}'"
                )));
            }
            let target_unit = target.and_then(|l| find_leaf(l, &path));
            if untranslated_only
                && target_unit.is_some_and(|u| {
                    matches!(
                        u.state,
                        TranslationState::Translated | TranslationState::MachineTranslated
                    )
                })
            {
                continue;
            }
            let source_value = export_source_text(source, target, key, &path)?;
            let target_value = target_unit
                .map(|u| {
                    apple_substitutions::export_text(
                        &u.value,
                        target.ok_or_else(|| {
                            XcStringsError::XliffFormat("missing target localization".into())
                        })?,
                        &path,
                    )
                    .map_err(XcStringsError::XliffFormat)
                })
                .transpose()?;
            units.push((id, source_value, target_value, target_unit));
        }
        if !units.is_empty() {
            exported_keys.push(key.clone());
        }
        units.sort_by(|a, b| a.0.cmp(&b.0));
        for (id, source_value, target_value, target_unit) in units {
            valid_xml(&id)?;
            valid_xml(&source_value)?;
            if let Some(value) = &target_value {
                valid_xml(value)?;
            }
            if let Some(comment) = &entry.comment {
                valid_xml(comment)?;
            }
            let mut unit = BytesStart::new("trans-unit");
            unit.push_attribute(("id", id.as_str()));
            unit.push_attribute(("xml:space", "preserve"));
            write(&mut writer, Event::Start(unit))?;
            text_element(&mut writer, "source", &source_value)?;
            if let (Some(value), Some(target_unit)) = (target_value, target_unit) {
                let (state, qualifier) = export_state(&target_unit.state)?;
                let mut target = BytesStart::new("target");
                target.push_attribute(("state", state));
                if let Some(qualifier) = qualifier {
                    target.push_attribute(("state-qualifier", qualifier));
                }
                write(&mut writer, Event::Start(target))?;
                write(&mut writer, Event::Text(BytesText::new(&value)))?;
                write(&mut writer, Event::End(BytesEnd::new("target")))?;
            }
            if let Some(comment) = &entry.comment {
                let mut note = BytesStart::new("note");
                if entry
                    .extra
                    .get("isCommentAutoGenerated")
                    .and_then(|v| v.as_bool())
                    == Some(true)
                {
                    note.push_attribute(("from", "auto-generated"));
                }
                write(&mut writer, Event::Start(note))?;
                write(&mut writer, Event::Text(BytesText::new(comment)))?;
                write(&mut writer, Event::End(BytesEnd::new("note")))?;
            }
            write(&mut writer, Event::End(BytesEnd::new("trans-unit")))?;
            count += 1;
        }
    }
    for name in ["body", "file", "xliff"] {
        write(&mut writer, Event::End(BytesEnd::new(name)))?;
    }
    let xml = String::from_utf8(writer.into_inner().into_inner())
        .map_err(|e| XcStringsError::XliffFormat(e.to_string()))?;
    Ok((xml, count, exported_keys))
}
fn export_paths(
    localization: &Localization,
    locale: &str,
) -> Result<Vec<LeafPath>, XcStringsError> {
    crate::service::assessment::validate_substitution_references(localization)
        .map_err(XcStringsError::XliffFormat)?;
    let traversal = collect_leaves(localization);
    if !traversal.diagnostics.is_empty() {
        return Err(XcStringsError::XliffFormat(format!(
            "unsupported catalog shape: {:?}",
            traversal.diagnostics
        )));
    }
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for leaf in traversal.leaves {
        apple_id::validate_path(&leaf.path).map_err(XcStringsError::XliffFormat)?;
        if seen.insert(leaf.path.clone()) {
            paths.push(leaf.path.clone());
        }
        if matches!(leaf.path.last(), Some(LeafStep::Plural(_))) {
            for category in plural_categories(locale)? {
                let mut path = leaf.path.clone();
                path.pop();
                path.push(LeafStep::Plural(category.as_str().into()));
                if seen.insert(path.clone()) {
                    paths.push(path);
                }
            }
        }
    }
    if paths.is_empty() {
        return Err(XcStringsError::XliffFormat(
            "catalog localization has no exportable leaves".into(),
        ));
    }
    Ok(paths)
}

fn export_source_text(
    source: Option<&Localization>,
    target: Option<&Localization>,
    key: &str,
    path: &[LeafStep],
) -> Result<String, XcStringsError> {
    let target_only_substitution = path
        .iter()
        .any(|step| matches!(step, LeafStep::Substitution(_)))
        && source.is_none_or(|root| source_leaf(root, path).is_none());
    if target_only_substitution
        && let Some(target) = target
        && find_leaf(target, path).is_none()
    {
        let mut fallback = path.to_vec();
        fallback.pop();
        fallback.push(LeafStep::Plural("other".into()));
        if find_leaf(target, &fallback).is_some() {
            // Xcode checks this source context against an existing exported target-only case.
            return apple_id::encode(key, &fallback).map_err(XcStringsError::XliffFormat);
        }
        return Err(XcStringsError::XliffFormat(
            "new target-only substitution category requires an existing other source context"
                .into(),
        ));
    }
    source_text(source, key, path)
}

pub(super) fn source_text(
    source: Option<&Localization>,
    key: &str,
    path: &[LeafStep],
) -> Result<String, XcStringsError> {
    if let Some(source) = source
        && let Some((resolved, unit)) = source_leaf(source, path)
    {
        return apple_substitutions::export_text(&unit.value, source, &resolved)
            .map_err(XcStringsError::XliffFormat);
    }
    if path.iter().any(|p| matches!(p, LeafStep::Substitution(_))) {
        return apple_id::encode(key, path).map_err(XcStringsError::XliffFormat);
    }
    Ok(key.into())
}
/// Match the requested branch, then its explicit other fallback, then a simple source.
pub(super) fn source_leaf<'a>(
    source: &'a Localization,
    path: &[LeafStep],
) -> Option<(LeafPath, &'a StringUnit)> {
    if let Some(unit) = find_leaf(source, path) {
        return Some((path.to_vec(), unit));
    }
    if let Some(LeafStep::Plural(_)) = path.last() {
        let mut fallback = path.to_vec();
        fallback.pop();
        fallback.push(LeafStep::Plural("other".into()));
        if let Some(unit) = find_leaf(source, &fallback) {
            return Some((fallback, unit));
        }
    }
    if path.iter().any(|p| matches!(p, LeafStep::Substitution(_))) {
        return None;
    }
    if let Some(LeafStep::Device(_)) = path.first() {
        let fallback = vec![LeafStep::Device(
            crate::model::xcstrings::DeviceCategory::Other,
        )];
        if let Some(unit) = find_leaf(source, &fallback) {
            return Some((fallback, unit));
        }
    }
    if let Some(unit) = &source.string_unit {
        return Some((Vec::new(), unit));
    }
    for fallback in [
        vec![LeafStep::Plural("other".into())],
        vec![LeafStep::Device(
            crate::model::xcstrings::DeviceCategory::Other,
        )],
    ] {
        if let Some(unit) = find_leaf(source, &fallback) {
            return Some((fallback, unit));
        }
    }
    None
}
fn export_state(
    state: &TranslationState,
) -> Result<(&'static str, Option<&'static str>), XcStringsError> {
    match state {
        TranslationState::Translated => Ok(("translated", None)),
        TranslationState::New => Ok(("new", None)),
        TranslationState::NeedsReview => Ok(("needs-review-l10n", None)),
        TranslationState::MachineTranslated => Ok(("translated", Some("leveraged-mt"))),
        _ => Err(XcStringsError::XliffFormat(format!(
            "catalog state {state:?} has no lossless Apple XLIFF mapping"
        ))),
    }
}
fn text_element(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    name: &str,
    value: &str,
) -> Result<(), XcStringsError> {
    write(writer, Event::Start(BytesStart::new(name)))?;
    write(writer, Event::Text(BytesText::new(value)))?;
    write(writer, Event::End(BytesEnd::new(name)))
}
fn write(writer: &mut Writer<Cursor<Vec<u8>>>, event: Event<'_>) -> Result<(), XcStringsError> {
    writer
        .write_event(event)
        .map_err(|e| XcStringsError::XliffFormat(e.to_string()))
}

fn valid_xml(value: &str) -> Result<(), XcStringsError> {
    if value.chars().any(|c| {
        (c < ' ' && !matches!(c, '\t' | '\n' | '\r')) || matches!(c, '\u{fffe}' | '\u{ffff}')
    }) {
        return Err(XcStringsError::XliffFormat(
            "catalog text contains a character forbidden by XML 1.0".into(),
        ));
    }
    Ok(())
}
