use super::{assessment, extractor::build_translation_unit};
use crate::error::XcStringsError;
use crate::model::{
    plural::plural_categories,
    translation::PluralUnit,
    xcstrings::{XcStringsFile, paths::LeafStep},
};
use std::collections::BTreeMap;

/// Return incomplete keys with any plural, device or substitution destination.
pub fn get_untranslated_plurals(
    file: &XcStringsFile,
    locale: &str,
    batch_size: usize,
    offset: usize,
) -> Result<(Vec<PluralUnit>, usize), XcStringsError> {
    if locale.is_empty() {
        return Err(XcStringsError::LocaleNotFound("locale is empty".into()));
    }
    if batch_size == 0 || batch_size > 100 {
        return Err(XcStringsError::InvalidBatchSize(format!(
            "batch_size must be 1..=100, got {batch_size}"
        )));
    }
    let required_forms: Vec<String> = plural_categories(locale)?
        .iter()
        .map(|c| c.as_str().into())
        .collect();
    let mut results = Vec::new();
    for (key, entry) in &file.strings {
        if !entry.should_translate {
            continue;
        }
        let assessment = assessment::assess(key, entry, &file.source_language, locale);
        if assessment.complete() || !assessment.leaves.iter().any(|leaf| !leaf.path.is_empty()) {
            continue;
        }
        let unit = build_translation_unit(key, entry, &file.source_language, locale);
        let source_forms = flat_forms(
            entry
                .localizations
                .as_ref()
                .and_then(|locs| locs.get(&file.source_language)),
        );
        let existing_translations = flat_forms(
            entry
                .localizations
                .as_ref()
                .and_then(|locs| locs.get(locale)),
        );
        let mut device_forms = Vec::new();
        // Legacy flattened fields describe direct plural branches; leaves retain all scopes.
        for leaf in &unit.leaves {
            if let Some(LeafStep::Device(category)) = leaf.path.first() {
                let name = serde_json::to_value(category)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default();
                if !device_forms.contains(&name) {
                    device_forms.push(name);
                }
            }
        }
        results.push(PluralUnit {
            source_version: unit.source_version,
            source_freshness: unit.source_freshness,
            key: unit.key,
            source_text: unit.source_text,
            target_locale: unit.target_locale,
            comment: unit.comment,
            format_specifiers: unit.format_specifiers,
            required_forms: required_forms.clone(),
            source_forms,
            existing_translations,
            has_substitutions: unit.has_substitutions,
            device_forms,
            leaves: unit.leaves,
            diagnostics: unit.diagnostics,
        });
    }
    let total = results.len();
    Ok((
        results.into_iter().skip(offset).take(batch_size).collect(),
        total,
    ))
}

