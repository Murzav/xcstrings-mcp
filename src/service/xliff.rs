use std::io::Cursor;

use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Decoder, NsReader, Writer, XmlVersion};

use crate::error::XcStringsError;
use crate::model::translation::CompletedTranslation;
use crate::model::xcstrings::{TranslationState, XcStringsFile};

mod import_validation;

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
/// **Limitation**: Only exports simple string translations. Plural forms and
/// device variant forms cannot be represented in XLIFF 1.2 format and are
/// excluded from the export.
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
        if !entry.should_translate {
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
/// **Limitation**: Only imports simple string translations. Plural forms and
/// substitution translations cannot be represented in XLIFF 1.2 format and
/// are skipped during import. Use `submit_translations` with `plural_forms`
/// for plural key translations.
pub fn import_xliff(
    xliff_content: &str,
) -> Result<(String, Vec<CompletedTranslation>), XcStringsError> {
    use quick_xml::escape::resolve_xml_entity;

    let mut reader = NsReader::from_str(xliff_content);

    let mut target_locale = String::new();
    let mut translations = Vec::new();

    let mut current_id = String::new();
    let mut in_source = false;
    let mut in_target = false;
    let mut current_source = String::new();
    let mut current_target = String::new();
    let mut document = DocumentValidator::new();

    loop {
        let decoder = reader.decoder();
        let event = reader
            .read_event()
            .map_err(|error| XcStringsError::XliffParse(error.to_string()))?;
        match event {
            Event::Start(ref e) => {
                let (namespace, _) = reader.resolver().resolve_element(e.name());
                let local_name = e.local_name();
                document.start(decoder, &namespace, e, reader.resolver())?;
                match local_name.as_ref() {
                    b"file" => {
                        if let Some(value) = normalized_attribute(decoder, e, b"target-language")? {
                            target_locale = value;
                        }
                    }
                    b"trans-unit" => {
                        current_id.clear();
                        current_source.clear();
                        current_target.clear();
                        if let Some(value) = normalized_attribute(decoder, e, b"id")? {
                            current_id = value;
                        }
                    }
                    b"source" => {
                        in_source = true;
                    }
                    b"target" => {
                        in_target = true;
                    }
                    _ => {}
                }
            }
            // Empty elements (self-closing) -- extract attributes but don't
            // set in_source/in_target since there is no text content or end tag.
            Event::Empty(ref e) => {
                let (namespace, _) = reader.resolver().resolve_element(e.name());
                let local_name = e.local_name();
                document.empty(decoder, &namespace, e, reader.resolver())?;
                if local_name.as_ref() == b"file"
                    && let Some(value) = normalized_attribute(decoder, e, b"target-language")?
                {
                    target_locale = value;
                }
            }
            Event::Text(ref e) => {
                let text = e
                    .decode()
                    .map_err(|err| XcStringsError::XliffParse(err.to_string()))?;
                document.text(&text)?;
                if in_source {
                    current_source.push_str(&text);
                } else if in_target {
                    current_target.push_str(&text);
                }
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
                if in_source {
                    current_source.push_str(&resolved);
                } else if in_target {
                    current_target.push_str(&resolved);
                }
            }
            Event::End(ref e) => {
                let (namespace, _) = reader.resolver().resolve_element(e.name());
                let local_name = e.local_name();
                document.end(decoder, &namespace, local_name.as_ref())?;
                match local_name.as_ref() {
                    b"source" => {
                        in_source = false;
                    }
                    b"target" => {
                        in_target = false;
                    }
                    b"trans-unit" if !current_id.is_empty() && !current_target.is_empty() => {
                        translations.push(CompletedTranslation {
                            key: current_id.clone(),
                            locale: target_locale.clone(),
                            value: current_target.clone(),
                            plural_forms: None,
                            substitution_name: None,
                        });
                    }
                    _ => {}
                }
            }
            Event::CData(_) => document.cdata()?,
            Event::Decl(_) => document.declaration()?,
            Event::DocType(_) => document.doctype()?,
            Event::Eof => {
                document.finish()?;
                break;
            }
            _ => {}
        }
    }

    if target_locale.is_empty() {
        return Err(XcStringsError::XliffParse(
            "missing target-language attribute in <file> element".into(),
        ));
    }

    Ok((target_locale, translations))
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
