use std::io::Cursor;

use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Decoder, NsReader, Writer, XmlVersion};

use crate::error::XcStringsError;
use crate::model::translation::CompletedTranslation;
use crate::model::xcstrings::{TranslationState, XcStringsFile};

mod import_state;
mod import_validation;

use import_state::{CoreElement, ImportElementKind, ImportState};
use import_validation::DocumentValidator;

/// Export an XcStringsFile to XLIFF 1.2 XML format.
///
/// Parameters:
/// - `file`: the parsed .xcstrings data
/// - `target_locale`: locale to export translations for
/// - `original`: the original filename (e.g., "Localizable.xcstrings")
/// - `untranslated_only`: if true, only include untranslated/new strings
///
/// Returns `(xml_string, exported_count)`.
///
/// **Scope**: Exports only entries that have simple `stringUnit` semantics.
/// Variation-only entries are excluded because this exporter does not implement
/// Apple's variation-unit ID mapping.
pub fn export_xliff(
    file: &XcStringsFile,
    target_locale: &str,
    original: &str,
    untranslated_only: bool,
) -> Result<(String, usize), XcStringsError> {
    let mut writer = Writer::new_with_indent(Cursor::new(Vec::new()), b' ', 2);

    write_event(
        &mut writer,
        Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)),
    )?;

    let mut xliff = BytesStart::new("xliff");
    xliff.push_attribute(("version", "1.2"));
    xliff.push_attribute(("xmlns", "urn:oasis:names:tc:xliff:document:1.2"));
    write_event(&mut writer, Event::Start(xliff))?;

    let mut file_elem = BytesStart::new("file");
    file_elem.push_attribute(("source-language", file.source_language.as_str()));
    file_elem.push_attribute(("target-language", target_locale));
    file_elem.push_attribute(("original", original));
    file_elem.push_attribute(("datatype", "plaintext"));
    write_event(&mut writer, Event::Start(file_elem))?;

    write_event(&mut writer, Event::Start(BytesStart::new("body")))?;

    let source_lang = &file.source_language;
    let mut exported_count = 0;

    for (key, entry) in &file.strings {
        if !entry.should_translate || is_variation_only(entry) {
            continue;
        }

        let locs = entry.localizations.as_ref();

        let source_text = locs
            .and_then(|l| l.get(source_lang))
            .and_then(|loc| loc.string_unit.as_ref())
            .map(|su| su.value.as_str())
            .unwrap_or(key.as_str());

        let target_info = locs
            .and_then(|l| l.get(target_locale))
            .and_then(|loc| loc.string_unit.as_ref());

        let (target_text, state) = match target_info {
            Some(su) => {
                let state_str = match &su.state {
                    TranslationState::Translated => "translated",
                    TranslationState::NeedsReview => "needs-review-translation",
                    _ => "new",
                };
                (su.value.as_str(), state_str)
            }
            None => ("", "new"),
        };

        if untranslated_only && state == "translated" && !target_text.is_empty() {
            continue;
        }

        write_trans_unit(
            &mut writer,
            key,
            source_text,
            target_text,
            state,
            entry.comment.as_deref(),
        )?;
        exported_count += 1;
    }

    write_event(&mut writer, Event::End(BytesEnd::new("body")))?;
    write_event(&mut writer, Event::End(BytesEnd::new("file")))?;
    write_event(&mut writer, Event::End(BytesEnd::new("xliff")))?;

    let result = writer.into_inner().into_inner();
    let xml = String::from_utf8(result).map_err(|e| XcStringsError::XliffFormat(e.to_string()))?;
    Ok((xml, exported_count))
}

fn is_variation_only(entry: &crate::model::xcstrings::StringEntry) -> bool {
    let Some(localizations) = &entry.localizations else {
        return false;
    };
    let has_simple_unit = localizations
        .values()
        .any(|localization| localization.string_unit.is_some());
    let has_variations = localizations.values().any(|localization| {
        localization.variations.is_some() || localization.substitutions.is_some()
    });
    !has_simple_unit && has_variations
}

/// Write a single trans-unit element.
fn write_trans_unit(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    id: &str,
    source: &str,
    target: &str,
    state: &str,
    comment: Option<&str>,
) -> Result<(), XcStringsError> {
    let mut tu = BytesStart::new("trans-unit");
    tu.push_attribute(("id", id));
    write_event(writer, Event::Start(tu))?;

    // <source>
    write_event(writer, Event::Start(BytesStart::new("source")))?;
    write_event(writer, Event::Text(BytesText::new(source)))?;
    write_event(writer, Event::End(BytesEnd::new("source")))?;

    // <target>
    let mut target_elem = BytesStart::new("target");
    target_elem.push_attribute(("state", state));
    if target.is_empty() {
        write_event(writer, Event::Empty(target_elem))?;
    } else {
        write_event(writer, Event::Start(target_elem))?;
        write_event(writer, Event::Text(BytesText::new(target)))?;
        write_event(writer, Event::End(BytesEnd::new("target")))?;
    }

    // <note>
    if let Some(note) = comment {
        write_event(writer, Event::Start(BytesStart::new("note")))?;
        write_event(writer, Event::Text(BytesText::new(note)))?;
        write_event(writer, Event::End(BytesEnd::new("note")))?;
    }

    write_event(writer, Event::End(BytesEnd::new("trans-unit")))?;
    Ok(())
}

