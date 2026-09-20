use super::{inspect, read, versions};
use crate::{
    error::XcStringsError,
    model::{
        workflow::*,
        xcstrings::{TranslationState, XcStringsFile},
    },
};
use serde_json::json;

pub fn review_queue(
    catalog_identity: &str,
    file: &XcStringsFile,
    document: &WorkflowDocument,
    query: &ReviewQuery,
) -> Result<ReviewPage, XcStringsError> {
    if query.batch_size == 0 || query.batch_size > 100 {
        return Err(XcStringsError::InvalidBatchSize(
            "review batch_size must be 1..=100".into(),
        ));
    }
    if let Some(locale) = &query.locale {
        crate::model::plural::plural_categories(locale)?;
    }
    let view = inspect(catalog_identity, file, document)?;
    let mut filters = query.reasons.clone();
    filters.sort();
    filters.dedup();
    // Bind all semantic inputs, even changes that leave the visible rows unchanged.
    let queue_version = versions::hash(
        "review-queue-v1",
        &json!({"identity":catalog_identity,"catalog":file,"workflow":document,"locale":query.locale,"reasons":filters,"order":"key_locale_typed_path_v1"}),
    );
    match query.expected_queue_version.as_ref() {
        None if query.offset > 0 => {
            return Err(super::invalid(
                "queue_version_required",
                "later pages require the first page revision",
            ));
        }
        Some(expected) if expected != &queue_version => {
            return Err(super::invalid(
                "queue_version_mismatch",
                "catalog, workflow or query changed; restart at offset zero",
            ));
        }
        _ => {}
    }
    let mut items = Vec::new();
    for destination in read::destinations(file) {
        if query
            .locale
            .as_ref()
            .is_some_and(|locale| locale != &destination.locale)
        {
            continue;
        }
        let Some(native) = read::unit(file, &destination) else {
            continue;
        };
        let status = &view.keys[&destination.key];
        let ready = crate::service::assessment::ready(&native.state);
        let draft = matches!(
            native.state,
            TranslationState::New | TranslationState::NeedsReview
        );
        let content = versions::content_version(file, &destination)?;
        let pending = document.pending_review.iter().find(|record| {
            record.destination == destination
                && !ready
                && Some(&record.target_content_version) == content.as_ref()
        });
        let mut reasons = Vec::new();
        if draft {
            reasons.push(ReviewReason::Draft);
        }
        if status.freshness == SourceFreshness::SourceChanged
            || pending.is_some_and(|r| {
                r.old_source
                    .as_ref()
                    .is_some_and(|old| old != &status.current)
            })
        {
            reasons.push(ReviewReason::SourceChanged);
        }
        if status.freshness == SourceFreshness::Untracked
            || pending.is_some_and(|record| record.old_source.is_none())
        {
            reasons.push(ReviewReason::Untracked);
        }
        let assessment = crate::service::assessment::assess(
            &destination.key,
            &file.strings[&destination.key],
            &file.source_language,
            &destination.locale,
        );
        let mut diagnostics: Vec<_> = assessment
            .diagnostics
            .into_iter()
            .map(|diagnostic| {
                super::diagnostic(
                    WorkflowCode::UnsupportedShape,
                    diagnostic.detail,
                    Some(destination.clone()),
                    None,
                )
            })
            .collect();
        if !ready && !draft {
            diagnostics.push(super::diagnostic(
                WorkflowCode::NotDraft,
                "native state is not an approvable draft",
                Some(destination.clone()),
                None,
            ));
        }
        if reasons.is_empty() && diagnostics.is_empty() {
            continue;
        }
        if !filters.is_empty() && !reasons.iter().any(|reason| filters.contains(reason)) {
            continue;
        }
        let source_text = assessment
            .leaves
            .iter()
            .find(|leaf| leaf.path == destination.path)
            .map_or_else(|| destination.key.clone(), |leaf| leaf.source_text.clone());
        let Some(target_version) = super::target_version(catalog_identity, file, &destination)?
        else {
            continue;
        };
        let effective = read::unit(&view.effective_catalog, &destination)
            .map_or_else(|| native.state.clone(), |u| u.state.clone());
        let old_source = match pending {
            // An explicit unknown origin must not become a later source baseline.
            Some(record) => record.old_source.clone(),
            None if status.freshness == SourceFreshness::SourceChanged => status.baseline.clone(),
            None => None,
        };
        items.push(ReviewItem {
            destination,
            source_text,
            value: native.value.clone(),
            native_state: native.state.clone(),
            effective_state: effective,
            source_version: status.source_version.clone(),
            target_version,
            freshness: status.freshness,
            reasons,
            old_source,
            current_source: status.current.clone(),
            diagnostics,
        });
    }
    let total = items.len();
    let end = query.offset.saturating_add(query.batch_size).min(total);
    let items = items
        .into_iter()
        .skip(query.offset)
        .take(query.batch_size)
        .collect();
    Ok(ReviewPage {
        tracking: view.tracking,
        items,
        total,
        queue_version,
        offset: query.offset,
        next_offset: if end < total { Some(end) } else { None },
        diagnostics: view.diagnostics,
    })
}
