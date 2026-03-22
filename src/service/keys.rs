use crate::error::XcStringsError;
use crate::model::xcstrings::XcStringsFile;

/// Result of deleting localization keys.
#[derive(Debug)]
pub struct DeleteKeysResult {
    pub deleted: Vec<String>,
    pub not_found: Vec<String>,
}

/// Result of renaming a localization key.
#[derive(Debug)]
pub struct RenameKeyResult {
    pub old_key: String,
    pub new_key: String,
}

/// Result of deleting translations for specific keys in a locale.
#[derive(Debug)]
pub struct DeleteTranslationsResult {
    pub reset: Vec<String>,
    pub not_found: Vec<String>,
}

/// Delete localization keys and all their translations from the file.
pub fn delete_keys(file: &mut XcStringsFile, keys: &[String]) -> DeleteKeysResult {
    let mut deleted = Vec::new();
    let mut not_found = Vec::new();

    for key in keys {
        if file.strings.shift_remove(key).is_some() {
            deleted.push(key.clone());
        } else {
            not_found.push(key.clone());
        }
    }

    DeleteKeysResult { deleted, not_found }
}

/// Rename a localization key, preserving all existing translations.
pub fn rename_key(
    file: &mut XcStringsFile,
    old_key: &str,
    new_key: &str,
) -> Result<RenameKeyResult, XcStringsError> {
    if new_key.is_empty() {
        return Err(XcStringsError::InvalidFormat(
            "new key name is empty".into(),
        ));
    }

    if file.strings.contains_key(new_key) {
        return Err(XcStringsError::KeyAlreadyExists(new_key.to_string()));
    }

    let entry = file
        .strings
        .shift_remove(old_key)
        .ok_or_else(|| XcStringsError::KeyNotFound(old_key.to_string()))?;

    file.strings.insert(new_key.to_string(), entry);

    Ok(RenameKeyResult {
        old_key: old_key.to_string(),
        new_key: new_key.to_string(),
    })
}

