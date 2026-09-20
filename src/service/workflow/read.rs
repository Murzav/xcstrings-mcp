use super::{source_snapshot, source_version, target_version};
use crate::{
    error::XcStringsError,
    model::{
        translation::TranslationDestination,
        workflow::*,
        xcstrings::{
            StringUnit, TranslationState, XcStringsFile,
            paths::{collect_leaves, find_leaf, find_leaf_mut},
        },
    },
};
use std::{borrow::Cow, collections::BTreeMap};

#[derive(Debug)]
pub struct WorkflowView<'a> {
    pub effective_catalog: Cow<'a, XcStringsFile>,
    pub tracking: TrackingStatus,
    pub keys: BTreeMap<String, KeySourceStatus>,
    pub diagnostics: Vec<WorkflowDiagnostic>,
}
pub fn inspect<'a>(
    catalog_identity: &str,
    file: &'a XcStringsFile,
    document: &WorkflowDocument,
) -> Result<WorkflowView<'a>, XcStringsError> {
    super::identity(catalog_identity)?;
    super::document::validate(document)?;
    let mut keys = BTreeMap::new();
    for key in file.strings.keys() {
        let current = source_snapshot(file, document, key)?;
        let baseline = document
            .source_baseline
            .as_ref()
            .and_then(|b| b.sources.get(key))
            .cloned();
        let freshness = match &baseline {
            None => SourceFreshness::Untracked,
            Some(old) if old == &current => SourceFreshness::Current,
            Some(_) => SourceFreshness::SourceChanged,
        };
        keys.insert(
            key.clone(),
            KeySourceStatus {
                freshness,
                source_version: source_version(catalog_identity, key, &current),
                current,
                baseline,
            },
        );
    }
    let tracking = if document.source_baseline.is_some() {
        TrackingStatus::Initialized
    } else {
        TrackingStatus::Uninitialized
    };
    let mut effective = Cow::Borrowed(file);
    if tracking == TrackingStatus::Initialized {
        for destination in destinations(file) {
            if keys[&destination.key].freshness != SourceFreshness::Current
                && unit(file, &destination)
                    .is_some_and(|u| crate::service::assessment::ready(&u.state))
                && let Some(unit) = unit_mut(effective.to_mut(), &destination)
            {
                unit.state = TranslationState::NeedsReview;
            }
        }
    }
    let mut diagnostics = Vec::new();
    if let Some(baseline) = &document.source_baseline {
        for key in baseline
            .sources
            .keys()
            .filter(|key| !file.strings.contains_key(*key))
        {
            diagnostics.push(super::diagnostic(
                WorkflowCode::StaleBaselineKey,
                "source baseline key is no longer in catalog",
                None,
                Some(key.clone()),
            ));
        }
    }
    for key in document
        .contexts
        .keys()
        .filter(|key| !file.strings.contains_key(*key))
    {
        diagnostics.push(super::diagnostic(
            WorkflowCode::StaleContextKey,
            "authored context key is no longer in catalog",
            None,
            Some(key.clone()),
        ));
    }
    Ok(WorkflowView {
        effective_catalog: effective,
        tracking,
        keys,
        diagnostics,
    })
}
pub fn leaf_status(
    catalog_identity: &str,
    file: &XcStringsFile,
    view: &WorkflowView<'_>,
    destination: &TranslationDestination,
) -> Result<LeafWorkflowStatus, XcStringsError> {
    let status = view
        .keys
        .get(&destination.key)
        .ok_or_else(|| XcStringsError::KeyNotFound(destination.key.clone()))?;
    Ok(LeafWorkflowStatus {
        source_version: status.source_version.clone(),
        target_version: target_version(catalog_identity, file, destination)?,
        freshness: status.freshness,
        native_state: unit(file, destination).map(|u| u.state.clone()),
    })
}
pub(super) fn destinations(file: &XcStringsFile) -> Vec<TranslationDestination> {
    let mut result = Vec::new();
    for (key, entry) in &file.strings {
        if !entry.should_translate {
            continue;
        }
        if let Some(locales) = &entry.localizations {
            for (locale, root) in locales {
                if locale == &file.source_language {
                    continue;
                }
                for leaf in collect_leaves(root).leaves {
                    result.push(TranslationDestination {
                        key: key.clone(),
                        locale: locale.clone(),
                        path: leaf.path,
                    });
                }
            }
        }
    }
    result.sort_by_cached_key(|destination| {
        (
            destination.key.clone(),
            destination.locale.clone(),
            serde_json::json!(destination.path).to_string(),
        )
    });
    result
}
pub(super) fn unit<'a>(
    file: &'a XcStringsFile,
    destination: &TranslationDestination,
) -> Option<&'a StringUnit> {
    find_leaf(
        file.strings
            .get(&destination.key)?
            .localizations
            .as_ref()?
            .get(&destination.locale)?,
        &destination.path,
    )
}
pub(super) fn unit_mut<'a>(
    file: &'a mut XcStringsFile,
    destination: &TranslationDestination,
) -> Option<&'a mut StringUnit> {
    find_leaf_mut(
        file.strings
            .get_mut(&destination.key)?
            .localizations
            .as_mut()?
            .get_mut(&destination.locale)?,
        &destination.path,
    )
}

pub(super) fn shape_diagnostics(file: &XcStringsFile, key: &str) -> Vec<WorkflowDiagnostic> {
    let Some(entry) = file.strings.get(key) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    if let Some(locales) = &entry.localizations {
        for (locale, root) in locales {
            let traversal = collect_leaves(root);
            for diagnostic in traversal.diagnostics {
                result.push(super::diagnostic(
                    WorkflowCode::UnsupportedShape,
                    diagnostic.detail,
                    Some(TranslationDestination {
                        key: key.into(),
                        locale: locale.clone(),
                        path: diagnostic.path,
                    }),
                    None,
                ));
            }
            for leaf in traversal.leaves {
                if let Err(detail) =
                    crate::model::xcstrings::paths::validate_supported_path(&leaf.path)
                {
                    result.push(super::diagnostic(
                        WorkflowCode::UnsupportedShape,
                        detail,
                        Some(TranslationDestination {
                            key: key.into(),
                            locale: locale.clone(),
                            path: leaf.path,
                        }),
                        None,
                    ));
                }
            }
            if let Err(detail) = crate::service::assessment::validate_substitution_references(root)
            {
                result.push(super::diagnostic(
                    WorkflowCode::UnsupportedShape,
                    detail,
                    None,
                    Some(key.into()),
                ));
            }
        }
    }
    result
}
