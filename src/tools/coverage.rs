use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::service::coverage;
use crate::tools::FileCache;
use crate::tools::workflow::{read_result, resolve_read_snapshot};

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetCoverageParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
}

pub(crate) async fn handle_get_coverage(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    params: GetCoverageParams,
) -> Result<serde_json::Value, XcStringsError> {
    let snapshot = resolve_read_snapshot(store, cache, params.file_path.as_deref()).await?;
    let view = snapshot.view()?;
    let report = coverage::get_coverage(&view.effective_catalog);
    read_result(report, &snapshot, &view)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ValidateFileParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Specific locale to validate (validates all non-source locales if omitted)
    #[serde(default)]
    pub locale: Option<String>,
}

pub(crate) async fn handle_validate_file(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    glossary_path: &std::path::Path,
    params: ValidateFileParams,
) -> Result<serde_json::Value, XcStringsError> {
    let snapshot = resolve_read_snapshot(store, cache, params.file_path.as_deref()).await?;
    let guidance = crate::guidance_operation::GuidanceSnapshot::load(store, glossary_path);
    let report = crate::workflow_operation::read::validate_catalog(
        &snapshot,
        &guidance,
        params.locale.as_deref(),
    )?;
    Ok(serde_json::to_value(report)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::test_helpers::{MemoryStore, SIMPLE_FIXTURE};

    #[tokio::test]
    async fn test_get_coverage_returns_data() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());

        let params = GetCoverageParams {
            file_path: Some("/test/file.xcstrings".to_string()),
        };
        let result = handle_get_coverage(&store, &cache, params).await.unwrap();

        assert_eq!(result["source_language"], "en");
        assert_eq!(result["total_keys"], 2);
        assert_eq!(result["translatable_keys"], 2);
        assert!(!result["locales"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_validate_file_clean() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());

        let params = ValidateFileParams {
            file_path: Some("/test/file.xcstrings".to_string()),
            locale: Some("uk".to_string()),
        };
        let result = handle_validate_file(
            &store,
            &cache,
            std::path::Path::new("/glossary.json"),
            params,
        )
        .await
        .unwrap();

        let reports = result["reports"].as_array().unwrap();
        assert_eq!(reports.len(), 1);
        assert!(reports[0]["errors"].as_array().unwrap().is_empty());
    }
}
