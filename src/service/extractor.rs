use super::assessment;
use crate::error::XcStringsError;
use crate::model::specifier::extract_specifiers;
use crate::model::translation::TranslationUnit;
use crate::model::xcstrings::{
    ExtractionState, StringEntry, XcStringsFile,
    paths::{LeafStep, collect_leaves},
};

fn validate_batch(locale: &str, batch_size: usize) -> Result<(), XcStringsError> {
    if locale.is_empty() {
        return Err(XcStringsError::LocaleNotFound("locale is empty".into()));
    }
    if batch_size == 0 || batch_size > 100 {
        return Err(XcStringsError::InvalidBatchSize(format!(
            "batch_size must be 1..=100, got {batch_size}"
        )));
    }
    Ok(())
}

pub fn get_untranslated(
    file: &XcStringsFile,
    locale: &str,
    batch_size: usize,
    offset: usize,
) -> Result<(Vec<TranslationUnit>, usize), XcStringsError> {
    get_untranslated_multi(file, &[locale], batch_size, offset)
}

pub(crate) fn build_translation_unit(
    key: &str,
    entry: &StringEntry,
    source_language: &str,
    locale: &str,
) -> TranslationUnit {
    let report = assessment::assess(key, entry, source_language, locale);
    let source_text = entry
        .localizations
        .as_ref()
        .and_then(|locs| locs.get(source_language))
        .and_then(|node| assessment::source_unit(node, &[]))
        .map_or_else(|| key.to_string(), |unit| unit.value.clone());
    TranslationUnit {
        source_version: None,
        source_freshness: None,
        key: key.into(),
        format_specifiers: extract_specifiers(&source_text)
            .into_iter()
            .map(|s| s.raw)
            .collect(),
        source_text,
        target_locale: locale.into(),
        comment: entry.comment.clone(),
        has_plurals: report
            .leaves
            .iter()
            .any(|l| l.path.iter().any(|s| matches!(s, LeafStep::Plural(_)))),
        has_substitutions: report.leaves.iter().any(|l| !l.substitutions.is_empty()),
        leaves: report.leaves,
        diagnostics: report.diagnostics,
    }
}

pub fn get_untranslated_multi(
    file: &XcStringsFile,
    locales: &[&str],
    batch_size: usize,
    offset: usize,
) -> Result<(Vec<TranslationUnit>, usize), XcStringsError> {
    let Some(first) = locales.first() else {
        return Err(XcStringsError::LocaleNotFound("no locales provided".into()));
    };
    for locale in locales {
        validate_batch(locale, batch_size)?;
    }
    let results: Vec<_> = file
        .strings
        .iter()
        .filter(|(_, e)| e.should_translate)
        .filter_map(|(key, entry)| {
            let locale = locales.iter().find(|locale| {
                !assessment::assess(key, entry, &file.source_language, locale).complete()
            })?;
            Some(build_translation_unit(
                key,
                entry,
                &file.source_language,
                locale,
            ))
        })
        .collect();
    let _ = first;
    Ok(page(results, batch_size, offset))
}

pub fn get_stale(
    file: &XcStringsFile,
    locale: &str,
    batch_size: usize,
    offset: usize,
) -> Result<(Vec<TranslationUnit>, usize), XcStringsError> {
    validate_batch(locale, batch_size)?;
    let results = file
        .strings
        .iter()
        .filter(|(_, e)| e.should_translate && e.extraction_state == Some(ExtractionState::Stale))
        .map(|(key, entry)| build_translation_unit(key, entry, &file.source_language, locale))
        .collect();
    Ok(page(results, batch_size, offset))
}

pub fn search_keys(
    file: &XcStringsFile,
    pattern: &str,
    locale: &str,
    batch_size: usize,
    offset: usize,
) -> Result<(Vec<TranslationUnit>, usize), XcStringsError> {
    validate_batch(locale, batch_size)?;
    let pattern = pattern.to_lowercase();
    let results = file
        .strings
        .iter()
        .filter(|(_, e)| e.should_translate)
        .filter(|(key, entry)| {
            key.to_lowercase().contains(&pattern)
                || entry
                    .localizations
                    .as_ref()
                    .and_then(|locs| locs.get(&file.source_language))
                    .is_some_and(|node| {
                        collect_leaves(node)
                            .leaves
                            .iter()
                            .any(|leaf| leaf.unit.value.to_lowercase().contains(&pattern))
                    })
        })
        .map(|(key, entry)| build_translation_unit(key, entry, &file.source_language, locale))
        .collect();
    Ok(page(results, batch_size, offset))
}

fn page(
    results: Vec<TranslationUnit>,
    batch_size: usize,
    offset: usize,
) -> (Vec<TranslationUnit>, usize) {
    let total = results.len();
    (
        results.into_iter().skip(offset).take(batch_size).collect(),
        total,
    )
}

#[cfg(test)]
#[path = "extractor_tests.rs"]
mod tests;
