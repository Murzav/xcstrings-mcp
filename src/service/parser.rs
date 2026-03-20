use std::collections::BTreeSet;

use crate::error::XcStringsError;
use crate::model::translation::FileSummary;
use crate::model::xcstrings::XcStringsFile;

/// Parse an xcstrings JSON string into the typed model.
pub fn parse(content: &str) -> Result<XcStringsFile, XcStringsError> {
    let file: XcStringsFile =
        serde_json::from_str(content).map_err(|e| XcStringsError::JsonParse(e.to_string()))?;

    if file.source_language.is_empty() {
        return Err(XcStringsError::InvalidFormat(
            "sourceLanguage is empty".into(),
        ));
    }
    if file.version.is_empty() {
        return Err(XcStringsError::InvalidFormat("version is empty".into()));
    }

    Ok(file)
}

/// Build a summary of the parsed file.
pub fn summarize(file: &XcStringsFile) -> FileSummary {
    let total_keys = file.strings.len();
    let translatable_keys = file.strings.values().filter(|e| e.should_translate).count();

    let mut locale_set = BTreeSet::new();
    for entry in file.strings.values() {
        if let Some(localizations) = &entry.localizations {
            for locale in localizations.keys() {
                locale_set.insert(locale.clone());
            }
        }
    }

    let mut keys_by_state = std::collections::BTreeMap::new();
    for entry in file.strings.values() {
        if !entry.should_translate {
            continue;
        }
        if let Some(localizations) = &entry.localizations {
            for localization in localizations.values() {
                let state_name = if let Some(su) = &localization.string_unit {
                    state_to_string(&su.state)
                } else if localization.variations.is_some() {
                    "translated".to_string()
                } else {
                    "new".to_string()
                };
                *keys_by_state.entry(state_name).or_insert(0usize) += 1;
            }
        }
    }

    FileSummary {
        source_language: file.source_language.clone(),
        total_keys,
        translatable_keys,
        locales: locale_set.into_iter().collect(),
        keys_by_state,
    }
}

/// Serialize a TranslationState to its serde string representation.
#[allow(dead_code, reason = "used by summarize")]
fn state_to_string(state: &crate::model::xcstrings::TranslationState) -> String {
    // serde_json::to_string produces `"translated"` with quotes; strip them.
    let json = serde_json::to_string(state).unwrap_or_else(|_| "\"unknown\"".to_string());
    json.trim_matches('"').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::xcstrings::{ExtractionState, TranslationState};

    #[test]
    fn test_parse_valid() {
        let content = include_str!("../../tests/fixtures/simple.xcstrings");
        let file = parse(content).expect("should parse simple.xcstrings");

        assert_eq!(file.source_language, "en");
        assert_eq!(file.version, "1.0");
        assert_eq!(file.strings.len(), 2);

        let greeting = &file.strings["greeting"];
        assert_eq!(greeting.extraction_state, Some(ExtractionState::Manual));

        let locs = greeting.localizations.as_ref().expect("has localizations");
        let en = locs["en"].string_unit.as_ref().expect("has en string_unit");
        assert_eq!(en.state, TranslationState::Translated);
        assert_eq!(en.value, "Hello");
    }

    #[test]
    fn test_parse_invalid_json() {
        let result = parse("not json at all");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, XcStringsError::JsonParse(_)));
    }

    #[test]
    fn test_parse_empty_source_language() {
        let json = r#"{"sourceLanguage":"","strings":{},"version":"1.0"}"#;
        let result = parse(json);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            XcStringsError::InvalidFormat(_)
        ));
    }

    #[test]
    fn test_summarize() {
        let content = include_str!("../../tests/fixtures/simple.xcstrings");
        let file = parse(content).unwrap();
        let summary = summarize(&file);

        assert_eq!(summary.source_language, "en");
        assert_eq!(summary.total_keys, 2);
        assert_eq!(summary.translatable_keys, 2);
        assert!(summary.locales.contains(&"en".to_string()));
        assert!(summary.locales.contains(&"uk".to_string()));
        // greeting has en(translated) + uk(translated), welcome_message has en(translated)
        assert_eq!(summary.keys_by_state.get("translated"), Some(&3));
    }

    #[test]
    fn test_summarize_with_should_not_translate() {
        let content = include_str!("../../tests/fixtures/should_not_translate.xcstrings");
        let file = parse(content).unwrap();
        let summary = summarize(&file);

        assert_eq!(summary.total_keys, 2);
        // CFBundleName has shouldTranslate=false
        assert_eq!(summary.translatable_keys, 1);
    }

    #[test]
    fn test_parse_unknown_enum_preserved() {
        let json = r#"{
            "sourceLanguage": "en",
            "strings": {
                "test": {
                    "extractionState": "future_state_v99"
                }
            },
            "version": "1.0"
        }"#;
        let file = parse(json).unwrap();
        let entry = &file.strings["test"];
        assert_eq!(
            entry.extraction_state,
            Some(ExtractionState::Unknown("future_state_v99".to_string()))
        );
    }
}
