use crate::error::XcStringsError;
use crate::model::xcstrings::XcStringsFile;

/// Reformats JSON colon spacing from serde_json's `"key": value`
/// to Xcode's `"key" : value` style, only for structural separators
/// (colons outside JSON string literals).
pub fn fixup_colon_spacing(json: &str) -> String {
    let mut result = String::with_capacity(json.len() + json.len() / 10);
    let mut in_string = false;
    let mut escape_next = false;

    for ch in json.chars() {
        if escape_next {
            result.push(ch);
            escape_next = false;
            continue;
        }

        match ch {
            '\\' if in_string => {
                result.push(ch);
                escape_next = true;
            }
            '"' => {
                in_string = !in_string;
                result.push(ch);
            }
            ':' if !in_string => {
                // serde_json outputs `"key": value` — insert space before colon
                // to produce Xcode-style `"key" : value`
                result.push(' ');
                result.push(':');
            }
            _ => {
                result.push(ch);
            }
        }
    }

    result
}

/// Serializes an `XcStringsFile` to Xcode-compatible JSON with
/// `" : "` colon spacing and a trailing newline.
pub fn format_xcstrings(file: &XcStringsFile) -> Result<String, XcStringsError> {
    let json = serde_json::to_string_pretty(file)?;
    let mut formatted = fixup_colon_spacing(&json);
    if !formatted.ends_with('\n') {
        formatted.push('\n');
    }
    Ok(formatted)
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::*;

    #[test]
    fn basic_key_value() {
        let input = r#"  "key": "value""#;
        let expected = r#"  "key" : "value""#;
        assert_eq!(fixup_colon_spacing(input), expected);
    }

    #[test]
    fn url_in_value_preserved() {
        let input = r#"  "url": "https://x.com""#;
        let expected = r#"  "url" : "https://x.com""#;
        assert_eq!(fixup_colon_spacing(input), expected);
    }

    #[test]
    fn escaped_quotes() {
        let input = r#"  "msg": "he said \"hi: there\"""#;
        let expected = r#"  "msg" : "he said \"hi: there\"""#;
        assert_eq!(fixup_colon_spacing(input), expected);
    }

    #[test]
    fn empty_string_value() {
        let input = r#"  "key": """#;
        let expected = r#"  "key" : """#;
        assert_eq!(fixup_colon_spacing(input), expected);
    }

    #[test]
    fn numeric_value() {
        let input = r#"  "key": 42"#;
        let expected = r#"  "key" : 42"#;
        assert_eq!(fixup_colon_spacing(input), expected);
    }

    #[test]
    fn boolean_value() {
        let input = r#"  "key": true"#;
        let expected = r#"  "key" : true"#;
        assert_eq!(fixup_colon_spacing(input), expected);
    }

    #[test]
    fn null_value() {
        let input = r#"  "key": null"#;
        let expected = r#"  "key" : null"#;
        assert_eq!(fixup_colon_spacing(input), expected);
    }

    #[test]
    fn nested_object() {
        let input = "{\n  \"key\": {\n    \"a\": \"b\"\n  }\n}";
        let expected = "{\n  \"key\" : {\n    \"a\" : \"b\"\n  }\n}";
        assert_eq!(fixup_colon_spacing(input), expected);
    }

    #[test]
    fn unicode_in_value() {
        let input = "  \"key\": \"привіт: світ\"";
        let expected = "  \"key\" : \"привіт: світ\"";
        assert_eq!(fixup_colon_spacing(input), expected);
    }

    #[test]
    fn backslash_before_quote() {
        // \\\" in raw string = literal \\" in the JSON text
        // The \\ is an escaped backslash, \" is an escaped quote — string continues
        let input = r#"  "key": "path\\\"file""#;
        let expected = r#"  "key" : "path\\\"file""#;
        assert_eq!(fixup_colon_spacing(input), expected);
    }

    #[test]
    fn double_backslash_ends_string() {
        // \\\\" in raw string = literal \\" in text — two escaped backslashes,
        // then the quote closes the string. Next colon is a separator.
        let input = r#"  "key": "trailing\\", "b": 1"#;
        let expected = r#"  "key" : "trailing\\", "b" : 1"#;
        assert_eq!(fixup_colon_spacing(input), expected);
    }

    #[test]
    fn full_xcstrings_roundtrip() {
        let input = r#"{
  "sourceLanguage": "en",
  "strings": {
    "hello": {
      "localizations": {
        "en": {
          "stringUnit": {
            "state": "translated",
            "value": "Hello"
          }
        }
      }
    }
  },
  "version": "1.0"
}"#;
        let result = fixup_colon_spacing(input);
        assert!(result.contains("\"sourceLanguage\" : \"en\""));
        assert!(result.contains("\"state\" : \"translated\""));
        assert!(result.contains("\"value\" : \"Hello\""));
        // Colons inside string values should be untouched
        assert!(!result.contains("\" : 0\""));
    }

    #[test]
    fn no_strings_passthrough() {
        let input = "{ }";
        assert_eq!(fixup_colon_spacing(input), "{ }");
    }

    #[test]
    fn format_xcstrings_trailing_newline() {
        let file = XcStringsFile {
            source_language: "en".to_string(),
            strings: IndexMap::new(),
            version: "1.0".to_string(),
        };
        let result = format_xcstrings(&file).unwrap();
        assert!(result.ends_with('\n'));
        assert!(result.contains("\"sourceLanguage\" : \"en\""));
    }
}
