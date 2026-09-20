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
            source_locale: Some("en".to_string()),
            target_locale: Some("uk".to_string()),
            entries: Some(entries),
            expected_revision: Some(byte_revision(None)),
            ..Default::default()
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
            source_locale: Some("en".to_string()),
            target_locale: Some("de".to_string()),
            entries: Some(entries),
            expected_revision: Some(byte_revision(None)),
            ..Default::default()
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

#[cfg(test)]
mod workflow_contract_tests {
    use super::*;
    use crate::tools::test_helpers::MemoryStore;
    use serde_json::json;

    #[tokio::test]
    async fn glossary_dry_run_does_not_create_policy_file() {
        let store = MemoryStore::new();
        let path = Path::new("/glossary.json");
        let params:UpdateGlossaryParams=serde_json::from_value(json!({"source_locale":"en","target_locale":"fr","entries":{"Account":"Compte"},"dry_run":true})).unwrap();
        let result = handle_update_glossary(&store, path, &Mutex::new(()), params)
            .await
            .unwrap();
        assert_eq!(store.get_content(path), None);
        assert_eq!(result["written"], false);
        assert_eq!(result["dry_run"], true);
    }

    #[tokio::test]
    async fn glossary_apply_requires_captured_revision() {
        let store = MemoryStore::new();
        let path = Path::new("/glossary.json");
        let params: UpdateGlossaryParams = serde_json::from_value(
            json!({"source_locale":"en","target_locale":"fr","entries":{"Account":"Compte"}}),
        )
        .unwrap();
        let result = handle_update_glossary(&store, path, &Mutex::new(()), params).await;
        assert!(
            matches!(result,Err(XcStringsError::GlossaryError(ref message)) if message.contains("requires expected_revision"))
        );
        assert_eq!(store.get_content(path), None);
    }
}

