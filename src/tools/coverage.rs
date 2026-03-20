use schemars::JsonSchema;
use serde::Deserialize;
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::service::{coverage, file_validator};
use crate::tools::parse::CachedFile;
use crate::tools::resolve_file;

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetCoverageParams {
    /// Path to .xcstrings file (optional if already parsed)
    #[serde(default)]
    pub file_path: Option<String>,
}

pub(crate) async fn handle_get_coverage(
    store: &dyn FileStore,
    cache: &Mutex<Option<CachedFile>>,
    params: GetCoverageParams,
) -> Result<serde_json::Value, XcStringsError> {
    let (_path, file) = resolve_file(store, cache, params.file_path.as_deref()).await?;
    let report = coverage::get_coverage(&file);
    Ok(serde_json::to_value(report)?)
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
    cache: &Mutex<Option<CachedFile>>,
    params: ValidateFileParams,
) -> Result<serde_json::Value, XcStringsError> {
    let (_path, file) = resolve_file(store, cache, params.file_path.as_deref()).await?;
    let reports = file_validator::validate_file(&file, params.locale.as_deref());
    Ok(serde_json::to_value(reports)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::test_helpers::{MemoryStore, SIMPLE_FIXTURE};

    #[tokio::test]
    async fn test_get_coverage_returns_data() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(None);

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
        let cache = Mutex::new(None);

        let params = ValidateFileParams {
            file_path: Some("/test/file.xcstrings".to_string()),
            locale: Some("uk".to_string()),
        };
        let result = handle_validate_file(&store, &cache, params).await.unwrap();

        let reports = result.as_array().unwrap();
        assert_eq!(reports.len(), 1);
        assert!(reports[0]["errors"].as_array().unwrap().is_empty());
    }
}
