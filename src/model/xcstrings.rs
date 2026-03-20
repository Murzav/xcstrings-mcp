use std::collections::BTreeMap;

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Map type for xcstrings string keys and localizations.
/// Uses IndexMap to preserve Xcode's insertion order (Finder-like sort in Xcode 16+).
/// Xcode uses `localizedStandardCompare` which is locale-dependent and cannot be
/// reproduced in pure Rust. IndexMap preserves whatever order Xcode wrote.
pub type OrderedMap<K, V> = IndexMap<K, V>;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct XcStringsFile {
    pub source_language: String,
    pub strings: OrderedMap<String, StringEntry>,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StringEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_state: Option<ExtractionState>,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub should_translate: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub localizations: Option<OrderedMap<String, Localization>>,
}

fn default_true() -> bool {
    true
}

fn is_true(v: &bool) -> bool {
    *v
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Localization {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub string_unit: Option<StringUnit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variations: Option<Variations>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub substitutions: Option<BTreeMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StringUnit {
    pub state: TranslationState,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Variations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plural: Option<BTreeMap<String, PluralVariation>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<BTreeMap<DeviceCategory, DeviceVariation>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluralVariation {
    pub string_unit: StringUnit,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeviceVariation {
    pub string_unit: StringUnit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionState {
    Manual,
    ExtractedWithValue,
    Stale,
    Migrated,
    #[serde(untagged)]
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TranslationState {
    New,
    Translated,
    NeedsReview,
    Stale,
    #[serde(untagged)]
    Unknown(String),
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum DeviceCategory {
    #[serde(rename = "iphone")]
    IPhone,
    #[serde(rename = "ipad")]
    IPad,
    #[serde(rename = "mac")]
    Mac,
    #[serde(rename = "applewatch")]
    AppleWatch,
    #[serde(rename = "appletv")]
    AppleTv,
    #[serde(untagged)]
    Unknown(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_state_known_values_roundtrip() {
        let variants = [
            (ExtractionState::Manual, "\"manual\""),
            (
                ExtractionState::ExtractedWithValue,
                "\"extracted_with_value\"",
            ),
            (ExtractionState::Stale, "\"stale\""),
            (ExtractionState::Migrated, "\"migrated\""),
        ];
        for (variant, expected_json) in &variants {
            let json = serde_json::to_string(variant).unwrap();
            assert_eq!(&json, expected_json);
            let deserialized: ExtractionState = serde_json::from_str(&json).unwrap();
            assert_eq!(&deserialized, variant);
        }
    }

    #[test]
    fn extraction_state_unknown_value_roundtrip() {
        let json = "\"some_future_state\"";
        let state: ExtractionState = serde_json::from_str(json).unwrap();
        assert_eq!(
            state,
            ExtractionState::Unknown("some_future_state".to_string())
        );
        let serialized = serde_json::to_string(&state).unwrap();
        assert_eq!(serialized, json);
    }

    #[test]
    fn translation_state_known_values_roundtrip() {
        let variants = [
            (TranslationState::New, "\"new\""),
            (TranslationState::Translated, "\"translated\""),
            (TranslationState::NeedsReview, "\"needs_review\""),
            (TranslationState::Stale, "\"stale\""),
        ];
        for (variant, expected_json) in &variants {
            let json = serde_json::to_string(variant).unwrap();
            assert_eq!(&json, expected_json);
            let deserialized: TranslationState = serde_json::from_str(&json).unwrap();
            assert_eq!(&deserialized, variant);
        }
    }

    #[test]
    fn translation_state_unknown_roundtrip() {
        let json = "\"verified\"";
        let state: TranslationState = serde_json::from_str(json).unwrap();
        assert_eq!(state, TranslationState::Unknown("verified".to_string()));
        let serialized = serde_json::to_string(&state).unwrap();
        assert_eq!(serialized, json);
    }

    #[test]
    fn parse_simple_fixture() {
        let content = include_str!("../../tests/fixtures/simple.xcstrings");
        let file: XcStringsFile = serde_json::from_str(content).unwrap();

        assert_eq!(file.source_language, "en");
        assert_eq!(file.version, "1.0");
        assert_eq!(file.strings.len(), 2);

        let greeting = &file.strings["greeting"];
        assert_eq!(greeting.extraction_state, Some(ExtractionState::Manual));

        let localizations = greeting.localizations.as_ref().unwrap();
        assert_eq!(localizations.len(), 2);

        let en = localizations["en"].string_unit.as_ref().unwrap();
        assert_eq!(en.state, TranslationState::Translated);
        assert_eq!(en.value, "Hello");

        let uk = localizations["uk"].string_unit.as_ref().unwrap();
        assert_eq!(uk.state, TranslationState::Translated);
        assert_eq!(uk.value, "Привіт");
    }
}
