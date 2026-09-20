use super::extract::{GetKeyParams, handle_get_key};
use super::files::handle_list_files;
use super::parse::{ParseParams, handle_parse};
use super::test_helpers::MemoryStore;
use super::xliff::{ImportXliffParams, import_with_current_checkpoint as handle_import_xliff};
use super::*;
use serde_json::json;
use std::path::Path;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::SystemTime;

const ALIAS: &str = "/test/alias.xcstrings";
const FIRST: &str = "/test/first.xcstrings";
const SECOND: &str = "/test/second.xcstrings";
struct AliasStore {
    inner: MemoryStore,
    target: AtomicU8,
}
impl FileStore for AliasStore {
    fn file_identity(&self, path: &Path) -> Result<PathBuf, XcStringsError> {
        if path != Path::new(ALIAS) {
            return Ok(path.to_path_buf());
        }
        match self.target.load(Ordering::SeqCst) {
            0 => Ok(FIRST.into()),
            1 => Ok(SECOND.into()),
            _ => Err(XcStringsError::InvalidPath {
                path: path.to_path_buf(),
                reason: "alias cannot be resolved".into(),
            }),
        }
    }
    fn read(&self, path: &Path) -> Result<String, XcStringsError> {
        self.inner.read(&self.file_identity(path)?)
    }
    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, XcStringsError> {
        self.inner.read_bytes(&self.file_identity(path)?)
    }
    fn write(&self, path: &Path, content: &str) -> Result<(), XcStringsError> {
        self.inner.write(&self.file_identity(path)?, content)
    }
    fn write_if_matches(
        &self,
        path: &Path,
        expected: Option<&[u8]>,
        content: &str,
    ) -> Result<(), XcStringsError> {
        self.inner
            .write_if_matches(&self.file_identity(path)?, expected, content)
    }
    fn write_if_inputs_match(
        &self,
        path: &Path,
        expected: Option<&[u8]>,
        inputs: &[crate::io::FilePrecondition<'_>],
        content: &str,
    ) -> Result<(), XcStringsError> {
        let identities = inputs
            .iter()
            .map(|input| self.file_identity(input.path))
            .collect::<Result<Vec<_>, _>>()?;
        let guards = inputs
            .iter()
            .zip(&identities)
            .map(|(input, path)| crate::io::FilePrecondition {
                path,
                expected: input.expected,
            })
            .collect::<Vec<_>>();
        self.inner
            .write_if_inputs_match(&self.file_identity(path)?, expected, &guards, content)
    }
    fn modified_time(&self, _: &Path) -> Result<SystemTime, XcStringsError> {
        Ok(SystemTime::UNIX_EPOCH)
    }
    fn exists(&self, path: &Path) -> bool {
        self.file_identity(path)
            .is_ok_and(|p| self.inner.exists(&p))
    }
    fn create_parent_dirs(&self, _: &Path) -> Result<(), XcStringsError> {
        Ok(())
    }
}
async fn setup() -> (AliasStore, Mutex<FileCache>) {
    let store = AliasStore {
        inner: MemoryStore::new(),
        target: AtomicU8::new(0),
    };
    store.inner.add_file(FIRST,&json!({"sourceLanguage":"en","version":"1.0","strings":{"key":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"First source"}}}}}}).to_string());
    store.inner.add_file(SECOND,&json!({"sourceLanguage":"en","version":"1.0","strings":{"key":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Second source"}}}}}}).to_string());
    let cache = Mutex::new(FileCache::new());
    handle_parse(
        &store,
        &cache,
        ParseParams {
            file_path: ALIAS.into(),
        },
    )
    .await
    .unwrap();
    (store, cache)
}
#[tokio::test]
async fn implicit_read_after_import_refreshes_retargeted_alias_even_with_equal_mtime() {
    let (store, cache) = setup().await;
    store.inner.add_file("/test/input.xliff",r#"<xliff version="1.2"><file target-language="fr"><body><trans-unit id="key"><source>First source</source><target>First target</target></trans-unit></body></file></xliff>"#);
    let result = handle_import_xliff(
        &store,
        &cache,
        &Mutex::new(()),
        ImportXliffParams {
            expected_source_versions: Default::default(),
            file_path: Some(ALIAS.into()),
            original: None,
            xliff_path: "/test/input.xliff".into(),
            dry_run: false,
        },
    )
    .await
    .unwrap();
    assert_eq!(result["written"], true);
    store.target.store(1, Ordering::SeqCst);
    let result = handle_get_key(
        &store,
        &cache,
        GetKeyParams {
            file_path: None,
            key: "key".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(result["source_text"], "Second source");
    assert_eq!(
        handle_list_files(&cache).await.unwrap(),
        json!([{"path":ALIAS,"source_language":"en","total_keys":1,"is_active":true}])
    );
    let first: serde_json::Value =
        serde_json::from_str(&store.read(Path::new(FIRST)).unwrap()).unwrap();
    assert_eq!(
        first["strings"]["key"]["localizations"]["fr"]["stringUnit"]["value"],
        "First target"
    );
}
#[tokio::test]
async fn implicit_read_rejects_unresolvable_alias_instead_of_serving_stale_content() {
    let (store, cache) = setup().await;
    store.target.store(2, Ordering::SeqCst);
    let error = handle_get_key(
        &store,
        &cache,
        GetKeyParams {
            file_path: None,
            key: "key".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error,XcStringsError::InvalidPath { reason,.. } if reason=="alias cannot be resolved")
    );
}
