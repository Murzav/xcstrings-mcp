use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::guidance_operation::GuidanceSnapshot;
use crate::io::FileStore;
use crate::model::translation::{PluralUnit, TranslationDestination};
use crate::service::workflow;
use crate::service::{context, plural_extractor};
use crate::tools::FileCache;
use crate::tools::workflow::{read_result, resolve_read_snapshot};
use crate::workflow_operation::read::{annotate_plural, annotate_unit};
use std::path::Path;

fn default_plural_batch_size() -> usize {
    20
}

fn default_context_count() -> usize {
    5
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetPluralsParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Target locale code (e.g., "uk", "de")
    pub locale: String,
    /// Number of plural keys per batch (1-100, default 20). Plural keys are complex — use smaller batches than simple strings.
    #[serde(default = "default_plural_batch_size")]
    pub batch_size: usize,
    /// Offset for pagination (default 0). Set to previous offset + batch_size to get the next page.
    #[serde(default)]
    pub offset: usize,
}

#[derive(Debug, Serialize)]
pub(crate) struct GetPluralsResult {
    pub units: Vec<PluralUnit>,
    pub total: usize,
    pub offset: usize,
    pub batch_size: usize,
    pub has_more: bool,
}

/// Extract plural/device keys needing translation for a locale.
pub(crate) async fn handle_get_plurals(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    params: GetPluralsParams,
) -> Result<serde_json::Value, XcStringsError> {
    let snapshot = resolve_read_snapshot(store, cache, params.file_path.as_deref()).await?;
    let view = workflow::inspect(
        snapshot.identity_text()?,
        &snapshot.catalog,
        &snapshot.workflow,
    )?;

    let (mut units, total) = plural_extractor::get_untranslated_plurals(
        &view.effective_catalog,
        &params.locale,
        params.batch_size,
        params.offset,
    )?;

    for unit in &mut units {
        annotate_plural(&snapshot, &view, unit)?;
    }
    let has_more = params.offset + units.len() < total;

    let result = GetPluralsResult {
        units,
        total,
        offset: params.offset,
        batch_size: params.batch_size,
        has_more,
    };

    read_result(result, &snapshot, &view)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetContextParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Requested key. Returns its full translation/context package plus bounded related keys.
    pub key: String,
    /// Target locale code (e.g., "uk", "de")
    pub locale: String,
    /// Neighbor limit (0-50, default 5); the requested key is always included.
    #[serde(default = "default_context_count")]
    pub count: usize,
}

