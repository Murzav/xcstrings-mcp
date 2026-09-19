use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod layout;
pub mod paths;
pub use layout::ObjectLayout;
use layout::catalog_object;

/// Semantic maps retain the catalog's insertion order.
pub type OrderedMap<K, V> = IndexMap<K, V>;

catalog_object!(XcStringsFile {
    source_language: String = String::new() => "sourceLanguage", required;
    strings: OrderedMap<String, StringEntry> = OrderedMap::new() => "strings", required;
    version: String = "1.0".into() => "version", required;
});

fn default_should_translate() -> bool {
    true
}

catalog_object!(StringEntry {
    extraction_state: Option<ExtractionState> = None => "extractionState", optional;
    #[schemars(default = "default_should_translate")]
    should_translate: bool = true => "shouldTranslate", default_true;
    comment: Option<String> = None => "comment", optional;
    localizations: Option<OrderedMap<String, Localization>> = None => "localizations", optional;
});

catalog_object!(Localization {
    string_unit: Option<StringUnit> = None => "stringUnit", optional;
    variations: Option<Variations> = None => "variations", optional;
    substitutions: Option<OrderedMap<String, Substitution>> = None => "substitutions", optional;
});

catalog_object!(StringUnit {
    state: TranslationState = TranslationState::New => "state", required;
    value: String = String::new() => "value", required;
});

catalog_object!(Variations {
    plural: Option<OrderedMap<String, Localization>> = None => "plural", optional;
    device: Option<OrderedMap<DeviceCategory, Localization>> = None => "device", optional;
});

catalog_object!(Substitution {
    arg_num: Option<u32> = None => "argNum", optional;
    format_specifier: Option<String> = None => "formatSpecifier", optional;
    variations: Option<Variations> = None => "variations", optional;
});

/// Branches can contain another variation axis, substitutions, or a string unit.
pub type PluralVariation = Localization;
pub type DeviceVariation = Localization;

impl XcStringsFile {
    pub fn new(source_language: impl Into<String>) -> Self {
        Self {
            source_language: source_language.into(),
            ..Self::default()
        }
    }
}

impl StringUnit {
    pub fn new(state: TranslationState, value: impl Into<String>) -> Self {
        Self {
            state,
            value: value.into(),
            ..Self::default()
        }
    }

    /// Update only the translation payload, retaining future metadata and order.
    pub fn set_translation(&mut self, state: TranslationState, value: impl Into<String>) {
        self.state = state;
        self.value = value.into();
    }
}

impl Localization {
    pub fn with_unit(unit: StringUnit) -> Self {
        Self {
            string_unit: Some(unit),
            ..Self::default()
        }
    }

    pub fn set_translation(&mut self, state: TranslationState, value: impl Into<String>) {
        let unit = self.string_unit.get_or_insert_with(StringUnit::default);
        unit.set_translation(state, value);
    }
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
    MachineTranslated,
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
    #[serde(rename = "applevision")]
    AppleVision,
    #[serde(rename = "other")]
    Other,
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