/// Delete translations for specific keys in a locale, resetting them to untranslated.
pub fn delete_translations(
    file: &mut XcStringsFile,
    keys: &[String],
    locale: &str,
    source_language: &str,
) -> Result<DeleteTranslationsResult, XcStringsError> {
    if locale == source_language {
        return Err(XcStringsError::CannotRemoveSourceLocale(locale.into()));
    }

    let mut reset = Vec::new();
    let mut not_found = Vec::new();

    for key in keys {
        let Some(entry) = file.strings.get_mut(key) else {
            not_found.push(key.clone());
            continue;
        };

        let removed = entry
            .localizations
            .as_mut()
            .and_then(|locs| locs.shift_remove(locale))
            .is_some();

        if removed {
            reset.push(key.clone());
        } else {
            not_found.push(key.clone());
        }
    }

    Ok(DeleteTranslationsResult { reset, not_found })
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::*;
    use crate::model::xcstrings::{Localization, StringEntry, StringUnit, TranslationState};

    fn make_file(strings: IndexMap<String, StringEntry>) -> XcStringsFile {
        XcStringsFile {
            source_language: "en".to_string(),
            strings,
            version: "1.0".to_string(),
        }
    }

    fn make_entry(locales: &[(&str, TranslationState)]) -> StringEntry {
        let mut localizations = IndexMap::new();
        for (locale, state) in locales {
            localizations.insert(
                locale.to_string(),
                Localization {
                    string_unit: Some(StringUnit {
                        state: state.clone(),
                        value: format!("value_{locale}"),
                    }),
                    variations: None,
                    substitutions: None,
                },
            );
        }
        StringEntry {
            extraction_state: None,
            should_translate: true,
            comment: None,
            localizations: if localizations.is_empty() {
                None
            } else {
                Some(localizations)
            },
        }
    }

    #[test]
    fn test_delete_keys_success() {
        let mut strings = IndexMap::new();
        strings.insert(
            "key1".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        strings.insert(
            "key2".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        let mut file = make_file(strings);

        let result = delete_keys(&mut file, &["key1".to_string()]);
        assert_eq!(result.deleted, vec!["key1"]);
        assert!(result.not_found.is_empty());
        assert!(!file.strings.contains_key("key1"));
        assert!(file.strings.contains_key("key2"));
    }

    #[test]
    fn test_delete_keys_not_found() {
        let mut strings = IndexMap::new();
        strings.insert(
            "key1".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        let mut file = make_file(strings);

        let result = delete_keys(&mut file, &["nonexistent".to_string()]);
        assert!(result.deleted.is_empty());
        assert_eq!(result.not_found, vec!["nonexistent"]);
    }

    #[test]
    fn test_delete_keys_mixed() {
        let mut strings = IndexMap::new();
        strings.insert(
            "key1".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        strings.insert(
            "key2".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        let mut file = make_file(strings);

        let result = delete_keys(
            &mut file,
            &[
                "key1".to_string(),
                "missing".to_string(),
                "key2".to_string(),
            ],
        );
        assert_eq!(result.deleted, vec!["key1", "key2"]);
        assert_eq!(result.not_found, vec!["missing"]);
    }

    #[test]
    fn test_delete_keys_empty_list() {
        let mut strings = IndexMap::new();
        strings.insert(
            "key1".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        let mut file = make_file(strings);

        let result = delete_keys(&mut file, &[]);
        assert!(result.deleted.is_empty());
        assert!(result.not_found.is_empty());
        assert_eq!(file.strings.len(), 1);
    }

    #[test]
    fn test_delete_keys_preserves_order() {
        let mut strings = IndexMap::new();
        strings.insert(
            "a".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        strings.insert(
            "b".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        strings.insert(
            "c".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        let mut file = make_file(strings);

        delete_keys(&mut file, &["b".to_string()]);
        let keys: Vec<&String> = file.strings.keys().collect();
        assert_eq!(keys, vec!["a", "c"]);
    }

    #[test]
    fn test_rename_key_success() {
        let mut strings = IndexMap::new();
        strings.insert(
            "old".to_string(),
            make_entry(&[
                ("en", TranslationState::Translated),
                ("de", TranslationState::Translated),
            ]),
        );
        let mut file = make_file(strings);

        let result = rename_key(&mut file, "old", "new").unwrap();
        assert_eq!(result.old_key, "old");
        assert_eq!(result.new_key, "new");
        assert!(!file.strings.contains_key("old"));

        let entry = &file.strings["new"];
        let locs = entry.localizations.as_ref().unwrap();
        assert!(locs.contains_key("en"));
        assert!(locs.contains_key("de"));
    }

    #[test]
    fn test_rename_key_not_found() {
        let mut file = make_file(IndexMap::new());
        let result = rename_key(&mut file, "missing", "new");
        assert!(matches!(
            result.unwrap_err(),
            XcStringsError::KeyNotFound(_)
        ));
    }

    #[test]
    fn test_rename_key_already_exists() {
        let mut strings = IndexMap::new();
        strings.insert(
            "old".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        strings.insert(
            "new".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        let mut file = make_file(strings);

        let result = rename_key(&mut file, "old", "new");
        assert!(matches!(
            result.unwrap_err(),
            XcStringsError::KeyAlreadyExists(_)
        ));
    }

    #[test]
    fn test_rename_key_empty_new_key() {
        let mut strings = IndexMap::new();
        strings.insert(
            "old".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        let mut file = make_file(strings);

        let result = rename_key(&mut file, "old", "");
        assert!(matches!(
            result.unwrap_err(),
            XcStringsError::InvalidFormat(_)
        ));
    }

    #[test]
    fn test_rename_key_same_key() {
        let mut strings = IndexMap::new();
        strings.insert(
            "key".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        let mut file = make_file(strings);

        // Same key → KeyAlreadyExists because new_key is already in the map
        let result = rename_key(&mut file, "key", "key");
        assert!(matches!(
            result.unwrap_err(),
            XcStringsError::KeyAlreadyExists(_)
        ));
    }

    #[test]
    fn test_delete_translations_success() {
        let mut strings = IndexMap::new();
        strings.insert(
            "key1".to_string(),
            make_entry(&[
                ("en", TranslationState::Translated),
                ("de", TranslationState::Translated),
            ]),
        );
        strings.insert(
            "key2".to_string(),
            make_entry(&[
                ("en", TranslationState::Translated),
                ("de", TranslationState::Translated),
            ]),
        );
        let mut file = make_file(strings);

        let result = delete_translations(
            &mut file,
            &["key1".to_string(), "key2".to_string()],
            "de",
            "en",
        )
        .unwrap();
        assert_eq!(result.reset, vec!["key1", "key2"]);
        assert!(result.not_found.is_empty());

        for entry in file.strings.values() {
            let locs = entry.localizations.as_ref().unwrap();
            assert!(!locs.contains_key("de"));
        }
    }

    #[test]
    fn test_delete_translations_source_locale() {
        let mut strings = IndexMap::new();
        strings.insert(
            "key1".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        let mut file = make_file(strings);

        let result = delete_translations(&mut file, &["key1".to_string()], "en", "en");
        assert!(matches!(
            result.unwrap_err(),
            XcStringsError::CannotRemoveSourceLocale(_)
        ));
    }

    #[test]
    fn test_delete_translations_key_not_found() {
        let mut strings = IndexMap::new();
        strings.insert(
            "key1".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        let mut file = make_file(strings);

        let result = delete_translations(&mut file, &["missing".to_string()], "de", "en").unwrap();
        assert!(result.reset.is_empty());
        assert_eq!(result.not_found, vec!["missing"]);
    }

    #[test]
    fn test_delete_translations_locale_not_present() {
        let mut strings = IndexMap::new();
        strings.insert(
            "key1".to_string(),
            make_entry(&[("en", TranslationState::Translated)]),
        );
        let mut file = make_file(strings);

        let result = delete_translations(&mut file, &["key1".to_string()], "de", "en").unwrap();
        assert!(result.reset.is_empty());
        assert_eq!(result.not_found, vec!["key1"]);
    }
}
