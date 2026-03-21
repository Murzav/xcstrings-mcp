use std::io::Cursor;

use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};

use crate::error::XcStringsError;
use crate::model::translation::CompletedTranslation;
use crate::model::xcstrings::{TranslationState, XcStringsFile};

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
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xliff_content);

    let mut target_locale = String::new();
    let mut translations = Vec::new();

    let mut current_id = String::new();
    let mut in_source = false;
    let mut in_target = false;
    let mut current_source = String::new();
    let mut current_target = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"file" => {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"target-language" {
                            target_locale = String::from_utf8_lossy(&attr.value).to_string();
                        }
                    }
                }
                b"trans-unit" => {
                    current_id.clear();
                    current_source.clear();
                    current_target.clear();
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"id" {
                            current_id = String::from_utf8_lossy(&attr.value).to_string();
                        }
                    }
                }
                b"source" => {
                    in_source = true;
                }
                b"target" => {
                    in_target = true;
                }
                _ => {}
            },
            // Empty elements (self-closing) -- extract attributes but don't
            // set in_source/in_target since there is no text content or end tag.
            Ok(Event::Empty(ref e)) => {
                if e.name().as_ref() == b"file" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"target-language" {
                            target_locale = String::from_utf8_lossy(&attr.value).to_string();
                        }
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                // Use unescape() to handle XML entities (&amp; -> &, etc.)
                let text = e
                    .unescape()
                    .map_err(|err| XcStringsError::XliffParse(err.to_string()))?;
                if in_source {
                    current_source.push_str(&text);
                } else if in_target {
                    current_target.push_str(&text);
                }
            }
            Ok(Event::End(ref e)) => match e.name().as_ref() {
                b"source" => {
                    in_source = false;
                }
                b"target" => {
                    in_target = false;
                }
                b"trans-unit" => {
                    if !current_id.is_empty() && !current_target.is_empty() {
                        translations.push(CompletedTranslation {
                            key: current_id.clone(),
                            locale: target_locale.clone(),
                            value: current_target.clone(),
                            plural_forms: None,
                            substitution_name: None,
                        });
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(XcStringsError::XliffParse(e.to_string())),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::parser;

    const FIXTURE: &str = include_str!("../../tests/fixtures/simple.xcstrings");

    fn parsed_fixture() -> XcStringsFile {
        parser::parse(FIXTURE).unwrap()
    }

    #[test]
    fn export_produces_well_formed_xml() {
        let file = parsed_fixture();
        let (xml, _count) = export_xliff(&file, "uk", "Localizable.xcstrings", false).unwrap();

        // Should parse back without error
        let (locale, _translations) = import_xliff(&xml).unwrap();
        assert_eq!(locale, "uk");

        // Basic structure checks
        assert!(xml.contains("<xliff"));
        assert!(xml.contains("</xliff>"));
        assert!(xml.contains("target-language=\"uk\""));
    }

    #[test]
    fn export_import_roundtrip() {
        let file = parsed_fixture();
        let (xml, _) = export_xliff(&file, "uk", "test.xcstrings", false).unwrap();
        let (_locale, translations) = import_xliff(&xml).unwrap();

        // "greeting" has uk translation, "welcome_message" does not
        let greeting = translations.iter().find(|t| t.key == "greeting");
        assert!(greeting.is_some());
        assert_eq!(
            greeting.unwrap().value,
            "\u{041f}\u{0440}\u{0438}\u{0432}\u{0456}\u{0442}"
        );
        assert_eq!(greeting.unwrap().locale, "uk");
    }

    #[test]
    fn export_escapes_xml_special_chars() {
        let json = r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "html_key" : {
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "A & B < C > D"
          }
        }
      }
    }
  },
  "version" : "1.0"
}"#;
        let file = parser::parse(json).unwrap();
        let (xml, _) = export_xliff(&file, "de", "test.xcstrings", false).unwrap();

        assert!(xml.contains("A &amp; B &lt; C &gt; D"));
        // Must roundtrip correctly
        let (_locale, translations) = import_xliff(&xml).unwrap();
        // No translations because target is empty, but parsing succeeds
        assert!(translations.is_empty());
    }

    #[test]
    fn roundtrip_preserves_special_chars_in_target() {
        let json = r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "terms" : {
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "Terms & Conditions"
          }
        },
        "de" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "AGB & <Bedingungen>"
          }
        }
      }
    }
  },
  "version" : "1.0"
}"#;
        let file = parser::parse(json).unwrap();
        let (xml, count) = export_xliff(&file, "de", "test.xcstrings", false).unwrap();
        assert_eq!(count, 1);

        // XML must have escaped entities
        assert!(xml.contains("AGB &amp; &lt;Bedingungen&gt;"));

        // Import must unescape back to original
        let (_locale, translations) = import_xliff(&xml).unwrap();
        assert_eq!(translations.len(), 1);
        assert_eq!(translations[0].value, "AGB & <Bedingungen>");
    }

    #[test]
    fn export_untranslated_only_false_includes_all() {
        let file = parsed_fixture();
        let (xml, _) = export_xliff(&file, "uk", "test.xcstrings", false).unwrap();

        assert!(xml.contains("id=\"greeting\""));
        assert!(xml.contains("id=\"welcome_message\""));
    }

    #[test]
    fn export_untranslated_only_true_excludes_translated() {
        let file = parsed_fixture();
        let (xml, _) = export_xliff(&file, "uk", "test.xcstrings", true).unwrap();

        // greeting is translated to uk, should be excluded
        assert!(!xml.contains("id=\"greeting\""));
        // welcome_message is not translated to uk, should be included
        assert!(xml.contains("id=\"welcome_message\""));
    }

    #[test]
    fn import_empty_xliff_returns_zero_translations() {
        let xliff = r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">
  <file source-language="en" target-language="de" original="test.xcstrings" datatype="plaintext">
    <body>
    </body>
  </file>
</xliff>"#;

        let (locale, translations) = import_xliff(xliff).unwrap();
        assert_eq!(locale, "de");
        assert!(translations.is_empty());
    }

    #[test]
    fn import_missing_target_language_returns_error() {
        let xliff = r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">
  <file source-language="en" original="test.xcstrings" datatype="plaintext">
    <body>
    </body>
  </file>
</xliff>"#;

        let result = import_xliff(xliff);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing target-language"));
    }

    #[test]
    fn import_skips_empty_targets() {
        let xliff = r#"<?xml version="1.0" encoding="UTF-8"?>
<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2">
  <file source-language="en" target-language="de" original="test.xcstrings" datatype="plaintext">
    <body>
      <trans-unit id="key1">
        <source>Hello</source>
        <target state="new"></target>
      </trans-unit>
      <trans-unit id="key2">
        <source>World</source>
        <target state="translated">Welt</target>
      </trans-unit>
    </body>
  </file>
</xliff>"#;

        let (locale, translations) = import_xliff(xliff).unwrap();
        assert_eq!(locale, "de");
        assert_eq!(translations.len(), 1);
        assert_eq!(translations[0].key, "key2");
        assert_eq!(translations[0].value, "Welt");
    }

    #[test]
    fn export_comment_appears_as_note() {
        let json = r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "btn_ok" : {
      "comment" : "OK button label",
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "OK"
          }
        }
      }
    }
  },
  "version" : "1.0"
}"#;
        let file = parser::parse(json).unwrap();
        let (xml, _) = export_xliff(&file, "de", "test.xcstrings", false).unwrap();
        assert!(xml.contains("<note>OK button label</note>"));
    }
}
