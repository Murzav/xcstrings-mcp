pub mod coverage;
pub mod extractor;
pub mod file_validator;
pub mod formatter;
pub mod locale;
pub mod merger;
pub mod parser;
pub mod validator;

use crate::model::xcstrings::{StringEntry, TranslationState};

/// Check if an entry has a translated localization for the given locale.
/// A key is considered translated if it has:
/// - a string_unit with state == Translated, OR
/// - any variations present (plural/device)
pub(crate) fn is_translated_for(entry: &StringEntry, locale: &str) -> bool {
    let Some(locs) = &entry.localizations else {
        return false;
    };
    let Some(loc) = locs.get(locale) else {
        return false;
    };
    if loc.variations.is_some() {
        return true;
    }
    if let Some(su) = &loc.string_unit {
        return su.state == TranslationState::Translated;
    }
    false
}
