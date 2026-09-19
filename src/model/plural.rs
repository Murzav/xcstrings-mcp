use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[path = "plural_categories_data.rs"]
mod categories_data;

/// CLDR cardinal categories for a known locale, including regional fallback.
///
/// These are completeness recommendations, not the minimum accepted by Xcode's
/// compiler. Unknown locales return an error instead of inventing plural rules.
pub fn plural_categories(
    locale: &str,
) -> Result<Vec<PluralCategory>, crate::error::XcStringsError> {
    let mut candidate = locale.replace('_', "-").to_ascii_lowercase();
    if !candidate.is_empty() {
        let tag = language_tags::LanguageTag::parse(&candidate).map_err(|_| {
            crate::error::XcStringsError::InvalidFormat(format!(
                "invalid locale identifier '{locale}'"
            ))
        })?;
        // Check syntax without rejecting future IANA registry additions.
        let mut variants = std::collections::HashSet::new();
        let mut extensions = std::collections::HashSet::new();
        if tag.variant_subtags().any(|part| !variants.insert(part))
            || tag
                .extension_subtags()
                .any(|(name, _)| !extensions.insert(name))
        {
            return Err(crate::error::XcStringsError::InvalidFormat(format!(
                "invalid locale identifier '{locale}'"
            )));
        }
    }
    loop {
        if let Some(categories) = categories_data::lookup(&candidate) {
            return Ok(categories.to_vec());
        }
        let Some(separator) = candidate.rfind('-') else {
            return Err(crate::error::XcStringsError::InvalidFormat(format!(
                "no CLDR cardinal plural rules for locale '{locale}'"
            )));
        };
        candidate.truncate(separator);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    Other,
}

impl PluralCategory {
    /// Returns the CLDR string representation of this plural category.
    pub fn as_str(&self) -> &'static str {
        match self {
            PluralCategory::Zero => "zero",
            PluralCategory::One => "one",
            PluralCategory::Two => "two",
            PluralCategory::Few => "few",
            PluralCategory::Many => "many",
            PluralCategory::Other => "other",
        }
    }
}