/// Read current context and terminology from one captured workflow snapshot.
pub(crate) async fn handle_get_context(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    glossary_path: &Path,
    params: GetContextParams,
) -> Result<serde_json::Value, XcStringsError> {
    let snapshot = resolve_read_snapshot(store, cache, params.file_path.as_deref()).await?;
    let view = workflow::inspect(
        snapshot.identity_text()?,
        &snapshot.catalog,
        &snapshot.workflow,
    )?;
    let guidance = GuidanceSnapshot::load(store, glossary_path);
    let mut package = context::build_context_package(
        &view.effective_catalog,
        &snapshot.workflow.contexts,
        guidance.document.as_ref(),
        &params.key,
        &params.locale,
        params.count,
    )?;
    annotate_unit(&snapshot, &view, &mut package.current)?;
    for neighbor in &mut package.neighbors {
        annotate_unit(&snapshot, &view, &mut neighbor.unit)?;
    }
    let destinations: Vec<_> = package
        .current
        .leaves
        .iter()
        .map(|leaf| TranslationDestination {
            key: params.key.clone(),
            locale: params.locale.clone(),
            path: leaf.path.clone(),
        })
        .collect();
    let mut result = read_result(package, &snapshot, &view)?;
    result["guidance"] = serde_json::to_value(guidance.check_destinations(
        &snapshot.catalog,
        &snapshot.workflow.contexts,
        &destinations,
    ))?;
    // Raw selected record allows get-modify-set without dropping unknown authored fields.
    result["authored_contexts"] = serde_json::to_value(
        snapshot
            .workflow
            .contexts
            .iter()
            .filter(|(key, _)| *key == &params.key)
            .collect::<std::collections::BTreeMap<_, _>>(),
    )?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::test_helpers::MemoryStore;

    const PLURALS_FIXTURE: &str = include_str!("../../tests/fixtures/with_plurals.xcstrings");
    const SIMPLE_FIXTURE: &str = include_str!("../../tests/fixtures/simple.xcstrings");

    #[tokio::test]
    async fn test_get_plurals_success() {
        let store = MemoryStore::new();
        store.add_file("/test/plurals.xcstrings", PLURALS_FIXTURE);
        let cache = Mutex::new(FileCache::new());

        let params = GetPluralsParams {
            file_path: Some("/test/plurals.xcstrings".to_string()),
            locale: "de".to_string(),
            batch_size: 20,
            offset: 0,
        };
        let result = handle_get_plurals(&store, &cache, params).await.unwrap();
        assert!(result["total"].as_u64().unwrap() > 0);
    }

    #[tokio::test]
    async fn test_get_plurals_empty() {
        let store = MemoryStore::new();
        store.add_file("/test/simple.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());

        let params = GetPluralsParams {
            file_path: Some("/test/simple.xcstrings".to_string()),
            locale: "de".to_string(),
            batch_size: 20,
            offset: 0,
        };
        let result = handle_get_plurals(&store, &cache, params).await.unwrap();
        assert_eq!(result["total"], 0);
    }

    #[tokio::test]
    async fn test_get_context_success() {
        let store = MemoryStore::new();
        store.add_file("/test/simple.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());

        let params = GetContextParams {
            file_path: Some("/test/simple.xcstrings".to_string()),
            key: "greeting".to_string(),
            locale: "uk".to_string(),
            count: 5,
        };
        let result = handle_get_context(&store, &cache, Path::new("/glossary.json"), params)
            .await
            .unwrap();
        assert_eq!(result["current"]["key"], "greeting");
        assert_eq!(result["guidance"]["status"], "absent");
        assert_eq!(result["tracking"], "uninitialized");
    }

    #[tokio::test]
    async fn test_get_context_missing_key() {
        let store = MemoryStore::new();
        store.add_file("/test/simple.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());

        let params = GetContextParams {
            file_path: Some("/test/simple.xcstrings".to_string()),
            key: "nonexistent".to_string(),
            locale: "uk".to_string(),
            count: 5,
        };
        let result = handle_get_context(&store, &cache, Path::new("/glossary.json"), params).await;
        assert!(matches!(result, Err(XcStringsError::KeyNotFound(key)) if key == "nonexistent"));
    }
}

#[cfg(test)]
mod context_workflow_contract_tests {
    use super::*;
    use crate::tools::test_helpers::MemoryStore;
    use serde_json::json;

    #[tokio::test]
    async fn zero_neighbor_context_returns_requested_key_and_review_version() {
        let store = MemoryStore::new();
        store.add_file("/test/catalog.xcstrings",&json!({"sourceLanguage":"en","version":"1.0","strings":{"screen.title":{"comment":"Screen heading","localizations":{"en":{"stringUnit":{"state":"translated","value":"Settings"}},"fr":{"stringUnit":{"state":"needs_review","value":"Réglages"}}}}}}).to_string());
        let result = handle_get_context(
            &store,
            &Mutex::new(FileCache::new()),
            Path::new("/glossary.json"),
            GetContextParams {
                file_path: Some("/test/catalog.xcstrings".into()),
                key: "screen.title".into(),
                locale: "fr".into(),
                count: 0,
            },
        )
        .await
        .unwrap();
        assert_eq!(result["current"]["key"], "screen.title");
        assert_eq!(result["current"]["comment"], "Screen heading");
        assert_eq!(
            result["current"]["leaves"][0]["workflow"]["native_state"],
            "needs_review"
        );
        assert_eq!(result["tracking"], "uninitialized");
        assert_eq!(result["neighbors"], json!([]));
    }
}
