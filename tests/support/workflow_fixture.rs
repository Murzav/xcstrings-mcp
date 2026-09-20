//! Explicit workflow setup for pre-existing behavior fixtures. Revision/race
//! tests capture inputs themselves and never use these current-value adapters.
#![allow(dead_code)]
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};
use xcstrings_mcp::{
    io::{FileStore, fs::FsFileStore},
    model::{
        workflow::{SyncMode, WorkflowDocument},
        xcstrings::XcStringsFile,
    },
    service::workflow,
    workflow_operation::CatalogSnapshot,
};

pub fn capture(file: &XcStringsFile, mut requests: Value) -> Value {
    let view = workflow::inspect(
        "/test/Fixture.xcstrings",
        file,
        &WorkflowDocument::default(),
    )
    .unwrap();
    let populate = |request: &mut Value| {
        let key = request["key"].as_str().unwrap();
        request["expected_source_version"] = Value::String(
            view.keys
                .get(key)
                .map(|status| status.source_version.clone())
                .unwrap_or_else(|| "unknown-key-fixture".into()),
        );
    };
    match &mut requests {
        Value::Array(items) => items.iter_mut().for_each(populate),
        _ => populate(&mut requests),
    }
    requests
}

pub fn import_versions(catalog: &Path, xliff: &Path) -> String {
    let store = FsFileStore::new();
    let mut versions = BTreeMap::new();
    if let Ok(snapshot) = CatalogSnapshot::load(&store, catalog) {
        if snapshot.workflow.source_baseline.is_none() {
            let plan = workflow::plan_source_sync(
                snapshot.identity_text().unwrap(),
                &snapshot.catalog,
                &snapshot.workflow,
                SyncMode::AdoptExisting,
            )
            .unwrap();
            let checkpoint =
                workflow::plan_checkpoint(&snapshot.catalog, &snapshot.workflow, &plan).unwrap();
            snapshot
                .write_workflow(&store, &checkpoint, &snapshot.catalog_bytes)
                .unwrap();
        }
        let snapshot = CatalogSnapshot::load(&store, catalog).unwrap();
        versions = workflow::inspect(
            snapshot.identity_text().unwrap(),
            &snapshot.catalog,
            &snapshot.workflow,
        )
        .unwrap()
        .keys
        .into_iter()
        .map(|(key, status)| (key, status.source_version))
        .collect();
    }
    let directory = if catalog.exists() {
        catalog.parent().unwrap()
    } else {
        xliff.parent().unwrap()
    };
    let path = directory.join("captured-test-source-versions.json");
    store
        .write(&path, &serde_json::to_string_pretty(&versions).unwrap())
        .unwrap();
    path.to_str().unwrap().into()
}

pub fn approve_drafts(file: &mut XcStringsFile, locale: &str) {
    use xcstrings_mcp::model::{
        translation::TranslationDestination,
        workflow::ApprovalRequest,
        xcstrings::{TranslationState, paths::collect_leaves},
    };
    let identity = "/test/Fixture.xcstrings";
    let document = WorkflowDocument::default();
    let sync =
        workflow::plan_source_sync(identity, file, &document, SyncMode::AdoptExisting).unwrap();
    let document = workflow::plan_checkpoint(file, &document, &sync).unwrap();
    let view = workflow::inspect(identity, file, &document).unwrap();
    let mut requests = Vec::new();
    for (key, entry) in &file.strings {
        if let Some(target) = entry
            .localizations
            .as_ref()
            .and_then(|locales| locales.get(locale))
        {
            for leaf in collect_leaves(target).leaves {
                if matches!(
                    leaf.unit.state,
                    TranslationState::New | TranslationState::NeedsReview
                ) {
                    let destination = TranslationDestination {
                        key: key.clone(),
                        locale: locale.into(),
                        path: leaf.path.clone(),
                    };
                    requests.push(ApprovalRequest {
                        key: key.clone(),
                        locale: locale.into(),
                        path: leaf.path,
                        expected_source_version: view.keys[key].source_version.clone(),
                        expected_target_version: workflow::target_version(
                            identity,
                            file,
                            &destination,
                        )
                        .unwrap()
                        .unwrap(),
                    });
                }
            }
        }
    }
    let plan = workflow::plan_approval(identity, file, &document, &requests).unwrap();
    assert_eq!(plan.report.rejected, vec![]);
    assert_eq!(plan.report.accepted, requests.len());
    if let Some(candidate) = plan.candidate {
        *file = candidate;
    }
}
