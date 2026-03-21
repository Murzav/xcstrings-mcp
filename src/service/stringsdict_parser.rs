use std::collections::BTreeMap;

use indexmap::IndexMap;
use quick_xml::Reader;
use quick_xml::events::Event;

use crate::error::XcStringsError;

/// A single entry from a `.stringsdict` file, representing a pluralized string.
#[derive(Debug)]
pub struct StringsdictEntry {
    pub key: String,
    /// The `NSStringLocalizedFormatKey` value, e.g. `%#@varname@`.
    pub format_key: String,
    /// Plural variables referenced by the format key.
    pub variables: IndexMap<String, PluralVariable>,
}

/// A plural variable within a stringsdict entry.
#[derive(Debug)]
pub struct PluralVariable {
    /// The format specifier from `NSStringFormatValueTypeKey`, e.g. "d", "lld", "f", "@".
    pub format_specifier: String,
    /// CLDR plural forms: "zero", "one", "two", "few", "many", "other".
    pub forms: BTreeMap<String, String>,
}

/// Result of parsing a `.stringsdict` file, including any skipped entries.
#[derive(Debug)]
pub struct ParsedStringsdict {
    pub entries: Vec<StringsdictEntry>,
    /// Keys that were skipped due to unsupported rule types
    /// (e.g., `NSStringDeviceSpecificRuleType`, `NSStringVariableWidthRuleType`).
    pub skipped_keys: Vec<String>,
}

const PLURAL_FORMS: &[&str] = &["zero", "one", "two", "few", "many", "other"];

/// Parse a `.stringsdict` XML plist file into plural entries and skipped key names.
///
/// Entries using unsupported rule types (`NSStringDeviceSpecificRuleType`,
/// `NSStringVariableWidthRuleType`) are collected in `ParsedStringsdict::skipped_keys`.
pub fn parse_stringsdict(content: &str) -> Result<ParsedStringsdict, XcStringsError> {
    let mut reader = Reader::from_str(content);

    // Navigate to root <dict> inside <plist>
    let mut found_plist = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                if e.name().as_ref() == b"plist" {
                    found_plist = true;
                } else if found_plist && e.name().as_ref() == b"dict" {
                    break;
                }
            }
            Ok(Event::Eof) => {
                return Err(XcStringsError::StringsdictParse(
                    "unexpected EOF before root <dict>".into(),
                ));
            }
            Err(e) => return Err(XcStringsError::StringsdictParse(e.to_string())),
            _ => {}
        }
    }

    // Now inside root <dict>. Parse top-level key/dict pairs.
    let mut entries = Vec::new();
    let mut skipped_keys = Vec::new();
    loop {
        match read_next_significant_event(&mut reader)? {
            SignificantEvent::Key(entry_key) => {
                // Expect a <dict> for this entry
                skip_to_start_tag(&mut reader, b"dict")?;
                if let Some(entry) = parse_entry(&mut reader, &entry_key)? {
                    entries.push(entry);
                } else {
                    skipped_keys.push(entry_key);
                }
            }
            SignificantEvent::EndTag => break, // </dict> — end of root dict
            SignificantEvent::Eof => break,
        }
    }

    Ok(ParsedStringsdict {
        entries,
        skipped_keys,
    })
}

/// Events we care about when iterating dict contents.
enum SignificantEvent {
    Key(String),
    EndTag,
    Eof,
}

fn read_next_significant_event(
    reader: &mut Reader<&[u8]>,
) -> Result<SignificantEvent, XcStringsError> {
    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"key" => {
                let text = read_text_content(reader)?;
                return Ok(SignificantEvent::Key(text));
            }
            Ok(Event::End(_)) => return Ok(SignificantEvent::EndTag),
            Ok(Event::Eof) => return Ok(SignificantEvent::Eof),
            Err(e) => return Err(XcStringsError::StringsdictParse(e.to_string())),
            _ => {}
        }
    }
}

/// Read text content until the closing tag.
fn read_text_content(reader: &mut Reader<&[u8]>) -> Result<String, XcStringsError> {
    let mut text = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Text(ref e)) => {
                let unescaped = e
                    .unescape()
                    .map_err(|err| XcStringsError::StringsdictParse(err.to_string()))?;
                text.push_str(&unescaped);
            }
            Ok(Event::CData(ref e)) => {
                text.push_str(&String::from_utf8_lossy(e.as_ref()));
            }
            Ok(Event::End(_)) => return Ok(text),
            Ok(Event::Eof) => {
                return Err(XcStringsError::StringsdictParse(
                    "unexpected EOF in text content".into(),
                ));
            }
            Err(e) => return Err(XcStringsError::StringsdictParse(e.to_string())),
            _ => {}
        }
    }
}

