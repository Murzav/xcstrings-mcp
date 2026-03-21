use crate::model::translation::{DiffReport, ModifiedKey};
use crate::model::xcstrings::{StringEntry, XcStringsFile};

/// Compare two versions of an `XcStringsFile` and return the differences.
/// `old` is typically the cached version, `new` is the freshly-read version from disk.
///
/// Compares structural changes (added/removed keys) and source language text
/// changes. Translation changes in non-source locales are NOT detected — this
/// is intentional, as the primary use case is detecting source content changes
/// that require re-translation.
pub fn compute_diff(old: &XcStringsFile, new: &XcStringsFile) -> DiffReport {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut modified = Vec::new();

    // Keys in new but not in old -> added
    for key in new.strings.keys() {
        if !old.strings.contains_key(key) {
            added.push(key.clone());
        }
    }

    // Keys in old but not in new -> removed
    for key in old.strings.keys() {
        if !new.strings.contains_key(key) {
            removed.push(key.clone());
        }
    }

    // Keys in both -> check if source text changed
    let source_lang = &old.source_language;
    for key in old.strings.keys() {
        if let Some(new_entry) = new.strings.get(key) {
            let old_entry = &old.strings[key];
            let old_text = get_source_text(old_entry, source_lang);
            let new_text = get_source_text(new_entry, &new.source_language);
            if old_text != new_text {
                modified.push(ModifiedKey {
                    key: key.clone(),
                    old_value: old_text,
                    new_value: new_text,
                });
            }
        }
    }

    DiffReport {
        added,
        removed,
        modified,
    }
}

fn get_source_text(entry: &StringEntry, source_lang: &str) -> String {
    entry
        .localizations
        .as_ref()
        .and_then(|locs| locs.get(source_lang))
        .and_then(|loc| loc.string_unit.as_ref())
        .map(|su| su.value.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::xcstrings::{
        Localization, OrderedMap, StringEntry, StringUnit, TranslationState, XcStringsFile,
    };

    fn make_file(entries: Vec<(&str, &str)>) -> XcStringsFile {
        let mut strings = OrderedMap::new();
        for (key, value) in entries {
            let mut locs = OrderedMap::new();
            locs.insert(
                "en".to_string(),
                Localization {
                    string_unit: Some(StringUnit {
                        state: TranslationState::Translated,
                        value: value.to_string(),
                    }),
                    variations: None,
                    substitutions: None,
                },
            );
            strings.insert(
                key.to_string(),
                StringEntry {
                    extraction_state: None,
                    should_translate: true,
                    comment: None,
                    localizations: Some(locs),
                },
            );
        }
        XcStringsFile {
            source_language: "en".to_string(),
            strings,
            version: "1.0".to_string(),
        }
    }

    fn make_file_no_localizations(keys: Vec<&str>) -> XcStringsFile {
        let mut strings = OrderedMap::new();
        for key in keys {
            strings.insert(
                key.to_string(),
                StringEntry {
                    extraction_state: None,
                    should_translate: true,
                    comment: None,
                    localizations: None,
                },
            );
        }
        XcStringsFile {
            source_language: "en".to_string(),
            strings,
            version: "1.0".to_string(),
        }
    }

    #[test]
    fn added_key_appears_in_added() {
        let old = make_file(vec![("greeting", "Hello")]);
        let new = make_file(vec![("greeting", "Hello"), ("farewell", "Goodbye")]);

        let report = compute_diff(&old, &new);
        assert_eq!(report.added, vec!["farewell"]);
        assert!(report.removed.is_empty());
        assert!(report.modified.is_empty());
    }

    #[test]
    fn removed_key_appears_in_removed() {
        let old = make_file(vec![("greeting", "Hello"), ("farewell", "Goodbye")]);
        let new = make_file(vec![("greeting", "Hello")]);

        let report = compute_diff(&old, &new);
        assert!(report.added.is_empty());
        assert_eq!(report.removed, vec!["farewell"]);
        assert!(report.modified.is_empty());
    }

    #[test]
    fn changed_source_text_appears_in_modified() {
        let old = make_file(vec![("greeting", "Hello")]);
        let new = make_file(vec![("greeting", "Hi there")]);

        let report = compute_diff(&old, &new);
        assert!(report.added.is_empty());
        assert!(report.removed.is_empty());
        assert_eq!(report.modified.len(), 1);
        assert_eq!(report.modified[0].key, "greeting");
        assert_eq!(report.modified[0].old_value, "Hello");
        assert_eq!(report.modified[0].new_value, "Hi there");
    }

    #[test]
    fn no_changes_all_lists_empty() {
        let old = make_file(vec![("greeting", "Hello"), ("farewell", "Goodbye")]);
        let new = make_file(vec![("greeting", "Hello"), ("farewell", "Goodbye")]);

        let report = compute_diff(&old, &new);
        assert!(report.added.is_empty());
        assert!(report.removed.is_empty());
        assert!(report.modified.is_empty());
    }

    #[test]
    fn key_without_source_localization_compared_as_empty() {
        let old = make_file_no_localizations(vec!["greeting"]);
        let new = make_file(vec![("greeting", "Hello")]);

        let report = compute_diff(&old, &new);
        assert!(report.added.is_empty());
        assert!(report.removed.is_empty());
        assert_eq!(report.modified.len(), 1);
        assert_eq!(report.modified[0].key, "greeting");
        assert_eq!(report.modified[0].old_value, "");
        assert_eq!(report.modified[0].new_value, "Hello");
    }

    #[test]
    fn both_without_localization_are_equal() {
        let old = make_file_no_localizations(vec!["greeting"]);
        let new = make_file_no_localizations(vec!["greeting"]);

        let report = compute_diff(&old, &new);
        assert!(report.added.is_empty());
        assert!(report.removed.is_empty());
        assert!(report.modified.is_empty());
    }
}
