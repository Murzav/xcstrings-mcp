use quick_xml::events::{BytesStart, Event};
use quick_xml::{NsReader, XmlVersion};

use crate::error::XcStringsError;
use crate::model::translation::CompletedTranslation;
use crate::model::xliff::XliffDocument;

mod import_state;
mod import_validation;

use import_state::{CoreElement, ImportElementKind, ImportState};
use import_validation::DocumentValidator;

mod apple_export;
mod apple_id;
mod apple_import;
mod apple_mutation;
mod apple_substitutions;
pub use apple_export::{export_xliff, export_xliff_with_keys};
pub use apple_import::plan_import;

/// Parse a validated XLIFF 1.2 document while retaining file scope, target presence,
/// target state, notes, and Apple variation IDs. Use `plan_import` to resolve IDs
/// against a catalog and validate a complete, atomic import candidate.
pub fn parse_document(xliff_content: &str) -> Result<XliffDocument, XcStringsError> {
    use quick_xml::escape::resolve_xml_entity;

    let mut reader = NsReader::from_str(xliff_content);

    let mut document = DocumentValidator::new();
    let mut state = ImportState::new();

    loop {
        let event = reader
            .read_event()
            .map_err(|error| XcStringsError::XliffParse(error.to_string()))?;
        match event {
            Event::Start(ref e) => {
                let (namespace, _) = reader.resolver().resolve_element(e.name());
                let element = document.start(&namespace, e, reader.resolver())?;
                let target_locale =
                    semantic_attribute(e, &element.kind, CoreElement::File, "target-language")?;
                reject_unsafe_inline(&element.kind, &element.name, e)?;
                let unit_id = unit_id_attribute(e, &element.kind)?;
                state.start(element, target_locale, unit_id)?;
                state.attributes(e)?;
            }
            Event::Empty(ref e) => {
                let (namespace, _) = reader.resolver().resolve_element(e.name());
                let element = document.empty(&namespace, e, reader.resolver())?;
                let target_locale =
                    semantic_attribute(e, &element.kind, CoreElement::File, "target-language")?;
                reject_unsafe_inline(&element.kind, &element.name, e)?;
                let unit_id = unit_id_attribute(e, &element.kind)?;
                state.start(element.clone(), target_locale, unit_id)?;
                state.attributes(e)?;
                state.end(element)?;
            }
            Event::Text(ref e) => {
                let text = e.as_ref();
                document.text(text)?;
                state.text(text);
            }
            Event::GeneralRef(ref e) => {
                document.general_reference()?;
                let name = e.as_ref();
                let resolved = if let Some(s) = resolve_xml_entity(name) {
                    s.to_owned()
                } else if let Ok(Some(ch)) = e.resolve_char_ref() {
                    ch.to_string()
                } else {
                    return Err(XcStringsError::XliffParse(format!(
                        "unknown XML entity: &{name};"
                    )));
                };
                state.text(&resolved);
            }
            Event::End(ref e) => {
                let (namespace, _) = reader.resolver().resolve_element(e.name());
                let local_name = e.local_name();
                let element = document.end(&namespace, local_name.as_ref())?;
                state.end(element)?;
            }
            Event::CData(ref e) => {
                document.cdata()?;
                let text = e.as_ref();
                state.text(text);
            }
            Event::Decl(_) => document.declaration()?,
            Event::DocType(_) => document.doctype()?,
            Event::Eof => {
                document.finish()?;
                break;
            }
            _ => {}
        }
    }
    state.finish()
}

/// Compatibility adapter for the historical unscoped simple-string API.
/// Rejects scopes and states that CompletedTranslation cannot represent.
/// New callers should use parse_document and plan_import for complete semantics.
pub fn import_xliff(content: &str) -> Result<(String, Vec<CompletedTranslation>), XcStringsError> {
    let document = parse_document(content)?;
    let originals: std::collections::HashSet<_> = document
        .files
        .iter()
        .map(|file| file.original.as_deref().unwrap_or(""))
        .collect();
    if originals.len() > 1 {
        return Err(XcStringsError::XliffParse(
            "legacy import_xliff cannot preserve multiple file originals; use parse_document and plan_import".into(),
        ));
    }
    let mut locale = String::new();
    let mut ids = std::collections::HashSet::new();
    let mut translations = Vec::new();
    for file in document.files {
        if !locale.is_empty() && locale != file.target_language {
            return Err(XcStringsError::XliffParse(format!(
                "multiple <file> elements use different target-language values: '{locale}' and '{}'",
                file.target_language
            )));
        }
        locale.clone_from(&file.target_language);
        for unit in file.units {
            if unit.id.contains("|==|") {
                return Err(XcStringsError::XliffParse(format!(
                    "Apple XLIFF variation unit id '{}' is unsupported; import simple stringUnit ids only",
                    unit.id
                )));
            }
            if !ids.insert(unit.id.clone()) {
                return Err(XcStringsError::XliffParse(format!(
                    "XLIFF unit id '{}' is repeated across <file> elements and cannot be flattened safely",
                    unit.id
                )));
            }
            if unit.target.is_some()
                && !matches!(
                    apple_import::state(&unit),
                    Ok(crate::model::xcstrings::TranslationState::Translated)
                )
            {
                return Err(XcStringsError::XliffParse(format!(
                    "legacy import_xliff cannot preserve target state for '{}'; use parse_document and plan_import",
                    unit.id
                )));
            }
            if let Some(value) = unit.target {
                translations.push(CompletedTranslation {
                    key: unit.id,
                    // This legacy decoder has no captured catalog input revision.
                    // Guarded callers must supply the original export's version.
                    expected_source_version: String::new(),
                    locale: locale.clone(),
                    value,
                    plural_forms: None,
                    substitution_name: None,
                    path: None,
                });
            }
        }
    }
    Ok((locale, translations))
}

fn reject_unsafe_inline(
    kind: &ImportElementKind,
    name: &str,
    element: &BytesStart<'_>,
) -> Result<(), XcStringsError> {
    if *kind == ImportElementKind::Core(CoreElement::Inline)
        && (matches!(name, "x" | "bx" | "ex")
            || normalized_attribute(element, "equiv-text")?.is_some())
    {
        return Err(XcStringsError::XliffParse(format!(
            "inline <{name}> carries unsupported placeholder semantics"
        )));
    }
    Ok(())
}

fn semantic_attribute(
    element: &BytesStart<'_>,
    kind: &ImportElementKind,
    expected: CoreElement,
    attribute: &str,
) -> Result<Option<String>, XcStringsError> {
    if *kind == ImportElementKind::Core(expected) {
        normalized_attribute(element, attribute)
    } else {
        Ok(None)
    }
}

fn unit_id_attribute(
    element: &BytesStart<'_>,
    kind: &ImportElementKind,
) -> Result<Option<String>, XcStringsError> {
    match kind {
        ImportElementKind::Core(CoreElement::TransUnit | CoreElement::BinUnit) => {
            normalized_attribute(element, "id")
        }
        _ => Ok(None),
    }
}

fn normalized_attribute(
    element: &BytesStart<'_>,
    name: &str,
) -> Result<Option<String>, XcStringsError> {
    for attribute in element.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| XcStringsError::XliffParse(error.to_string()))?;
        if attribute.key.as_ref() == name {
            let value = attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .map_err(|error| XcStringsError::XliffParse(error.to_string()))?;
            return Ok(Some(value.into_owned()));
        }
    }
    Ok(None)
}

#[cfg(test)]
#[path = "xliff/tests.rs"]
mod tests;