/// Skip events until we find a `<start>` tag with the given name.
fn skip_to_start_tag(reader: &mut Reader<&[u8]>, tag_name: &[u8]) -> Result<(), XcStringsError> {
    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) if e.name().as_ref() == tag_name => return Ok(()),
            Ok(Event::Eof) => {
                return Err(XcStringsError::StringsdictParse(format!(
                    "unexpected EOF waiting for <{}>",
                    String::from_utf8_lossy(tag_name)
                )));
            }
            Err(e) => return Err(XcStringsError::StringsdictParse(e.to_string())),
            _ => {}
        }
    }
}

/// Parse a single entry dict. Returns `None` if the entry should be skipped
/// (e.g., contains only unsupported rule types).
fn parse_entry(
    reader: &mut Reader<&[u8]>,
    key: &str,
) -> Result<Option<StringsdictEntry>, XcStringsError> {
    let mut format_key = String::new();
    let mut variables = IndexMap::new();
    let mut has_plural_variable = false;

    // Read key/value pairs inside the entry dict
    loop {
        match read_next_significant_event(reader)? {
            SignificantEvent::Key(k) if k == "NSStringLocalizedFormatKey" => {
                // Next element should be <string>
                skip_to_start_tag(reader, b"string")?;
                format_key = read_text_content(reader)?;
            }
            SignificantEvent::Key(var_name) => {
                // Should be a variable dict
                skip_to_start_tag(reader, b"dict")?;
                if let Some(var) = parse_variable_dict(reader)? {
                    has_plural_variable = true;
                    variables.insert(var_name, var);
                }
            }
            SignificantEvent::EndTag => break, // </dict>
            SignificantEvent::Eof => {
                return Err(XcStringsError::StringsdictParse(
                    "unexpected EOF inside entry dict".into(),
                ));
            }
        }
    }

    if !has_plural_variable {
        // Entry has no plural variables (all were unsupported types) — skip
        return Ok(None);
    }

    if format_key.is_empty() {
        return Err(XcStringsError::StringsdictParse(format!(
            "entry '{key}' missing NSStringLocalizedFormatKey"
        )));
    }

    Ok(Some(StringsdictEntry {
        key: key.to_owned(),
        format_key,
        variables,
    }))
}

