use indexmap::IndexMap;

use crate::error::XcStringsError;
use crate::model::xcstrings::{
    ExtractionState, Localization, StringEntry, StringUnit, TranslationState, XcStringsFile,
};

/// Create an empty XcStringsFile with the given source language.
pub fn create_empty_file(source_language: &str) -> Result<XcStringsFile, XcStringsError> {
    if source_language.is_empty() {
        return Err(XcStringsError::InvalidFormat(
            "source_language is empty".into(),
        ));
    }
    Ok(XcStringsFile {
        source_language: source_language.to_string(),
        strings: IndexMap::new(),
        version: "1.0".to_string(),
    })
}

/// Request to add a single key to an XcStringsFile.
pub struct AddKeyRequest {
    pub key: String,
    pub source_text: String,
    pub comment: Option<String>,
}

/// Result of adding keys to a file.
pub struct AddKeysResult {
    pub added: usize,
    pub skipped: Vec<String>,
}

/// Add keys to an XcStringsFile. Skips duplicates.
pub fn add_keys(file: &mut XcStringsFile, keys: &[AddKeyRequest]) -> AddKeysResult {
    let source_language = file.source_language.clone();
    let mut added = 0;
    let mut skipped = Vec::new();

    for req in keys {
        if file.strings.contains_key(&req.key) {
            skipped.push(req.key.clone());
            continue;
        }

        let mut localizations = IndexMap::new();
        localizations.insert(
            source_language.clone(),
            Localization {
                string_unit: Some(StringUnit {
                    state: TranslationState::Translated,
                    value: req.source_text.clone(),
                }),
                variations: None,
                substitutions: None,
            },
        );

        let entry = StringEntry {
            extraction_state: Some(ExtractionState::Manual),
            should_translate: true,
            comment: req.comment.clone(),
            localizations: Some(localizations),
        };

        file.strings.insert(req.key.clone(), entry);
        added += 1;
    }

    AddKeysResult { added, skipped }
}

/// Update comments on existing keys. Returns count of updated keys.
/// Silently skips non-existent keys.
pub fn update_comments(file: &mut XcStringsFile, updates: &[(String, String)]) -> usize {
    let mut count = 0;
    for (key, comment) in updates {
        if let Some(entry) = file.strings.get_mut(key) {
            entry.comment = Some(comment.clone());
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_empty_file_valid() {
        let file = create_empty_file("en").unwrap();
        assert_eq!(file.source_language, "en");
        assert!(file.strings.is_empty());
        assert_eq!(file.version, "1.0");
    }

    #[test]
    fn create_empty_file_empty_source_language() {
        let result = create_empty_file("");
        assert!(matches!(
            result.unwrap_err(),
            XcStringsError::InvalidFormat(_)
        ));
    }

    #[test]
    fn add_keys_to_empty_file() {
        let mut file = create_empty_file("en").unwrap();
        let keys = vec![AddKeyRequest {
            key: "greeting".to_string(),
            source_text: "Hello".to_string(),
            comment: Some("A greeting".to_string()),
        }];

        let result = add_keys(&mut file, &keys);
        assert_eq!(result.added, 1);
        assert!(result.skipped.is_empty());
        assert_eq!(file.strings.len(), 1);

        let entry = &file.strings["greeting"];
        assert_eq!(entry.extraction_state, Some(ExtractionState::Manual));
        assert!(entry.should_translate);
        assert_eq!(entry.comment.as_deref(), Some("A greeting"));

        let locs = entry.localizations.as_ref().unwrap();
        let en = locs.get("en").unwrap().string_unit.as_ref().unwrap();
        assert_eq!(en.state, TranslationState::Translated);
        assert_eq!(en.value, "Hello");
    }

    #[test]
    fn add_keys_to_existing_file() {
        let mut file = create_empty_file("en").unwrap();
        let keys1 = vec![AddKeyRequest {
            key: "a".to_string(),
            source_text: "Alpha".to_string(),
            comment: None,
        }];
        add_keys(&mut file, &keys1);

        let keys2 = vec![AddKeyRequest {
            key: "b".to_string(),
            source_text: "Beta".to_string(),
            comment: None,
        }];
        let result = add_keys(&mut file, &keys2);
        assert_eq!(result.added, 1);
        assert_eq!(file.strings.len(), 2);
    }

    #[test]
    fn add_keys_duplicate_skip() {
        let mut file = create_empty_file("en").unwrap();
        let keys = vec![
            AddKeyRequest {
                key: "dup".to_string(),
                source_text: "First".to_string(),
                comment: None,
            },
            AddKeyRequest {
                key: "dup".to_string(),
                source_text: "Second".to_string(),
                comment: None,
            },
        ];

        let result = add_keys(&mut file, &keys);
        assert_eq!(result.added, 1);
        assert_eq!(result.skipped, vec!["dup"]);
        // First value should be kept
        let locs = file.strings["dup"].localizations.as_ref().unwrap();
        let en = locs.get("en").unwrap().string_unit.as_ref().unwrap();
        assert_eq!(en.value, "First");
    }

    #[test]
    fn add_keys_empty_list() {
        let mut file = create_empty_file("en").unwrap();
        let result = add_keys(&mut file, &[]);
        assert_eq!(result.added, 0);
        assert!(result.skipped.is_empty());
    }

    #[test]
    fn add_keys_comment_preserved() {
        let mut file = create_empty_file("en").unwrap();
        let keys = vec![AddKeyRequest {
            key: "k".to_string(),
            source_text: "val".to_string(),
            comment: Some("My comment".to_string()),
        }];
        add_keys(&mut file, &keys);
        assert_eq!(file.strings["k"].comment.as_deref(), Some("My comment"));
    }

    #[test]
    fn update_comments_updates_existing() {
        let mut file = create_empty_file("en").unwrap();
        let keys = vec![
            AddKeyRequest {
                key: "a".to_string(),
                source_text: "Alpha".to_string(),
                comment: None,
            },
            AddKeyRequest {
                key: "b".to_string(),
                source_text: "Beta".to_string(),
                comment: Some("old".to_string()),
            },
        ];
        add_keys(&mut file, &keys);

        let updates = vec![
            ("a".to_string(), "New comment for A".to_string()),
            ("b".to_string(), "Updated comment for B".to_string()),
            ("nonexistent".to_string(), "Should be skipped".to_string()),
        ];
        let count = update_comments(&mut file, &updates);
        assert_eq!(count, 2);
        assert_eq!(
            file.strings["a"].comment.as_deref(),
            Some("New comment for A")
        );
        assert_eq!(
            file.strings["b"].comment.as_deref(),
            Some("Updated comment for B")
        );
    }

    #[test]
    fn update_comments_nonexistent_key_skipped() {
        let mut file = create_empty_file("en").unwrap();
        let updates = vec![("missing".to_string(), "comment".to_string())];
        let count = update_comments(&mut file, &updates);
        assert_eq!(count, 0);
    }
}
