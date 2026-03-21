use std::collections::BTreeMap;

use crate::error::XcStringsError;

/// Glossary: maps "source->target" locale pairs to term translations.
pub type Glossary = BTreeMap<String, BTreeMap<String, String>>;

pub(crate) fn locale_pair_key(source: &str, target: &str) -> String {
    format!("{source}\u{2192}{target}")
}

/// Deserialize glossary from a raw JSON string. Returns empty glossary if input is None.
pub fn parse_glossary(raw: Option<&str>) -> Result<Glossary, XcStringsError> {
    match raw {
        Some(json) => {
            serde_json::from_str(json).map_err(|e| XcStringsError::GlossaryError(e.to_string()))
        }
        None => Ok(Glossary::new()),
    }
}

/// Serialize glossary to a pretty-printed JSON string.
pub fn serialize_glossary(glossary: &Glossary) -> Result<String, XcStringsError> {
    serde_json::to_string_pretty(glossary).map_err(|e| XcStringsError::GlossaryError(e.to_string()))
}

/// Get glossary entries for a language pair, optionally filtered by substring.
pub fn get_entries(
    glossary: &Glossary,
    source_locale: &str,
    target_locale: &str,
    filter: Option<&str>,
) -> BTreeMap<String, String> {
    let key = locale_pair_key(source_locale, target_locale);
    let Some(entries) = glossary.get(&key) else {
        return BTreeMap::new();
    };
    match filter {
        Some(f) => {
            let f_lower = f.to_lowercase();
            entries
                .iter()
                .filter(|(k, v)| {
                    k.to_lowercase().contains(&f_lower) || v.to_lowercase().contains(&f_lower)
                })
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        }
        None => entries.clone(),
    }
}

/// Update glossary entries for a language pair (upsert). Returns number of entries updated.
pub fn update_entries(
    glossary: &mut Glossary,
    source_locale: &str,
    target_locale: &str,
    entries: BTreeMap<String, String>,
) -> usize {
    let key = locale_pair_key(source_locale, target_locale);
    let pair = glossary.entry(key).or_default();
    let count = entries.len();
    for (term, translation) in entries {
        pair.insert(term, translation);
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_then_parse_roundtrip() {
        let mut glossary = Glossary::new();
        let mut entries = BTreeMap::new();
        entries.insert("Settings".to_string(), "Nalashtuvannya".to_string());
        entries.insert("Cancel".to_string(), "Skasuvaty".to_string());
        update_entries(&mut glossary, "en", "uk", entries);

        let json = serialize_glossary(&glossary).unwrap();
        let reloaded = parse_glossary(Some(&json)).unwrap();

        let results = get_entries(&reloaded, "en", "uk", None);
        assert_eq!(results.len(), 2);
        assert_eq!(results.get("Settings").unwrap(), "Nalashtuvannya");
        assert_eq!(results.get("Cancel").unwrap(), "Skasuvaty");
    }

    #[test]
    fn parse_none_returns_empty() {
        let glossary = parse_glossary(None).unwrap();
        assert!(glossary.is_empty());
    }

    #[test]
    fn filter_by_substring_key_and_value() {
        let mut glossary = Glossary::new();
        let mut entries = BTreeMap::new();
        entries.insert("Settings".to_string(), "Einstellungen".to_string());
        entries.insert("Cancel".to_string(), "Abbrechen".to_string());
        entries.insert("Save".to_string(), "Speichern".to_string());
        update_entries(&mut glossary, "en", "de", entries);

        // Filter by key substring
        let results = get_entries(&glossary, "en", "de", Some("set"));
        assert_eq!(results.len(), 1);
        assert!(results.contains_key("Settings"));

        // Filter by value substring
        let results = get_entries(&glossary, "en", "de", Some("abbrech"));
        assert_eq!(results.len(), 1);
        assert!(results.contains_key("Cancel"));
    }

    #[test]
    fn empty_glossary_returns_empty_map() {
        let glossary = Glossary::new();
        let results = get_entries(&glossary, "en", "uk", None);
        assert!(results.is_empty());
    }

    #[test]
    fn overwrite_existing_entry() {
        let mut glossary = Glossary::new();
        let mut entries = BTreeMap::new();
        entries.insert("Settings".to_string(), "Old".to_string());
        update_entries(&mut glossary, "en", "uk", entries);

        let mut entries = BTreeMap::new();
        entries.insert("Settings".to_string(), "New".to_string());
        update_entries(&mut glossary, "en", "uk", entries);

        let results = get_entries(&glossary, "en", "uk", None);
        assert_eq!(results.get("Settings").unwrap(), "New");
    }

    #[test]
    fn corrupt_json_returns_glossary_error() {
        let result = parse_glossary(Some("not valid json{{{"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, XcStringsError::GlossaryError(_)),
            "expected GlossaryError, got: {err:?}"
        );
    }

    #[test]
    fn multiple_locale_pairs_independent() {
        let mut glossary = Glossary::new();

        let mut en_uk = BTreeMap::new();
        en_uk.insert("Settings".to_string(), "Nalashtuvannya".to_string());
        update_entries(&mut glossary, "en", "uk", en_uk);

        let mut en_de = BTreeMap::new();
        en_de.insert("Settings".to_string(), "Einstellungen".to_string());
        update_entries(&mut glossary, "en", "de", en_de);

        let uk = get_entries(&glossary, "en", "uk", None);
        let de = get_entries(&glossary, "en", "de", None);
        assert_eq!(uk.get("Settings").unwrap(), "Nalashtuvannya");
        assert_eq!(de.get("Settings").unwrap(), "Einstellungen");
    }
}
