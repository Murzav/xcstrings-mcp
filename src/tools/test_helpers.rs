use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use crate::error::XcStringsError;
use crate::io::{FilePrecondition, FileStore};

pub(crate) struct MemoryStore {
    files: Mutex<HashMap<PathBuf, (String, SystemTime)>>,
    binary_files: Mutex<HashMap<PathBuf, (Vec<u8>, SystemTime)>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            files: Mutex::new(HashMap::new()),
            binary_files: Mutex::new(HashMap::new()),
        }
    }

    pub fn add_binary_file(&self, path: impl Into<PathBuf>, bytes: Vec<u8>) {
        self.binary_files
            .lock()
            .unwrap()
            .insert(path.into(), (bytes, SystemTime::now()));
    }

    pub fn add_file(&self, path: impl Into<PathBuf>, content: &str) {
        self.files
            .lock()
            .unwrap()
            .insert(path.into(), (content.to_string(), SystemTime::now()));
    }

    /// Update a file's content and bump its modified time.
    /// Used in tests to simulate external file changes.
    pub fn update_file(&self, path: impl Into<PathBuf>, content: &str) {
        self.files
            .lock()
            .unwrap()
            .insert(path.into(), (content.to_string(), SystemTime::now()));
    }

    pub fn get_content(&self, path: &Path) -> Option<String> {
        self.files.lock().unwrap().get(path).map(|(c, _)| c.clone())
    }
}

impl FileStore for MemoryStore {
    fn read(&self, path: &Path) -> Result<String, XcStringsError> {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .map(|(c, _)| c.clone())
            .ok_or_else(|| XcStringsError::FileNotFound {
                path: path.to_path_buf(),
            })
    }

    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, XcStringsError> {
        // Check binary_files first, then fall back to string conversion
        if let Some((bytes, _)) = self.binary_files.lock().unwrap().get(path) {
            return Ok(bytes.clone());
        }
        self.read(path).map(|s| s.into_bytes())
    }

    fn write(&self, path: &Path, content: &str) -> Result<(), XcStringsError> {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), (content.to_string(), SystemTime::now()));
        Ok(())
    }

    fn write_if_matches(
        &self,
        path: &Path,
        expected: Option<&[u8]>,
        content: &str,
    ) -> Result<(), XcStringsError> {
        let mut binary = self.binary_files.lock().unwrap();
        let mut files = self.files.lock().unwrap();
        let actual = binary
            .get(path)
            .map(|(bytes, _)| bytes.as_slice())
            .or_else(|| files.get(path).map(|(text, _)| text.as_bytes()));
        if actual != expected {
            return Err(XcStringsError::ConditionalWriteConflict {
                path: path.to_path_buf(),
                expected_exists: expected.is_some(),
                actual_exists: actual.is_some(),
            });
        }
        binary.remove(path);
        files.insert(
            path.to_path_buf(),
            (content.to_owned(), SystemTime::UNIX_EPOCH),
        );
        Ok(())
    }

    fn modified_time(&self, path: &Path) -> Result<SystemTime, XcStringsError> {
        if let Some((_, t)) = self.files.lock().unwrap().get(path) {
            return Ok(*t);
        }
        if let Some((_, t)) = self.binary_files.lock().unwrap().get(path) {
            return Ok(*t);
        }
        Err(XcStringsError::FileNotFound {
            path: path.to_path_buf(),
        })
    }

    fn write_if_inputs_match(
        &self,
        path: &Path,
        expected: Option<&[u8]>,
        inputs: &[FilePrecondition<'_>],
        content: &str,
    ) -> Result<(), XcStringsError> {
        let mut identities = std::collections::BTreeSet::from([path]);
        if inputs.iter().any(|input| !identities.insert(input.path)) {
            return Err(XcStringsError::InvalidPath {
                path: path.to_path_buf(),
                reason: "guarded write inputs must have distinct canonical identities".into(),
            });
        }
        let mut binary = self.binary_files.lock().unwrap();
        let mut files = self.files.lock().unwrap();
        let output = FilePrecondition { path, expected };
        for condition in std::iter::once(&output).chain(inputs) {
            let actual = binary
                .get(condition.path)
                .map(|(bytes, _)| bytes.as_slice())
                .or_else(|| files.get(condition.path).map(|(text, _)| text.as_bytes()));
            if actual != condition.expected {
                return Err(XcStringsError::ConditionalWriteConflict {
                    path: condition.path.to_path_buf(),
                    expected_exists: condition.expected.is_some(),
                    actual_exists: actual.is_some(),
                });
            }
        }
        binary.remove(path);
        files.insert(
            path.to_path_buf(),
            (content.to_owned(), SystemTime::UNIX_EPOCH),
        );
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        self.files.lock().unwrap().contains_key(path)
            || self.binary_files.lock().unwrap().contains_key(path)
    }

    fn create_parent_dirs(&self, _path: &Path) -> Result<(), XcStringsError> {
        Ok(())
    }
}

pub(crate) const SIMPLE_FIXTURE: &str = include_str!("../../tests/fixtures/simple.xcstrings");

/// Fixture: "greeting" has %@ specifier, "farewell" has none.
pub(crate) const MIXED_SPECIFIER_FIXTURE: &str = r#"{
  "sourceLanguage" : "en",
  "strings" : {
"greeting" : {
  "localizations" : {
    "en" : {
      "stringUnit" : {
        "state" : "translated",
        "value" : "Hello %@"
      }
    }
  }
},
"farewell" : {
  "localizations" : {
    "en" : {
      "stringUnit" : {
        "state" : "translated",
        "value" : "Goodbye"
      }
    }
  }
}
  },
  "version" : "1.0"
}"#;