#[tokio::test]
async fn rich_policy_edit_preserves_unrelated_rules_and_unknown_metadata() {
    let store = MemoryStore::new();
    let path = Path::new("/glossary.json");
    let raw=serde_json::json!({"schema_version":2,"vendor":{"ordered":[3,1]},"terms":[{"id":"existing","source_locale":"en","target_locale":"fr","source":"Open","preferred":["Ouvrir"],"scope":{"roles":["button"]},"vendor":{"note":"keep"}}]}).to_string();
    store.add_file(path, &raw);
    let params:UpdateGlossaryParams=serde_json::from_value(serde_json::json!({"source_locale":"en","target_locale":"fr","entries":{"Account":"Compte"},"expected_revision":byte_revision(Some(raw.as_bytes()))})).unwrap();
    let result = handle_update_glossary(&store, path, &Mutex::new(()), params)
        .await
        .unwrap();
    assert_eq!(result["written"], true);
    let actual: Value = serde_json::from_str(&store.get_content(path).unwrap()).unwrap();
    assert_eq!(actual["vendor"], serde_json::json!({"ordered":[3,1]}));
    assert_eq!(
        actual["terms"][0]["vendor"],
        serde_json::json!({"note":"keep"})
    );
    assert_eq!(
        actual["terms"][0]["scope"]["roles"],
        serde_json::json!(["button"])
    );
    assert_eq!(actual["terms"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn stale_glossary_revision_preserves_external_edit() {
    let store = MemoryStore::new();
    let path = Path::new("/glossary.json");
    let before = r#"{"en→fr":{"Account":"Compte"}}"#;
    let after = r#"{"en→fr":{"Account":"Profil"}}"#;
    store.add_file(path, after);
    let params:UpdateGlossaryParams=serde_json::from_value(serde_json::json!({"source_locale":"en","target_locale":"fr","entries":{"Account":"Dossier"},"expected_revision":byte_revision(Some(before.as_bytes()))})).unwrap();
    let result = handle_update_glossary(&store, path, &Mutex::new(()), params).await;
    assert!(
        matches!(result,Err(XcStringsError::GlossaryError(message)) if message=="stale_glossary_revision")
    );
    assert_eq!(store.get_content(path).as_deref(), Some(after));
}

#[tokio::test]
async fn malformed_glossary_is_unavailable_on_read_and_never_overwritten() {
    let store = MemoryStore::new();
    let path = Path::new("/glossary.json");
    store.add_file(path, "{bad");
    let read = handle_get_glossary(
        &store,
        path,
        GetGlossaryParams {
            source_locale: "en".into(),
            target_locale: "fr".into(),
            filter: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(read["status"], "unavailable");
    assert_eq!(read["unavailable"]["code"], "glossary_parse_unavailable");
    let params:UpdateGlossaryParams=serde_json::from_value(serde_json::json!({"source_locale":"en","target_locale":"fr","entries":{"Account":"Compte"},"dry_run":true})).unwrap();
    assert!(matches!(
        handle_update_glossary(&store, path, &Mutex::new(()), params).await,
        Err(XcStringsError::GlossaryError(_))
    ));
    assert_eq!(store.get_content(path).as_deref(), Some("{bad"));
}

#[tokio::test]
async fn empty_rich_edit_and_mixed_edit_modes_are_explicit_errors() {
    let store = MemoryStore::new();
    let path = Path::new("/glossary.json");
    let empty = UpdateGlossaryParams {
        dry_run: true,
        ..Default::default()
    };
    assert!(
        matches!(handle_update_glossary(&store,path,&Mutex::new(()),empty).await,Err(XcStringsError::GlossaryError(message)) if message=="empty_glossary_edit")
    );
    let mixed: UpdateGlossaryParams =
        serde_json::from_value(serde_json::json!({"entries":{},"remove_ids":["a"],"dry_run":true}))
            .unwrap();
    assert!(
        matches!(handle_update_glossary(&store,path,&Mutex::new(()),mixed).await,Err(XcStringsError::GlossaryError(message)) if message.starts_with("conflicting_glossary_edit"))
    );
    assert_eq!(store.get_content(path), None);
}

#[tokio::test]
async fn invalid_rich_candidate_returns_diagnostic_without_write() {
    let store = MemoryStore::new();
    let path = Path::new("/glossary.json");
    let params:UpdateGlossaryParams=serde_json::from_value(serde_json::json!({"upsert":[{"id":"a","source_locale":"en","target_locale":"fr","source":"Account"}],"expected_revision":byte_revision(None)})).unwrap();
    let result = handle_update_glossary(&store, path, &Mutex::new(()), params)
        .await
        .unwrap();
    assert_eq!(result["written"], false);
    assert_eq!(result["rejected"][0]["code"], "empty_rule");
    assert_eq!(store.get_content(path), None);
}

struct PolicyStore {
    inner: MemoryStore,
    replace_on_read: std::sync::Mutex<Option<String>>,
    alias: bool,
    fail_read: bool,
}
impl FileStore for PolicyStore {
    fn file_identity(&self, path: &Path) -> Result<PathBuf, XcStringsError> {
        if self.alias && path == Path::new("/glossary.json") {
            Ok("/other.json".into())
        } else {
            Ok(path.into())
        }
    }
    fn read(&self, path: &Path) -> Result<String, XcStringsError> {
        self.inner.read(path)
    }
    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, XcStringsError> {
        if self.fail_read {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "policy read denied",
            )
            .into());
        }
        let bytes = self.inner.read_bytes(path)?;
        if let Some(replacement) = self.replace_on_read.lock().unwrap().take() {
            self.inner.update_file(path, &replacement);
        }
        Ok(bytes)
    }
    fn write(&self, path: &Path, content: &str) -> Result<(), XcStringsError> {
        self.inner.write(path, content)
    }
    fn write_if_matches(
        &self,
        path: &Path,
        expected: Option<&[u8]>,
        content: &str,
    ) -> Result<(), XcStringsError> {
        self.inner.write_if_matches(path, expected, content)
    }
    fn modified_time(&self, path: &Path) -> Result<std::time::SystemTime, XcStringsError> {
        self.inner.modified_time(path)
    }
    fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path)
    }
    fn create_parent_dirs(&self, path: &Path) -> Result<(), XcStringsError> {
        self.inner.create_parent_dirs(path)
    }
}

#[tokio::test]
async fn policy_changed_after_read_causes_cas_conflict_without_lost_update() {
    let before = r#"{"en→fr":{"Account":"Compte"}}"#;
    let after = r#"{"en→fr":{"Account":"Profil externe"}}"#;
    let inner = MemoryStore::new();
    inner.add_file("/glossary.json", before);
    let store = PolicyStore {
        inner,
        replace_on_read: std::sync::Mutex::new(Some(after.into())),
        alias: false,
        fail_read: false,
    };
    let params:UpdateGlossaryParams=serde_json::from_value(serde_json::json!({"source_locale":"en","target_locale":"fr","entries":{"Account":"Dossier"},"expected_revision":byte_revision(Some(before.as_bytes()))})).unwrap();
    let result =
        handle_update_glossary(&store, Path::new("/glossary.json"), &Mutex::new(()), params).await;
    assert!(matches!(
        result,
        Err(XcStringsError::ConditionalWriteConflict { .. })
    ));
    assert_eq!(
        store
            .inner
            .get_content(Path::new("/glossary.json"))
            .as_deref(),
        Some(after)
    );
}

#[tokio::test]
async fn policy_alias_is_unavailable_and_cannot_redirect_glossary_write() {
    let inner = MemoryStore::new();
    inner.add_file("/other.json", "protected");
    let store = PolicyStore {
        inner,
        replace_on_read: std::sync::Mutex::new(None),
        alias: true,
        fail_read: false,
    };
    let report = handle_get_glossary(
        &store,
        Path::new("/glossary.json"),
        GetGlossaryParams {
            source_locale: "en".into(),
            target_locale: "fr".into(),
            filter: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(report["status"], "unavailable");
    assert_eq!(report["unavailable"]["code"], "glossary_path_unavailable");
    let params:UpdateGlossaryParams=serde_json::from_value(serde_json::json!({"source_locale":"en","target_locale":"fr","entries":{"Account":"Compte"},"dry_run":true})).unwrap();
    assert!(matches!(
        handle_update_glossary(&store, Path::new("/glossary.json"), &Mutex::new(()), params).await,
        Err(XcStringsError::GlossaryError(_))
    ));
    assert_eq!(
        store.inner.get_content(Path::new("/other.json")).as_deref(),
        Some("protected")
    );
}

#[tokio::test]
async fn unreadable_policy_reports_unknown_revision_not_empty_guidance() {
    let inner = MemoryStore::new();
    inner.add_file("/glossary.json", "present but unreadable");
    let store = PolicyStore {
        inner,
        replace_on_read: std::sync::Mutex::new(None),
        alias: false,
        fail_read: true,
    };
    let report = handle_get_glossary(
        &store,
        Path::new("/glossary.json"),
        GetGlossaryParams {
            source_locale: "en".into(),
            target_locale: "fr".into(),
            filter: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(report["status"], "unavailable");
    assert_eq!(report["revision"], Value::Null);
    assert_eq!(report["unavailable"]["code"], "glossary_read_unavailable");
}
