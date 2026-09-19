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
