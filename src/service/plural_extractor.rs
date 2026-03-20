use std::collections::BTreeMap;

use crate::error::XcStringsError;
use crate::model::plural::required_plural_forms;
use crate::model::specifier::extract_specifiers;
use crate::model::translation::PluralUnit;
use crate::model::xcstrings::XcStringsFile;

/// Extract keys needing plural/device translation for a locale.
/// Returns `(batch, total_count)`.
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

    let required = required_plural_forms(locale);
    let required_form_names: Vec<String> = required
        .iter()
        .map(|cat| cat.as_str().to_string())
        .collect();

    let mut results = Vec::new();

    for (key, entry) in &file.strings {
        if !entry.should_translate {
            continue;
        }

        let source_loc = entry
            .localizations
            .as_ref()
            .and_then(|locs| locs.get(&file.source_language));

        let source_loc = match source_loc {
            Some(loc) => loc,
            None => continue,
        };

        let has_plural_variations = source_loc
            .variations
            .as_ref()
            .is_some_and(|v| v.plural.is_some());

        let has_device_variations = source_loc
            .variations
            .as_ref()
            .is_some_and(|v| v.device.is_some());

        let has_substitutions = source_loc.substitutions.is_some();

        // Skip simple keys (no plurals, no substitutions, no device variants)
        if !has_plural_variations && !has_substitutions && !has_device_variations {
            continue;
        }

        // Get source text: from string_unit or fall back to key
        let source_text = source_loc
            .string_unit
            .as_ref()
            .map(|su| su.value.clone())
            .unwrap_or_else(|| key.clone());

        let format_specifiers: Vec<String> = extract_specifiers(&source_text)
            .iter()
            .map(|s| s.raw.clone())
            .collect();

        // Collect source plural forms
        let mut source_forms = BTreeMap::new();
        if let Some(variations) = &source_loc.variations
            && let Some(plural) = &variations.plural
        {
            for (form, var) in plural {
                source_forms.insert(form.clone(), var.string_unit.value.clone());
            }
        }

        // Collect source plural forms from substitutions
        let sub_plurals = source_loc
            .substitutions
            .as_ref()
            .map(parse_substitution_plurals)
            .unwrap_or_default();

        // For substitution keys without direct plural variations, use substitution plurals
        if source_forms.is_empty()
            && !sub_plurals.is_empty()
            && let Some((_, forms)) = sub_plurals.first()
        {
            source_forms.clone_from(forms);
        }

        // Collect device forms from source
        let device_forms: Vec<String> = if let Some(variations) = &source_loc.variations {
            if let Some(device) = &variations.device {
                device
                    .keys()
                    .map(|cat| {
                        serde_json::to_string(cat)
                            .unwrap_or_else(|_| "\"unknown\"".to_string())
                            .trim_matches('"')
                            .to_string()
                    })
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Check target locale: collect existing translations
        let target_loc = entry
            .localizations
            .as_ref()
            .and_then(|locs| locs.get(locale));

        let mut existing_translations = BTreeMap::new();
        let mut target_has_all_forms = false;

        if let Some(t_loc) = target_loc {
            if let Some(variations) = &t_loc.variations {
                if let Some(plural) = &variations.plural {
                    for (form, var) in plural {
                        existing_translations.insert(form.clone(), var.string_unit.value.clone());
                    }
                }

                // Check device variations for target
                if let Some(device) = &variations.device
                    && !device_forms.is_empty()
                    && device.len() >= device_forms.len()
                {
                    // Has all device forms — for device-only keys this counts as complete
                    if !has_plural_variations && !has_substitutions {
                        target_has_all_forms = true;
                    }
                }
            }

            // For plural keys, check if all required forms are present
            if has_plural_variations || has_substitutions {
                target_has_all_forms = required_form_names
                    .iter()
                    .all(|form| existing_translations.contains_key(form));
            }
        }

        // Skip fully translated plural keys
        if target_has_all_forms {
            continue;
        }

        results.push(PluralUnit {
            key: key.clone(),
            source_text,
            target_locale: locale.to_string(),
            comment: entry.comment.clone(),
            format_specifiers,
            required_forms: required_form_names.clone(),
            source_forms,
            existing_translations,
            has_substitutions,
            device_forms,
        });
    }

    let total = results.len();
    let batch: Vec<PluralUnit> = results.into_iter().skip(offset).take(batch_size).collect();

    Ok((batch, total))
}

/// Extract plural form values from substitution JSON entries.
/// Returns `(substitution_name, { form_name -> value })`.
fn parse_substitution_plurals(
    subs: &BTreeMap<String, serde_json::Value>,
) -> Vec<(String, BTreeMap<String, String>)> {
    let mut result = Vec::new();

    for (name, value) in subs {
        let mut forms = BTreeMap::new();

        let plural = value
            .get("variations")
            .and_then(|v| v.get("plural"))
            .and_then(|p| p.as_object());

        if let Some(plural_map) = plural {
            for (form, form_value) in plural_map {
                if let Some(val) = form_value
                    .get("stringUnit")
                    .and_then(|su| su.get("value"))
                    .and_then(|v| v.as_str())
                {
                    forms.insert(form.clone(), val.to_string());
                }
            }
        }

        if !forms.is_empty() {
            result.push((name.clone(), forms));
        }
    }

    result
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
