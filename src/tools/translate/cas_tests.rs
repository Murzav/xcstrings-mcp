use super::{SubmitTranslationsParams, captured_params, handle_submit_translations};
use crate::tools::{
    FileCache,
    files::handle_list_files,
    parse::{ParseParams, handle_parse},
    test_helpers::MemoryStore,
};
use crate::{error::XcStringsError, io::FileStore};
use serde_json::json;
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::SystemTime,
};
use tokio::sync::Mutex;

const A: &str = r#"{"sourceLanguage":"en","strings":{"k":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Source A"}}}}},"version":"1.0"}"#;
const B: &str = r#"{"sourceLanguage":"en","strings":{"k":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Source B"}}}}},"version":"1.0"}"#;
const EXTERNAL: &str = r#"{"sourceLanguage":"en","strings":{"external":{}},"version":"1.0"}"#;
struct Store {
    inner: MemoryStore,
    conflict: bool,
    metadata_failure: bool,
    wrote: AtomicBool,
    require_bom: bool,
}
impl FileStore for Store {
    fn read(&self, path: &Path) -> Result<String, XcStringsError> {
        self.inner.read(path)
    }
    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, XcStringsError> {
        self.inner.read_bytes(path)
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
        if self.require_bom {
            assert!(expected.unwrap().starts_with(&[0xef, 0xbb, 0xbf]));
        }
        if self.conflict {
            self.inner.update_file(path, EXTERNAL);
        }
        self.inner.write_if_matches(path, expected, content)?;
        self.wrote.store(true, Ordering::SeqCst);
        Ok(())
    }
    fn write_if_inputs_match(
        &self,
        path: &Path,
        expected: Option<&[u8]>,
        inputs: &[crate::io::FilePrecondition<'_>],
        content: &str,
    ) -> Result<(), XcStringsError> {
        if self.require_bom {
            assert!(expected.unwrap().starts_with(&[0xef, 0xbb, 0xbf]));
        }
        if self.conflict {
            self.inner.update_file(path, EXTERNAL);
        }
        self.inner
            .write_if_inputs_match(path, expected, inputs, content)?;
        self.wrote.store(true, Ordering::SeqCst);
        Ok(())
    }
    fn modified_time(&self, path: &Path) -> Result<SystemTime, XcStringsError> {
        if self.metadata_failure && self.wrote.load(Ordering::SeqCst) {
            return Err(std::io::Error::other("metadata unavailable after commit").into());
        }
        self.inner.modified_time(path)
    }
    fn exists(&self, path: &Path) -> bool {
        self.inner.exists(path)
    }
    fn create_parent_dirs(&self, path: &Path) -> Result<(), XcStringsError> {
        self.inner.create_parent_dirs(path)
    }
}
async fn setup() -> (Store, Mutex<FileCache>) {
    let store = Store {
        inner: MemoryStore::new(),
        conflict: false,
        metadata_failure: false,
        wrote: AtomicBool::new(false),
        require_bom: false,
    };
    store.inner.add_file("/test/A.xcstrings", A);
    store.inner.add_file("/test/B.xcstrings", B);
    let cache = Mutex::new(FileCache::new());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/A.xcstrings".into(),
        },
    )
    .await
    .unwrap();
    (store, cache)
}
fn params(dry_run: bool) -> SubmitTranslationsParams {
    SubmitTranslationsParams {
        file_path: Some("/test/B.xcstrings".into()),
        dry_run,
        continue_on_error: true,
        translations: serde_json::from_value(
            json!([{"expected_source_version":"","key":"k","locale":"de","path":[],"value":"Quelle B"}]),
        )
        .unwrap(),
    }
}
async fn assert_original_cache(cache: &Mutex<FileCache>) {
    assert_eq!(
        handle_list_files(cache).await.unwrap(),
        json!([{"path":"/test/A.xcstrings","source_language":"en","total_keys":1,"is_active":true}])
    );
}
#[tokio::test]
async fn concurrent_external_edit_is_not_overwritten_and_cache_is_unchanged() {
    let (mut store, cache) = setup().await;
    store.conflict = true;
    let error = handle_submit_translations(
        &store,
        &cache,
        &Mutex::new(()),
        Path::new("/test/glossary.json"),
        captured_params(&store, params(false)),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        XcStringsError::ConditionalWriteConflict {
            expected_exists: true,
            actual_exists: true,
            ..
        }
    ));
    assert_eq!(
        store.inner.read(Path::new("/test/B.xcstrings")).unwrap(),
        EXTERNAL
    );
    assert_original_cache(&cache).await;
}
#[tokio::test]
async fn dry_native_submit_keeps_active_file_and_cache_unchanged() {
    let (store, cache) = setup().await;
    let result = handle_submit_translations(
        &store,
        &cache,
        &Mutex::new(()),
        Path::new("/test/glossary.json"),
        captured_params(&store, params(true)),
    )
    .await
    .unwrap();
    assert_eq!(result["accepted"], 1);
    assert_eq!(result["dry_run"], true);
    assert_eq!(store.inner.read(Path::new("/test/B.xcstrings")).unwrap(), B);
    assert_original_cache(&cache).await;
}
#[tokio::test]
async fn native_conditional_write_compares_exact_bom_bytes() {
    let (mut store, cache) = setup().await;
    store.require_bom = true;
    store
        .inner
        .update_file("/test/B.xcstrings", &format!("\u{feff}{B}"));
    let result = handle_submit_translations(
        &store,
        &cache,
        &Mutex::new(()),
        Path::new("/test/glossary.json"),
        captured_params(&store, params(false)),
    )
    .await
    .unwrap();
    assert_eq!(result["accepted"], 1);
    let updated = store.inner.read(Path::new("/test/B.xcstrings")).unwrap();
    assert!(!updated.starts_with('\u{feff}'));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&updated).unwrap()["strings"]["k"]["localizations"]
            ["de"]["stringUnit"]["value"],
        "Quelle B"
    );
}
#[tokio::test]
async fn native_commit_survives_cache_metadata_failure() {
    let (mut store, cache) = setup().await;
    store.metadata_failure = true;
    let result = handle_submit_translations(
        &store,
        &cache,
        &Mutex::new(()),
        Path::new("/test/glossary.json"),
        captured_params(&store, params(false)),
    )
    .await
    .unwrap();
    assert_eq!(result["accepted"], 1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            &store.inner.read(Path::new("/test/B.xcstrings")).unwrap()
        )
        .unwrap()["strings"]["k"]["localizations"]["de"]["stringUnit"]["value"],
        "Quelle B"
    );
    assert_original_cache(&cache).await;
}