/// Helper to write an XML event, mapping errors to `XcStringsError`.
fn write_event(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    event: Event<'_>,
) -> Result<(), XcStringsError> {
    writer
        .write_event(event)
        .map_err(|e| XcStringsError::XliffFormat(e.to_string()))
}

/// Parse XLIFF 1.2 XML and extract translations as `CompletedTranslation` vectors.
///
/// Returns `(target_locale, translations)`.
///
/// Multiple `<file>` sections are accepted only when every section has the
/// same non-empty `target-language`, which is the only locale shape this
/// return type can represent without losing scope.
///
/// **Scope**: Imports only simple string-unit IDs. Apple variation-unit IDs are
/// rejected because this importer does not implement their path semantics. Use
/// `submit_translations` with `plural_forms` for plural key translations.
pub fn import_xliff(
    xliff_content: &str,
) -> Result<(String, Vec<CompletedTranslation>), XcStringsError> {
    use quick_xml::escape::resolve_xml_entity;

    let mut reader = NsReader::from_str(xliff_content);

    let mut document = DocumentValidator::new();
    let mut state = ImportState::new();

    loop {
        let decoder = reader.decoder();
        let event = reader
            .read_event()
            .map_err(|error| XcStringsError::XliffParse(error.to_string()))?;
        match event {
            Event::Start(ref e) => {
                let (namespace, _) = reader.resolver().resolve_element(e.name());
                let element = document.start(decoder, &namespace, e, reader.resolver())?;
                let target_locale = semantic_attribute(
                    decoder,
                    e,
                    &element.kind,
                    CoreElement::File,
                    b"target-language",
                )?;
                let unit_id = unit_id_attribute(decoder, e, &element.kind)?;
                state.start(element, target_locale, unit_id)?;
            }
            Event::Empty(ref e) => {
                let (namespace, _) = reader.resolver().resolve_element(e.name());
                let element = document.empty(decoder, &namespace, e, reader.resolver())?;
                let target_locale = semantic_attribute(
                    decoder,
                    e,
                    &element.kind,
                    CoreElement::File,
                    b"target-language",
                )?;
                let unit_id = unit_id_attribute(decoder, e, &element.kind)?;
                state.start(element.clone(), target_locale, unit_id)?;
                state.end(element)?;
            }
            Event::Text(ref e) => {
                let text = e
                    .decode()
                    .map_err(|err| XcStringsError::XliffParse(err.to_string()))?;
                document.text(&text)?;
                state.text(&text);
            }
            Event::GeneralRef(ref e) => {
                document.general_reference()?;
                let name = e
                    .decode()
                    .map_err(|err| XcStringsError::XliffParse(err.to_string()))?;
                let resolved = if let Some(s) = resolve_xml_entity(&name) {
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
                let element = document.end(decoder, &namespace, local_name.as_ref())?;
                state.end(element)?;
            }
            Event::CData(ref e) => {
                document.cdata()?;
                let text = e
                    .decode()
                    .map_err(|err| XcStringsError::XliffParse(err.to_string()))?;
                state.text(&text);
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

fn semantic_attribute(
    decoder: Decoder,
    element: &BytesStart<'_>,
    kind: &ImportElementKind,
    expected: CoreElement,
    attribute: &[u8],
) -> Result<Option<String>, XcStringsError> {
    if *kind == ImportElementKind::Core(expected) {
        normalized_attribute(decoder, element, attribute)
    } else {
        Ok(None)
    }
}

fn unit_id_attribute(
    decoder: Decoder,
    element: &BytesStart<'_>,
    kind: &ImportElementKind,
) -> Result<Option<String>, XcStringsError> {
    match kind {
        ImportElementKind::Core(CoreElement::TransUnit | CoreElement::BinUnit) => {
            normalized_attribute(decoder, element, b"id")
        }
        _ => Ok(None),
    }
}

fn normalized_attribute(
    decoder: Decoder,
    element: &BytesStart<'_>,
    name: &[u8],
) -> Result<Option<String>, XcStringsError> {
    for attribute in element.attributes().with_checks(false) {
        let attribute = attribute.map_err(|error| XcStringsError::XliffParse(error.to_string()))?;
        if attribute.key.as_ref() == name {
            let value = attribute
                .decoded_and_normalized_value(XmlVersion::Implicit1_0, decoder)
                .map_err(|error| XcStringsError::XliffParse(error.to_string()))?;
            return Ok(Some(value.into_owned()));
        }
    }
    Ok(None)
}

#[cfg(test)]
#[path = "xliff/tests.rs"]
mod tests;
