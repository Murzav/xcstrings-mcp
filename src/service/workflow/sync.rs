use super::{inspect, read, source_snapshot, versions};
use crate::{
    error::XcStringsError,
    model::{
        workflow::*,
        xcstrings::{TranslationState, XcStringsFile},
    },
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub struct SourceSyncPlan {
    pub invalidation: Option<XcStringsFile>,
    pub checkpoint_sources: BTreeMap<String, SourceSnapshot>,
    pub report: SourceSyncReport,
    baseline_at_plan: Option<SourceBaseline>,
}
pub fn plan_source_sync(
    catalog_identity: &str,
    file: &XcStringsFile,
    document: &WorkflowDocument,
    mode: SyncMode,
) -> Result<SourceSyncPlan, XcStringsError> {
    let view = inspect(catalog_identity, file, document)?;
    if mode == SyncMode::AdoptExisting && document.source_baseline.is_some() {
        return Err(super::invalid(
            "adopt_requires_uninitialized",
            "existing checkpoints cannot be adopted over changed source",
        ));
    }
    let changed_keys: Vec<_> = view
        .keys
        .iter()
        .filter(|(_, status)| status.freshness == SourceFreshness::SourceChanged)
        .map(|(key, _)| key.clone())
        .collect();
    let untracked_keys: Vec<_> = view
        .keys
        .iter()
        .filter(|(_, status)| status.freshness == SourceFreshness::Untracked)
        .map(|(key, _)| key.clone())
        .collect();
    let removed_keys = document
        .source_baseline
        .as_ref()
        .map(|b| {
            b.sources
                .keys()
                .filter(|key| !file.strings.contains_key(*key))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let affected: BTreeSet<_> = changed_keys
        .iter()
        .chain(&untracked_keys)
        .cloned()
        .collect();
    let mut invalidation = None;
    let mut invalidated_destinations = Vec::new();
    let mut diagnostics = view.diagnostics;
    for key in &affected {
        diagnostics.extend(read::shape_diagnostics(file, key));
    }
    let blocked = diagnostics
        .iter()
        .any(|d| d.code == WorkflowCode::UnsupportedShape);
    if mode == SyncMode::Review && !blocked {
        for destination in read::destinations(file) {
            if affected.contains(&destination.key)
                && read::unit(file, &destination)
                    .is_some_and(|unit| crate::service::assessment::ready(&unit.state))
            {
                // Clone only when an actual physical state must change.
                let candidate = invalidation.get_or_insert_with(|| file.clone());
                if let Some(unit) = read::unit_mut(candidate, &destination) {
                    unit.state = TranslationState::NeedsReview;
                }
                invalidated_destinations.push(destination);
            }
        }
    }
    let checkpoint_sources = view
        .keys
        .into_iter()
        .map(|(key, status)| (key, status.current))
        .collect();
    let mut checkpoint_needed = document
        .source_baseline
        .as_ref()
        .is_none_or(|baseline| baseline.sources != checkpoint_sources);
    for pending in &document.pending_review {
        if !applicable(file, pending)? {
            checkpoint_needed = true;
        }
    }
    Ok(SourceSyncPlan {
        invalidation,
        checkpoint_sources,
        baseline_at_plan: document.source_baseline.clone(),
        report: SourceSyncReport {
            tracking_before: view.tracking,
            mode,
            changed_keys,
            untracked_keys,
            removed_keys,
            invalidated_destinations,
            checkpoint_needed,
            historical_freshness_unknown: mode == SyncMode::AdoptExisting,
            diagnostics,
        },
    })
}
pub fn plan_checkpoint(
    file: &XcStringsFile,
    document: &WorkflowDocument,
    captured: &SourceSyncPlan,
) -> Result<WorkflowDocument, XcStringsError> {
    super::document::validate(document)?;
    if captured
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == WorkflowCode::UnsupportedShape)
    {
        return Err(super::invalid(
            "unsupported_shape",
            "source checkpoint cannot certify an unsupported catalog shape",
        ));
    }
    if document.source_baseline != captured.baseline_at_plan {
        return Err(super::invalid(
            "source_changed_during_sync",
            "source baseline changed after the plan",
        ));
    }
    let snapshots: BTreeMap<_, _> = file
        .strings
        .keys()
        .map(|key| source_snapshot(file, document, key).map(|snapshot| (key.clone(), snapshot)))
        .collect::<Result<_, _>>()?;
    if snapshots != captured.checkpoint_sources {
        return Err(super::invalid(
            "source_changed_during_sync",
            "source or authored context changed after the plan",
        ));
    }
    let affected: BTreeSet<_> = captured
        .report
        .changed_keys
        .iter()
        .chain(&captured.report.untracked_keys)
        .cloned()
        .collect();
    if affected
        .iter()
        .any(|key| !read::shape_diagnostics(file, key).is_empty())
    {
        return Err(super::invalid(
            "unsupported_shape",
            "actual target shape changed or is unsupported at checkpoint",
        ));
    }
    let destinations = read::destinations(file);
    if captured.report.mode == SyncMode::Review {
        for destination in &destinations {
            if affected.contains(&destination.key)
                && read::unit(file, destination)
                    .is_some_and(|unit| crate::service::assessment::ready(&unit.state))
            {
                return Err(super::invalid(
                    "ready_targets_require_invalidation",
                    format!("{} / {} remains ready", destination.key, destination.locale),
                ));
            }
        }
    } else if document.source_baseline.is_some() {
        return Err(super::invalid(
            "adopt_requires_uninitialized",
            "adoption only applies to first initialization",
        ));
    }
    // Clone the document because checkpoint output must preserve unrelated authored namespaces.
    let mut result = document.clone();
    let mut pending = Vec::new();
    for record in &document.pending_review {
        if applicable(file, record)? {
            pending.push(record.clone());
        }
    }
    for destination in destinations {
        if !affected.contains(&destination.key)
            || read::unit(file, &destination)
                .is_none_or(|unit| crate::service::assessment::ready(&unit.state))
        {
            continue;
        }
        let Some(target_content_version) = versions::content_version(file, &destination)? else {
            continue;
        };
        let existing = pending
            .iter()
            .position(|record| record.destination == destination);
        let previous = existing.map(|index| pending.remove(index));
        let old_source = match &previous {
            Some(record) => record.old_source.clone(),
            None => document
                .source_baseline
                .as_ref()
                .and_then(|baseline| baseline.sources.get(&destination.key))
                .cloned(),
        };
        let new_source = snapshots
            .get(&destination.key)
            .ok_or_else(|| {
                super::invalid(
                    "source_changed_during_sync",
                    "destination source disappeared",
                )
            })?
            .clone();
        pending.push(PendingReview {
            destination,
            old_source,
            new_source,
            target_content_version,
            extra: previous.map(|record| record.extra).unwrap_or_default(),
        });
    }
    pending.sort_by_cached_key(|record| {
        (
            record.destination.key.clone(),
            record.destination.locale.clone(),
            serde_json::json!(record.destination.path).to_string(),
        )
    });
    result.pending_review = pending;
    let extra = result
        .source_baseline
        .take()
        .map(|baseline| baseline.extra)
        .unwrap_or_default();
    result.source_baseline = Some(SourceBaseline {
        sources: snapshots,
        extra,
    });
    Ok(result)
}
pub(super) fn applicable(
    file: &XcStringsFile,
    pending: &PendingReview,
) -> Result<bool, XcStringsError> {
    if read::unit(file, &pending.destination)
        .is_none_or(|unit| crate::service::assessment::ready(&unit.state))
    {
        return Ok(false);
    }
    Ok(
        versions::content_version(file, &pending.destination)?.as_ref()
            == Some(&pending.target_content_version),
    )
}