/// Parse a variable dict (containing NSStringFormatSpecTypeKey, NSStringFormatValueTypeKey,
/// and plural forms). Returns `None` for unsupported rule types.
fn parse_variable_dict(
    reader: &mut Reader<&[u8]>,
) -> Result<Option<PluralVariable>, XcStringsError> {
    let mut spec_type = String::new();
    let mut format_specifier = String::new();
    let mut forms = BTreeMap::new();

    loop {
        match read_next_significant_event(reader)? {
            SignificantEvent::Key(k) => {
                // All values here are <string> elements
                skip_to_start_tag(reader, b"string")?;
                let value = read_text_content(reader)?;

                match k.as_str() {
                    "NSStringFormatSpecTypeKey" => spec_type = value,
                    "NSStringFormatValueTypeKey" => format_specifier = value,
                    _ if PLURAL_FORMS.contains(&k.as_str()) => {
                        forms.insert(k, value);
                    }
                    _ => {} // ignore unknown keys
                }
            }
            SignificantEvent::EndTag => break, // </dict>
            SignificantEvent::Eof => {
                return Err(XcStringsError::StringsdictParse(
                    "unexpected EOF inside variable dict".into(),
                ));
            }
        }
    }

    if spec_type != "NSStringPluralRuleType" {
        return Ok(None);
    }

    if format_specifier.is_empty() {
        return Err(XcStringsError::StringsdictParse(
            "plural variable missing NSStringFormatValueTypeKey".into(),
        ));
    }

    if !forms.contains_key("other") {
        return Err(XcStringsError::StringsdictParse(
            "plural variable missing required 'other' form".into(),
        ));
    }

    Ok(Some(PluralVariable {
        format_specifier,
        forms,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_content() -> &'static str {
        include_str!("../../tests/fixtures/en.lproj/Localizable.stringsdict")
    }

    #[test]
    fn simple_single_variable_plural() {
        let parsed = parse_stringsdict(fixture_content()).expect("should parse");
        let entry = parsed
            .entries
            .iter()
            .find(|e| e.key == "items_count")
            .unwrap();

        assert_eq!(entry.format_key, "%#@items@");
        assert_eq!(entry.variables.len(), 1);

        let var = &entry.variables["items"];
        assert_eq!(var.format_specifier, "lld");
        assert_eq!(var.forms["one"], "%lld item");
        assert_eq!(var.forms["other"], "%lld items");
    }

    #[test]
    fn multiple_plural_categories() {
        let parsed = parse_stringsdict(fixture_content()).expect("should parse");
        let entry = parsed
            .entries
            .iter()
            .find(|e| e.key == "messages_remaining")
            .unwrap();

        let var = &entry.variables["count"];
        assert_eq!(var.forms.len(), 3);
        assert!(var.forms.contains_key("zero"));
        assert!(var.forms.contains_key("one"));
        assert!(var.forms.contains_key("other"));
        assert_eq!(var.forms["zero"], "No messages remaining");
    }

    #[test]
    fn multiple_variables_in_one_entry() {
        let parsed = parse_stringsdict(fixture_content()).expect("should parse");
        let entry = parsed
            .entries
            .iter()
            .find(|e| e.key == "photos_in_albums")
            .unwrap();

        assert_eq!(entry.format_key, "%1$#@photos@ in %2$#@albums@");
        assert_eq!(entry.variables.len(), 2);
        assert!(entry.variables.contains_key("photos"));
        assert!(entry.variables.contains_key("albums"));

        assert_eq!(entry.variables["photos"].forms["one"], "%lld photo");
        assert_eq!(entry.variables["albums"].forms["other"], "%lld albums");
    }

    #[test]
    fn missing_other_category_is_error() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>bad_entry</key>
    <dict>
        <key>NSStringLocalizedFormatKey</key>
        <string>%#@count@</string>
        <key>count</key>
        <dict>
            <key>NSStringFormatSpecTypeKey</key>
            <string>NSStringPluralRuleType</string>
            <key>NSStringFormatValueTypeKey</key>
            <string>d</string>
            <key>one</key>
            <string>one thing</string>
        </dict>
    </dict>
</dict>
</plist>"#;

        let err = parse_stringsdict(xml).unwrap_err();
        assert!(err.to_string().contains("other"));
    }

    #[test]
    fn unsupported_rule_type_is_skipped() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>device_entry</key>
    <dict>
        <key>NSStringLocalizedFormatKey</key>
        <string>%#@device@</string>
        <key>device</key>
        <dict>
            <key>NSStringFormatSpecTypeKey</key>
            <string>NSStringDeviceSpecificRuleType</string>
            <key>iphone</key>
            <string>iPhone text</string>
            <key>ipad</key>
            <string>iPad text</string>
        </dict>
    </dict>
</dict>
</plist>"#;

        let parsed = parse_stringsdict(xml).expect("should parse without error");
        assert!(
            parsed.entries.is_empty(),
            "device-specific entries should be skipped"
        );
        assert_eq!(
            parsed.skipped_keys,
            vec!["device_entry"],
            "skipped key should be reported"
        );
    }

    #[test]
    fn empty_stringsdict() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
</dict>
</plist>"#;

        let parsed = parse_stringsdict(xml).expect("should parse");
        assert!(parsed.entries.is_empty());
    }

    #[test]
    fn invalid_xml_is_error() {
        let result = parse_stringsdict("this is not xml at all < >");
        assert!(result.is_err());
    }

    #[test]
    fn format_specifier_preservation() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>test_specifiers</key>
    <dict>
        <key>NSStringLocalizedFormatKey</key>
        <string>%#@count@</string>
        <key>count</key>
        <dict>
            <key>NSStringFormatSpecTypeKey</key>
            <string>NSStringPluralRuleType</string>
            <key>NSStringFormatValueTypeKey</key>
            <string>@</string>
            <key>other</key>
            <string>%@ things</string>
        </dict>
    </dict>
</dict>
</plist>"#;

        let parsed = parse_stringsdict(xml).expect("should parse");
        assert_eq!(parsed.entries[0].variables["count"].format_specifier, "@");
    }

    #[test]
    fn empty_format_value_type_key_is_error() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>test</key>
    <dict>
        <key>NSStringLocalizedFormatKey</key>
        <string>%#@count@</string>
        <key>count</key>
        <dict>
            <key>NSStringFormatSpecTypeKey</key>
            <string>NSStringPluralRuleType</string>
            <key>NSStringFormatValueTypeKey</key>
            <string></string>
            <key>other</key>
            <string>%d things</string>
        </dict>
    </dict>
</dict>
</plist>"#;

        let err = parse_stringsdict(xml).unwrap_err();
        assert!(
            err.to_string().contains("NSStringFormatValueTypeKey"),
            "error should mention missing format value type key: {err}"
        );
    }

    #[test]
    fn cdata_in_text_content() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>cdata_test</key>
    <dict>
        <key>NSStringLocalizedFormatKey</key>
        <string><![CDATA[%#@count@]]></string>
        <key>count</key>
        <dict>
            <key>NSStringFormatSpecTypeKey</key>
            <string>NSStringPluralRuleType</string>
            <key>NSStringFormatValueTypeKey</key>
            <string>d</string>
            <key>other</key>
            <string><![CDATA[%d items & more]]></string>
        </dict>
    </dict>
</dict>
</plist>"#;

        let parsed = parse_stringsdict(xml).expect("should parse CDATA");
        assert_eq!(parsed.entries[0].format_key, "%#@count@");
        assert_eq!(
            parsed.entries[0].variables["count"].forms["other"],
            "%d items & more"
        );
    }
}
