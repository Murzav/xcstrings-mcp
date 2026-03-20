use crate::model::translation::ContextKey;
use crate::model::xcstrings::{TranslationState, XcStringsFile};

/// Count shared prefix segments between pre-split key segments and another key.
fn shared_prefix_length(key_segments: &[&str], other_key: &str) -> usize {
    let other_segments: Vec<&str> = other_key.split('.').collect();
    key_segments
        .iter()
        .zip(other_segments.iter())
        .take_while(|(a, b)| a == b)
        .count()
}

/// Get context keys for a specific key — nearby keys sharing a common prefix.
///
/// Returns up to `count` keys from the same file, sorted by longest shared
/// dot-separated prefix (descending), then alphabetically by key name.
pub fn get_context(file: &XcStringsFile, key: &str, locale: &str, count: usize) -> Vec<ContextKey> {
    if count == 0 || !file.strings.contains_key(key) {
        return Vec::new();
    }

    let count = count.min(50); // Cap at 50 to prevent unbounded allocations

    let key_segments: Vec<&str> = key.split('.').collect();

    // Score all other keys by shared prefix length.
    let mut scored: Vec<(usize, &str)> = file
        .strings
        .keys()
        .filter(|k| k.as_str() != key)
        .map(|k| (shared_prefix_length(&key_segments, k), k.as_str()))
        .collect();

    // Sort by score DESC, then key ASC.
    scored.sort_by(|(score_a, key_a), (score_b, key_b)| {
        score_b.cmp(score_a).then_with(|| key_a.cmp(key_b))
    });

    scored
        .into_iter()
        .take(count)
        .map(|(_, other_key)| {
            let entry = &file.strings[other_key];

            // Source text: from source language string_unit, fallback to key name.
            let source_text = entry
                .localizations
                .as_ref()
                .and_then(|locs| locs.get(&file.source_language))
                .and_then(|loc| loc.string_unit.as_ref())
                .map(|su| su.value.clone())
                .unwrap_or_else(|| other_key.to_string());

            // Translated text: from target locale if state == Translated.
            let translated_text = entry
                .localizations
                .as_ref()
                .and_then(|locs| locs.get(locale))
                .and_then(|loc| loc.string_unit.as_ref())
                .filter(|su| su.state == TranslationState::Translated)
                .map(|su| su.value.clone());

            ContextKey {
                key: other_key.to_string(),
                source_text,
                translated_text,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::*;
    use crate::model::xcstrings::{Localization, StringEntry, StringUnit, XcStringsFile};

    fn make_file(entries: Vec<(&str, StringEntry)>) -> XcStringsFile {
        XcStringsFile {
            source_language: "en".to_string(),
            strings: entries
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            version: "1.0".to_string(),
        }
    }

    fn simple_entry(source_value: &str) -> StringEntry {
        let mut localizations = IndexMap::new();
        localizations.insert(
            "en".to_string(),
            Localization {
                string_unit: Some(StringUnit {
                    state: TranslationState::Translated,
                    value: source_value.to_string(),
                }),
                variations: None,
                substitutions: None,
            },
        );
        StringEntry {
            extraction_state: None,
            should_translate: true,
            comment: None,
            localizations: Some(localizations),
        }
    }

    fn entry_with_translation(
        source_value: &str,
        locale: &str,
        translated_value: &str,
        state: TranslationState,
    ) -> StringEntry {
        let mut localizations = IndexMap::new();
        localizations.insert(
            "en".to_string(),
            Localization {
                string_unit: Some(StringUnit {
                    state: TranslationState::Translated,
                    value: source_value.to_string(),
                }),
                variations: None,
                substitutions: None,
            },
        );
        localizations.insert(
            locale.to_string(),
            Localization {
                string_unit: Some(StringUnit {
                    state,
                    value: translated_value.to_string(),
                }),
                variations: None,
                substitutions: None,
            },
        );
        StringEntry {
            extraction_state: None,
            should_translate: true,
            comment: None,
            localizations: Some(localizations),
        }
    }

    #[test]
    fn test_prefix_match_returns_closest() {
        let file = make_file(vec![
            (
                "settings.notifications.title",
                simple_entry("Notifications"),
            ),
            ("settings.notifications.body", simple_entry("Body")),
            ("settings.general.title", simple_entry("General")),
            ("login.title", simple_entry("Login")),
        ]);

        let result = get_context(&file, "settings.notifications.title", "de", 10);
        assert_eq!(result.len(), 3);
        // Highest prefix match first (2 segments shared)
        assert_eq!(result[0].key, "settings.notifications.body");
        // Then 1 segment shared
        assert_eq!(result[1].key, "settings.general.title");
        // Then 0 segments shared
        assert_eq!(result[2].key, "login.title");
    }

    #[test]
    fn test_no_prefix_match_alphabetical() {
        let file = make_file(vec![
            ("alpha", simple_entry("Alpha")),
            ("beta", simple_entry("Beta")),
            ("gamma", simple_entry("Gamma")),
            ("delta", simple_entry("Delta")),
        ]);

        let result = get_context(&file, "beta", "de", 10);
        assert_eq!(result.len(), 3);
        // All score 0 → alphabetical
        assert_eq!(result[0].key, "alpha");
        assert_eq!(result[1].key, "delta");
        assert_eq!(result[2].key, "gamma");
    }

    #[test]
    fn test_flat_keys_no_dots() {
        let file = make_file(vec![
            ("cancel", simple_entry("Cancel")),
            ("confirm", simple_entry("Confirm")),
            ("delete", simple_entry("Delete")),
        ]);

        let result = get_context(&file, "confirm", "de", 10);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].key, "cancel");
        assert_eq!(result[1].key, "delete");
    }

    #[test]
    fn test_existing_translations_included() {
        let file = make_file(vec![
            ("app.title", simple_entry("Title")),
            (
                "app.subtitle",
                entry_with_translation(
                    "Subtitle",
                    "de",
                    "Untertitel",
                    TranslationState::Translated,
                ),
            ),
            (
                "app.footer",
                entry_with_translation("Footer", "de", "Entwurf", TranslationState::NeedsReview),
            ),
        ]);

        let result = get_context(&file, "app.title", "de", 10);
        assert_eq!(result.len(), 2);

        // app.footer and app.subtitle both share 1 prefix segment, sorted alphabetically
        assert_eq!(result[0].key, "app.footer");
        assert!(result[0].translated_text.is_none()); // NeedsReview, not Translated

        assert_eq!(result[1].key, "app.subtitle");
        assert_eq!(result[1].translated_text.as_deref(), Some("Untertitel"));
    }

    #[test]
    fn test_count_limits_output() {
        let file = make_file(vec![
            ("a", simple_entry("A")),
            ("b", simple_entry("B")),
            ("c", simple_entry("C")),
            ("d", simple_entry("D")),
        ]);

        let result = get_context(&file, "a", "de", 2);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_key_not_in_file_returns_empty() {
        let file = make_file(vec![("existing", simple_entry("Existing"))]);

        let result = get_context(&file, "nonexistent", "de", 10);
        assert!(result.is_empty());
    }
}