async fn combined_orphan_batch(dry_run: bool) {
    let (store, cache) = setup().await;
    let node = json!({"substitutions":{"N":{"argNum":1,"formatSpecifier":"lld","variations":{"plural":{"one":{"stringUnit":{"state":"translated","value":"%arg item"}},"other":{"stringUnit":{"state":"translated","value":"%arg items"}}}}}},"variations":{"device":{"iphone":{"stringUnit":{"state":"translated","value":"%#@N@ phone"}},"ipad":{"stringUnit":{"state":"translated","value":"%#@N@ tablet"}},"other":{"stringUnit":{"state":"translated","value":"%lld other"}}}}});
    let catalog = json!({"sourceLanguage":"en","strings":{"k":{"localizations":{"en":node,"de":node}}},"version":"1.0"}).to_string();
    store.inner.update_file("/test/B.xcstrings", &catalog);
    let mut request = params(dry_run);
    request.continue_on_error = false;
    request.translations = serde_json::from_value(json!([{"expected_source_version":"","key":"k","locale":"de","path":[{"device":"iphone"}],"value":"%lld Handy"},{"expected_source_version":"","key":"k","locale":"de","path":[{"device":"ipad"}],"value":"%lld Tablet"}])).unwrap();
    let parsed = crate::service::parser::parse(&catalog).unwrap();
    assert!(
        crate::service::validator::validate_translations(&parsed, &request.translations).is_empty()
    );

    let result = handle_submit_translations(
        &store,
        &cache,
        &Mutex::new(()),
        Path::new("/test/glossary.json"),
        captured_params(&store, request),
    )
    .await
    .unwrap();

    assert_eq!(result["accepted"], 0);
    assert_eq!(result["accepted_destinations"], json!([]));
    assert_eq!(result["rejected"].as_array().unwrap().len(), 2);
    assert_eq!(result["dry_run"], dry_run);
    assert_eq!(
        store.inner.read(Path::new("/test/B.xcstrings")).unwrap(),
        catalog
    );
    assert_original_cache(&cache).await;
}
#[tokio::test]
async fn combined_invariant_failure_cancels_entire_apply_batch() {
    combined_orphan_batch(false).await;
}
#[tokio::test]
async fn combined_invariant_failure_cancels_entire_dry_batch() {
    combined_orphan_batch(true).await;
}
