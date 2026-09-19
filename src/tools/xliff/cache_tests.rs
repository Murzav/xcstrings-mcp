use super::{ImportXliffParams, handle_import_xliff};
use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::tools::FileCache;
use crate::tools::extract::{GetKeyParams, handle_get_key};
use crate::tools::files::handle_list_files;
use crate::tools::parse::{ParseParams, handle_parse};
use crate::tools::test_helpers::MemoryStore;
use serde_json::json;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;
use tokio::sync::Mutex;

const A: &str = r#"{"sourceLanguage":"en","strings":{"greeting":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"A original"}}}}},"version":"1.0"}"#;
const B: &str = r#"{"sourceLanguage":"en","strings":{"greeting":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"B original"}}}}},"version":"1.0"}"#;
const VALID: &str = r#"<xliff version="1.2" xmlns="urn:oasis:names:tc:xliff:document:1.2"><file original="B.xcstrings" source-language="en" target-language="fr"><body><trans-unit id="greeting"><source>B original</source><target>Bonjour</target></trans-unit></body></file></xliff>"#;

struct Store {
    inner: MemoryStore,
    conflict: bool,
    fail_modified_after_write: bool,
    wrote: AtomicBool,
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
        if self.conflict {
            return Err(XcStringsError::ConditionalWriteConflict {
                path: path.to_path_buf(),
                expected_exists: true,
                actual_exists: true,
            });
        }
        self.inner.write_if_matches(path, expected, content)?;
        self.wrote.store(true, Ordering::SeqCst);
        Ok(())
    }
    fn modified_time(&self, path: &Path) -> Result<SystemTime, XcStringsError> {
        if self.fail_modified_after_write && self.wrote.load(Ordering::SeqCst) {
            return Err(std::io::Error::other("metadata unavailable after committed write").into());
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

async fn setup(xml: &str, conflict: bool) -> (Store, Mutex<FileCache>) {
    let store = Store {
        inner: MemoryStore::new(),
        conflict,
        fail_modified_after_write: false,
        wrote: AtomicBool::new(false),
    };
    store.inner.add_file("/test/A.xcstrings", A);
    store.inner.add_file("/test/B.xcstrings", B);
    store.inner.add_file("/test/input.xliff", xml);
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
    assert_unchanged(&store, &cache).await;
    (store, cache)
}

fn params(dry_run: bool) -> ImportXliffParams {
    ImportXliffParams {
        file_path: Some("/test/B.xcstrings".into()),
        original: None,
        xliff_path: "/test/input.xliff".into(),
        dry_run,
    }
}

async fn assert_unchanged(store: &Store, cache: &Mutex<FileCache>) {
    assert_eq!(
        handle_list_files(cache).await.unwrap(),
        json!([{"path":"/test/A.xcstrings", "source_language":"en", "total_keys":1, "is_active":true}])
    );
    let implicit = handle_get_key(
        store,
        cache,
        GetKeyParams {
            file_path: None,
            key: "greeting".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(implicit["source_text"], "A original");
    assert_eq!(implicit["translations"][0]["value"], "A original");
    assert_eq!(store.read(Path::new("/test/A.xcstrings")).unwrap(), A);
    assert_eq!(store.read(Path::new("/test/B.xcstrings")).unwrap(), B);
}

#[tokio::test]
async fn malformed_import_keeps_previous_active_file_and_cache_contents() {
    let (store, cache) = setup("<xliff>", false).await;
    let error = handle_import_xliff(&store, &cache, &Mutex::new(()), params(false))
        .await
        .unwrap_err();
    assert!(matches!(error, XcStringsError::XliffParse(_)));
    assert_unchanged(&store, &cache).await;
}

#[tokio::test]
async fn rejected_import_keeps_previous_active_file_and_cache_contents() {
    let (store, cache) = setup(&VALID.replace("id=\"greeting\"", "id=\"missing\""), false).await;
    let result = handle_import_xliff(&store, &cache, &Mutex::new(()), params(false))
        .await
        .unwrap();
    assert_eq!(result["accepted"], 0);
    assert_eq!(result["written"], false);
    assert_eq!(result["rejected"].as_array().unwrap().len(), 1);
    assert_eq!(result["rejected"][0]["code"], "unknown_key");
    assert_unchanged(&store, &cache).await;
}

#[tokio::test]
async fn dry_import_keeps_previous_active_file_and_cache_contents() {
    let (store, cache) = setup(VALID, false).await;
    let result = handle_import_xliff(&store, &cache, &Mutex::new(()), params(true))
        .await
        .unwrap();
    assert_eq!(result["accepted"], 1);
    assert_eq!(result["written"], false);
    assert_eq!(result["dry_run"], true);
    assert_eq!(result["rejected"], json!([]));
    assert_unchanged(&store, &cache).await;
}

#[tokio::test]
async fn conflicting_import_keeps_previous_active_file_and_cache_contents() {
    let (store, cache) = setup(VALID, true).await;
    let error = handle_import_xliff(&store, &cache, &Mutex::new(()), params(false))
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
    assert_unchanged(&store, &cache).await;
}

#[tokio::test]
async fn committed_import_survives_metadata_failure_and_invalidates_stale_cache() {
    let (mut store, cache) = setup(VALID, false).await;
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: "/test/B.xcstrings".into(),
        },
    )
    .await
    .unwrap();
    store.fail_modified_after_write = true;
    let result = handle_import_xliff(&store, &cache, &Mutex::new(()), params(false))
        .await
        .unwrap();
    assert_eq!(result["written"], true);
    assert_eq!(result["accepted"], 1);
    assert_eq!(result["rejected"], json!([]));
    let written: serde_json::Value =
        serde_json::from_str(&store.read(Path::new("/test/B.xcstrings")).unwrap()).unwrap();
    assert_eq!(
        written["strings"]["greeting"]["localizations"]["fr"]["stringUnit"],
        json!({"state":"translated","value":"Bonjour"})
    );
    assert_eq!(
        handle_list_files(&cache).await.unwrap(),
        json!([{"path":"/test/A.xcstrings","source_language":"en","total_keys":1,"is_active":false}])
    );
    assert!(cache.lock().await.active_path().is_none());
}