pub(crate) fn required_plural_forms(locale: &str) -> Vec<PluralCategory> {
    let lang = locale.split('-').next().unwrap_or(locale);
    match lang {
        // East Asian: Other only
        "ja" | "ko" | "zh" | "vi" | "th" | "ms" | "id" => vec![PluralCategory::Other],

        // Germanic/Romance/other two-form: One, Other
        "en" | "de" | "nl" | "sv" | "da" | "no" | "nb" | "nn" | "es" | "fr" | "it" | "pt"
        | "ca" | "gl" | "af" | "bg" | "el" | "fi" | "he" | "hi" | "hu" | "tr" | "ka" => {
            vec![PluralCategory::One, PluralCategory::Other]
        }

        // Slavic: One, Few, Many, Other
        "uk" | "pl" | "hr" | "sr" | "bs" | "be" => {
            vec![
                PluralCategory::One,
                PluralCategory::Few,
                PluralCategory::Many,
                PluralCategory::Other,
            ]
        }

        // Czech/Slovak: One, Few, Many, Other
        "cs" | "sk" => vec![
            PluralCategory::One,
            PluralCategory::Few,
            PluralCategory::Many,
            PluralCategory::Other,
        ],

        // Romanian: One, Few, Other
        "ro" => vec![
            PluralCategory::One,
            PluralCategory::Few,
            PluralCategory::Other,
        ],

        // Latvian: Zero, One, Other
        "lv" => vec![
            PluralCategory::Zero,
            PluralCategory::One,
            PluralCategory::Other,
        ],

        // Arabic: all 6
        "ar" => vec![
            PluralCategory::Zero,
            PluralCategory::One,
            PluralCategory::Two,
            PluralCategory::Few,
            PluralCategory::Many,
            PluralCategory::Other,
        ],

        // Welsh: all 6
        "cy" => vec![
            PluralCategory::Zero,
            PluralCategory::One,
            PluralCategory::Two,
            PluralCategory::Few,
            PluralCategory::Many,
            PluralCategory::Other,
        ],

        // Irish: One, Two, Few, Many, Other
        "ga" => vec![
            PluralCategory::One,
            PluralCategory::Two,
            PluralCategory::Few,
            PluralCategory::Many,
            PluralCategory::Other,
        ],

        // Lithuanian: One, Few, Many, Other
        "lt" => vec![
            PluralCategory::One,
            PluralCategory::Few,
            PluralCategory::Many,
            PluralCategory::Other,
        ],

        // Unknown: safe default
        _ => vec![PluralCategory::One, PluralCategory::Other],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_east_asian_all() {
        for locale in ["ja", "ko", "zh", "vi", "th", "ms", "id"] {
            assert_eq!(
                required_plural_forms(locale),
                vec![PluralCategory::Other],
                "failed for {locale}"
            );
        }
    }

    #[test]
    fn test_germanic_romance_all() {
        for locale in [
            "en", "de", "nl", "sv", "da", "no", "nb", "nn", "es", "fr", "it", "pt", "ca", "gl",
            "af", "bg", "el", "fi", "he", "hi", "hu", "tr", "ka",
        ] {
            assert_eq!(
                required_plural_forms(locale),
                vec![PluralCategory::One, PluralCategory::Other],
                "failed for {locale}"
            );
        }
    }

    #[test]
    fn test_slavic_all() {
        for locale in ["uk", "pl", "hr", "sr", "bs", "be"] {
            assert_eq!(
                required_plural_forms(locale),
                vec![
                    PluralCategory::One,
                    PluralCategory::Few,
                    PluralCategory::Many,
                    PluralCategory::Other,
                ],
                "failed for {locale}"
            );
        }
    }

    #[test]
    fn test_czech_slovak() {
        for locale in ["cs", "sk"] {
            assert_eq!(
                required_plural_forms(locale),
                vec![
                    PluralCategory::One,
                    PluralCategory::Few,
                    PluralCategory::Many,
                    PluralCategory::Other,
                ],
                "failed for {locale}"
            );
        }
    }

    #[test]
    fn test_romanian() {
        assert_eq!(
            required_plural_forms("ro"),
            vec![
                PluralCategory::One,
                PluralCategory::Few,
                PluralCategory::Other,
            ]
        );
    }

    #[test]
    fn test_latvian() {
        assert_eq!(
            required_plural_forms("lv"),
            vec![
                PluralCategory::Zero,
                PluralCategory::One,
                PluralCategory::Other,
            ]
        );
    }

    #[test]
    fn test_arabic() {
        assert_eq!(
            required_plural_forms("ar"),
            vec![
                PluralCategory::Zero,
                PluralCategory::One,
                PluralCategory::Two,
                PluralCategory::Few,
                PluralCategory::Many,
                PluralCategory::Other,
            ]
        );
    }

    #[test]
    fn test_welsh() {
        assert_eq!(
            required_plural_forms("cy"),
            vec![
                PluralCategory::Zero,
                PluralCategory::One,
                PluralCategory::Two,
                PluralCategory::Few,
                PluralCategory::Many,
                PluralCategory::Other,
            ]
        );
    }

    #[test]
    fn test_irish() {
        assert_eq!(
            required_plural_forms("ga"),
            vec![
                PluralCategory::One,
                PluralCategory::Two,
                PluralCategory::Few,
                PluralCategory::Many,
                PluralCategory::Other,
            ]
        );
    }

    #[test]
    fn test_lithuanian() {
        assert_eq!(
            required_plural_forms("lt"),
            vec![
                PluralCategory::One,
                PluralCategory::Few,
                PluralCategory::Many,
                PluralCategory::Other,
            ]
        );
    }

    #[test]
    fn test_locale_with_region() {
        assert_eq!(required_plural_forms("uk-UA"), required_plural_forms("uk"));
    }

    #[test]
    fn test_unknown_locale() {
        assert_eq!(
            required_plural_forms("xx"),
            vec![PluralCategory::One, PluralCategory::Other]
        );
    }
}
