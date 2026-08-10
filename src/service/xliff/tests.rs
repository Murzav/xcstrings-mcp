use super::*;
use crate::service::parser;

const FIXTURE: &str = include_str!("../../../tests/fixtures/simple.xcstrings");

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