fn flat_forms(node: Option<&crate::model::xcstrings::Localization>) -> BTreeMap<String, String> {
    let Some(node) = node else {
        return BTreeMap::new();
    };
    let leaves = crate::model::xcstrings::paths::collect_leaves(node).leaves;
    let names: std::collections::HashSet<_> = leaves
        .iter()
        .filter_map(|leaf| match leaf.path.first() {
            Some(LeafStep::Substitution(name)) => Some(name),
            _ => None,
        })
        .collect();
    leaves
        .iter()
        .filter_map(|leaf| match leaf.path.as_slice() {
            [LeafStep::Plural(form)] => Some((form.clone(), leaf.unit.value.clone())),
            [LeafStep::Substitution(_), LeafStep::Plural(form)] if names.len() == 1 => {
                Some((form.clone(), leaf.unit.value.clone()))
            }
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::xcstrings::XcStringsFile;

    #[test]
    fn test_empty_file() {
        let json = r#"{
            "sourceLanguage": "en",
            "strings": {},
            "version": "1.0"
        }"#;
        let file: XcStringsFile = serde_json::from_str(json).unwrap();
        let (batch, total) = get_untranslated_plurals(&file, "de", 10, 0).unwrap();
        assert!(batch.is_empty());
        assert_eq!(total, 0);
    }

    #[test]
    fn test_plural_key_needing_translation() {
        let content = include_str!("../../tests/fixtures/with_plurals.xcstrings");
        let file: XcStringsFile = serde_json::from_str(content).unwrap();

        // "days_remaining" has en plurals but no uk → should be returned
        let (batch, total) = get_untranslated_plurals(&file, "uk", 100, 0).unwrap();
        assert!(total > 0);

        let days = batch.iter().find(|u| u.key == "days_remaining");
        assert!(days.is_some(), "days_remaining should need translation");

        let days = days.unwrap();
        assert_eq!(days.target_locale, "uk");
        // Ukrainian requires: one, few, many, other
        assert!(days.required_forms.contains(&"one".to_string()));
        assert!(days.required_forms.contains(&"few".to_string()));
        assert!(days.required_forms.contains(&"many".to_string()));
        assert!(days.required_forms.contains(&"other".to_string()));
        // Source forms should have one/other from English
        assert_eq!(
            days.source_forms.get("one"),
            Some(&"%lld day remaining".to_string())
        );
        assert_eq!(
            days.source_forms.get("other"),
            Some(&"%lld days remaining".to_string())
        );
    }

    #[test]
    fn test_fully_translated_plural_excluded() {
        let content = include_str!("../../tests/fixtures/with_plurals.xcstrings");
        let file: XcStringsFile = serde_json::from_str(content).unwrap();

        // "items_count" has full uk translation (one/few/many/other) → should be excluded
        let (batch, _) = get_untranslated_plurals(&file, "uk", 100, 0).unwrap();
        let items = batch.iter().find(|u| u.key == "items_count");
        assert!(
            items.is_none(),
            "fully translated items_count should be excluded"
        );
    }

    #[test]
    fn test_partially_translated_included() {
        let content = include_str!("../../tests/fixtures/with_plurals.xcstrings");
        let file: XcStringsFile = serde_json::from_str(content).unwrap();

        // "photos_count" has de with only "other" → should be included (missing "one")
        let (batch, _) = get_untranslated_plurals(&file, "de", 100, 0).unwrap();
        let photos = batch.iter().find(|u| u.key == "photos_count");
        assert!(
            photos.is_some(),
            "partially translated photos_count should be included"
        );

        let photos = photos.unwrap();
        assert_eq!(
            photos.existing_translations.get("other"),
            Some(&"%lld Fotos".to_string())
        );
        assert!(!photos.existing_translations.contains_key("one"));
    }

    #[test]
    fn test_substitution_key_parsed() {
        let content = include_str!("../../tests/fixtures/with_substitutions.xcstrings");
        let file: XcStringsFile = serde_json::from_str(content).unwrap();

        let (batch, total) = get_untranslated_plurals(&file, "de", 100, 0).unwrap();
        assert!(total > 0);

        let bird = batch.iter().find(|u| u.key == "bird_sighting");
        assert!(bird.is_some(), "bird_sighting should be returned");

        let bird = bird.unwrap();
        assert!(bird.has_substitutions);
        assert_eq!(bird.source_text, "I saw %#@BIRDS@ in the park");
        // Source forms should come from the substitution plurals
        assert!(bird.source_forms.contains_key("one"));
        assert!(bird.source_forms.contains_key("other"));
    }

    #[test]
    fn test_device_variant_key() {
        let content = include_str!("../../tests/fixtures/with_device_variants.xcstrings");
        let file: XcStringsFile = serde_json::from_str(content).unwrap();

        let (batch, total) = get_untranslated_plurals(&file, "de", 100, 0).unwrap();
        assert!(total > 0);

        let tap = batch.iter().find(|u| u.key == "tap_action");
        assert!(tap.is_some(), "tap_action should be returned");

        let tap = tap.unwrap();
        assert!(!tap.device_forms.is_empty());
        // Should contain iphone, ipad, mac
        assert!(tap.device_forms.contains(&"iphone".to_string()));
        assert!(tap.device_forms.contains(&"ipad".to_string()));
        assert!(tap.device_forms.contains(&"mac".to_string()));
    }

    #[test]
    fn test_should_not_translate_excluded() {
        let content = include_str!("../../tests/fixtures/with_plurals.xcstrings");
        let file: XcStringsFile = serde_json::from_str(content).unwrap();

        let (batch, _) = get_untranslated_plurals(&file, "de", 100, 0).unwrap();
        let no_translate = batch.iter().find(|u| u.key == "no_translate_plural");
        assert!(
            no_translate.is_none(),
            "shouldTranslate=false key should be excluded"
        );
    }

    #[test]
    fn test_batch_pagination() {
        let content = include_str!("../../tests/fixtures/with_plurals.xcstrings");
        let file: XcStringsFile = serde_json::from_str(content).unwrap();

        // Get total first
        let (_, total) = get_untranslated_plurals(&file, "de", 100, 0).unwrap();
        assert!(total > 1, "need at least 2 plural keys for pagination test");

        // Fetch in batches of 1
        let (batch1, total1) = get_untranslated_plurals(&file, "de", 1, 0).unwrap();
        assert_eq!(batch1.len(), 1);
        assert_eq!(total1, total);

        let (batch2, total2) = get_untranslated_plurals(&file, "de", 1, 1).unwrap();
        assert_eq!(total2, total);
        assert_eq!(batch2.len(), 1);

        // Different keys in each batch
        assert_ne!(batch1[0].key, batch2[0].key);

        // Offset beyond total returns empty
        let (batch_empty, _) = get_untranslated_plurals(&file, "de", 1, total).unwrap();
        assert!(batch_empty.is_empty());
    }
}
