#![allow(dead_code)]
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};
use xcstrings_mcp::{
    error::XcStringsError,
    io::{FilePrecondition, FileStore},
    model::workflow::SyncMode,
    service::workflow,
    workflow_operation::CatalogSnapshot,
};
pub const PATH: &str = "/test/Catalog.xcstrings";
pub const SIDECAR: &str = "/test/Catalog.xcstrings.xcstrings-mcp.json";
pub const CATALOG: &str = r#"{"sourceLanguage":"en","version":"1.0","strings":{"k":{"localizations":{"en":{"stringUnit":{"state":"translated","value":"Delete"}}}}}}"#;
pub const EXTERNAL_CONTEXT: &str =
    r#"{"version":1,"contexts":{"k":{"context":{"purpose":"Changed externally"}}}}"#;
#[derive(Default)]
pub struct Store {
    pub files: Mutex<BTreeMap<PathBuf, Vec<u8>>>,
    pub context_conflict: bool,
    pub xml_conflict: bool,
    pub aliases: BTreeMap<PathBuf, PathBuf>,
}
impl Store {
    pub fn new() -> Self {
        let store = Self::default();
        store.put(PATH, CATALOG);
        store
    }
    pub fn put(&self, path: &str, value: &str) {
        self.files
            .lock()
            .unwrap()
            .insert(path.into(), value.as_bytes().into());
    }
    pub fn snapshot(&self) -> CatalogSnapshot {
        CatalogSnapshot::load(self, Path::new(PATH)).unwrap()
    }
    pub fn versions(&self) -> BTreeMap<String, String> {
        let snapshot = self.snapshot();
        workflow::inspect(
            snapshot.identity_text().unwrap(),
            &snapshot.catalog,
            &snapshot.workflow,
        )
        .unwrap()
        .keys
        .into_iter()
        .map(|(key, status)| (key, status.source_version))
        .collect()
    }
    pub fn adopt(&self) {
        let snapshot = self.snapshot();
        let plan = workflow::plan_source_sync(
            snapshot.identity_text().unwrap(),
            &snapshot.catalog,
            &snapshot.workflow,
            SyncMode::AdoptExisting,
        )
        .unwrap();
        let checkpoint =
            workflow::plan_checkpoint(&snapshot.catalog, &snapshot.workflow, &plan).unwrap();
        self.put(SIDECAR, &workflow::format_document(&checkpoint).unwrap());
    }
}
fn check(
    files: &BTreeMap<PathBuf, Vec<u8>>,
    path: &Path,
    expected: Option<&[u8]>,
) -> Result<(), XcStringsError> {
    let actual = files.get(path).map(Vec::as_slice);
    if actual != expected {
        Err(XcStringsError::ConditionalWriteConflict {
            path: path.into(),
            expected_exists: expected.is_some(),
            actual_exists: actual.is_some(),
        })
    } else {
        Ok(())
    }
}
impl FileStore for Store {
    fn file_identity(&self, path: &Path) -> Result<PathBuf, XcStringsError> {
        Ok(self
            .aliases
            .get(path)
            .cloned()
            .unwrap_or_else(|| path.into()))
    }
    fn read(&self, path: &Path) -> Result<String, XcStringsError> {
        String::from_utf8(self.read_bytes(path)?)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error).into())
    }
    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, XcStringsError> {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "missing fixture").into()
            })
    }
    fn write(&self, path: &Path, content: &str) -> Result<(), XcStringsError> {
        self.files
            .lock()
            .unwrap()
            .insert(path.into(), content.as_bytes().into());
        Ok(())
    }
    fn write_if_matches(
        &self,
        path: &Path,
        expected: Option<&[u8]>,
        content: &str,
    ) -> Result<(), XcStringsError> {
        self.write_if_inputs_match(path, expected, &[], content)
    }
    fn write_if_inputs_match(
        &self,
        path: &Path,
        expected: Option<&[u8]>,
        inputs: &[FilePrecondition<'_>],
        content: &str,
    ) -> Result<(), XcStringsError> {
        let mut files = self.files.lock().unwrap();
        if self.context_conflict && !inputs.is_empty() {
            files.insert(SIDECAR.into(), EXTERNAL_CONTEXT.as_bytes().into());
        }
        if self.xml_conflict && path.extension().and_then(|p| p.to_str()) == Some("xliff") {
            files.insert(path.into(), b"external XML edit".to_vec());
        }
        check(&files, path, expected)?;
        for input in inputs {
            check(&files, input.path, input.expected)?;
        }
        files.insert(path.into(), content.as_bytes().into());
        Ok(())
    }
    fn exists(&self, path: &Path) -> bool {
        self.files.lock().unwrap().contains_key(path)
    }
    fn modified_time(&self, _: &Path) -> Result<SystemTime, XcStringsError> {
        Ok(SystemTime::UNIX_EPOCH)
    }
    fn create_parent_dirs(&self, _: &Path) -> Result<(), XcStringsError> {
        Ok(())
    }
}
pub fn xml(source: &str, target: &str, state: &str) -> String {
    format!(
        r#"<xliff version="1.2"><file original="Catalog.xcstrings" source-language="en" target-language="de"><body><trans-unit id="k"><source>{source}</source><target state="{state}">{target}</target></trans-unit></body></file></xliff>"#
    )
}
