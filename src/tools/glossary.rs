use std::collections::BTreeMap;
use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::service::glossary::{self, Glossary};

/// Load glossary from disk via FileStore. Returns empty glossary if file doesn't exist.
fn load_glossary(store: &dyn FileStore, path: &Path) -> Result<Glossary, XcStringsError> {
    if !store.exists(path) {
        return glossary::parse_glossary(None);
    }
    let raw = store.read(path)?;
    glossary::parse_glossary(Some(&raw))
}

/// Save glossary to disk via FileStore, creating parent directories if needed.
fn save_glossary(
    store: &dyn FileStore,
    path: &Path,
    data: &Glossary,
) -> Result<(), XcStringsError> {
    store.create_parent_dirs(path)?;
    let json = glossary::serialize_glossary(data)?;
    store.write(path, &json)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct GetGlossaryParams {
    /// Source locale (e.g. "en")
    pub source_locale: String,
    /// Target locale (e.g. "uk")
    pub target_locale: String,
    /// Optional substring filter (matches both keys and values, case-insensitive)
    #[serde(default)]
    pub filter: Option<String>,
}

#[derive(Debug, Serialize)]
struct GlossaryResult {
    source_locale: String,
    target_locale: String,
    entries: BTreeMap<String, String>,
    count: usize,
}

pub(crate) async fn handle_get_glossary(
    store: &dyn FileStore,
    glossary_path: &Path,
    params: GetGlossaryParams,
) -> Result<serde_json::Value, XcStringsError> {
    let glossary_data = load_glossary(store, glossary_path)?;
    let entries = glossary::get_entries(
        &glossary_data,
        &params.source_locale,
        &params.target_locale,
        params.filter.as_deref(),
    );
    let count = entries.len();
    let result = GlossaryResult {
        source_locale: params.source_locale,
        target_locale: params.target_locale,
        entries,
        count,
    };
    Ok(serde_json::to_value(result)?)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct UpdateGlossaryParams {
    /// Source locale (e.g. "en")
    pub source_locale: String,
    /// Target locale (e.g. "uk")
    pub target_locale: String,
    /// Map of source term to translation (upserts)
    pub entries: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct UpdateGlossaryResult {
    updated: usize,
    source_locale: String,
    target_locale: String,
}

pub(crate) async fn handle_update_glossary(
    store: &dyn FileStore,
    glossary_path: &Path,
    write_lock: &Mutex<()>,
    params: UpdateGlossaryParams,
) -> Result<serde_json::Value, XcStringsError> {
    let _guard = write_lock.lock().await;
    let mut glossary_data = load_glossary(store, glossary_path)?;
    let count = glossary::update_entries(
        &mut glossary_data,
        &params.source_locale,
        &params.target_locale,
        params.entries,
    );
    save_glossary(store, glossary_path, &glossary_data)?;
    let result = UpdateGlossaryResult {
        updated: count,
        source_locale: params.source_locale,
        target_locale: params.target_locale,
    };
    Ok(serde_json::to_value(result)?)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::tools::test_helpers::MemoryStore;

    #[tokio::test]
    async fn handle_get_glossary_empty() {
        let store = MemoryStore::new();
        let path = PathBuf::from("/glossary.json");

        let result = handle_get_glossary(
            &store,
            &path,
            GetGlossaryParams {
                source_locale: "en".to_string(),
                target_locale: "uk".to_string(),
                filter: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(result["count"], 0);
        assert!(result["entries"].as_object().unwrap().is_empty());
    }

    #[tokio::test]
    async fn handle_update_then_get() {
        let store = MemoryStore::new();
        let path = PathBuf::from("/glossary.json");
        let write_lock = Mutex::new(());

        let mut entries = BTreeMap::new();
        entries.insert("Settings".to_string(), "Nalashtuvannya".to_string());

        let update_result = handle_update_glossary(
            &store,
            &path,
            &write_lock,
            UpdateGlossaryParams {
                source_locale: "en".to_string(),
                target_locale: "uk".to_string(),
                entries,
            },
        )
        .await
        .unwrap();

        assert_eq!(update_result["updated"], 1);

        let get_result = handle_get_glossary(
            &store,
            &path,
            GetGlossaryParams {
                source_locale: "en".to_string(),
                target_locale: "uk".to_string(),
                filter: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(get_result["count"], 1);
        assert_eq!(get_result["entries"]["Settings"], "Nalashtuvannya");
    }

    #[tokio::test]
    async fn handle_get_glossary_with_filter() {
        let store = MemoryStore::new();
        let path = PathBuf::from("/glossary.json");
        let write_lock = Mutex::new(());

        let mut entries = BTreeMap::new();
        entries.insert("Settings".to_string(), "Einstellungen".to_string());
        entries.insert("Cancel".to_string(), "Abbrechen".to_string());

        handle_update_glossary(
            &store,
            &path,
            &write_lock,
            UpdateGlossaryParams {
                source_locale: "en".to_string(),
                target_locale: "de".to_string(),
                entries,
            },
        )
        .await
        .unwrap();

        let result = handle_get_glossary(
            &store,
            &path,
            GetGlossaryParams {
                source_locale: "en".to_string(),
                target_locale: "de".to_string(),
                filter: Some("cancel".to_string()),
            },
        )
        .await
        .unwrap();

        assert_eq!(result["count"], 1);
        assert!(result["entries"]["Cancel"].as_str().is_some());
    }
}
