use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use xcstrings_mcp::error::XcStringsError;
use xcstrings_mcp::io::FileStore;

pub struct MemoryStore {
    files: Mutex<HashMap<PathBuf, (String, SystemTime)>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            files: Mutex::new(HashMap::new()),
        }
    }

    pub fn add_file(&self, path: impl Into<PathBuf>, content: &str) {
        let mut files = self.files.lock().unwrap();
        files.insert(path.into(), (content.to_string(), SystemTime::now()));
    }

    pub fn get_content(&self, path: &Path) -> Option<String> {
        let files = self.files.lock().unwrap();
        files.get(path).map(|(c, _)| c.clone())
    }
}

impl FileStore for MemoryStore {
    fn read(&self, path: &Path) -> Result<String, XcStringsError> {
        let files = self.files.lock().unwrap();
        match files.get(path) {
            Some((content, _)) => Ok(content.clone()),
            None => Err(XcStringsError::FileNotFound {
                path: path.to_path_buf(),
            }),
        }
    }

    fn write(&self, path: &Path, content: &str) -> Result<(), XcStringsError> {
        let mut files = self.files.lock().unwrap();
        files.insert(path.to_path_buf(), (content.to_string(), SystemTime::now()));
        Ok(())
    }

    fn modified_time(&self, path: &Path) -> Result<SystemTime, XcStringsError> {
        let files = self.files.lock().unwrap();
        match files.get(path) {
            Some((_, mtime)) => Ok(*mtime),
            None => Err(XcStringsError::FileNotFound {
                path: path.to_path_buf(),
            }),
        }
    }

    fn exists(&self, path: &Path) -> bool {
        let files = self.files.lock().unwrap();
        files.contains_key(path)
    }
}

/// Generate a large xcstrings fixture for benchmarking.
#[allow(dead_code)]
pub fn generate_large_fixture(keys: usize, locales: usize) -> String {
    let locale_codes: Vec<String> = (0..locales).map(|i| format!("l{i}")).collect();

    let mut strings = String::new();
    for i in 0..keys {
        if i > 0 {
            strings.push(',');
        }

        let mut locs = String::new();
        // Always include "en" source
        locs.push_str(&format!(
            r#"
        "en" : {{
          "stringUnit" : {{
            "state" : "translated",
            "value" : "Value {i}"
          }}
        }}"#
        ));

        for locale in &locale_codes {
            let state = if i % 3 == 0 { "new" } else { "translated" };
            locs.push_str(&format!(
                r#",
        "{locale}" : {{
          "stringUnit" : {{
            "state" : "{state}",
            "value" : "Trans {i} {locale}"
          }}
        }}"#
            ));
        }

        strings.push_str(&format!(
            r#"
    "key_{i}" : {{
      "localizations" : {{{locs}
      }}
    }}"#
        ));
    }

    format!(
        r#"{{
  "sourceLanguage" : "en",
  "strings" : {{{strings}
  }},
  "version" : "1.0"
}}"#
    )
}
