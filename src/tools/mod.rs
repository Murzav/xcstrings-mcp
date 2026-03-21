pub(crate) mod coverage;
pub(crate) mod diff;
pub(crate) mod extract;
pub(crate) mod files;
pub(crate) mod glossary;
pub(crate) mod manage;
pub(crate) mod parse;
pub(crate) mod plural;
pub(crate) mod translate;
pub(crate) mod xliff;

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Serialize;
use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::model::xcstrings::XcStringsFile;
use crate::service::parser;
use parse::CachedFile;

/// Info about a cached file, returned by `list()`.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct CachedFileInfo {
    pub(crate) path: PathBuf,
    pub(crate) source_language: String,
    pub(crate) total_keys: usize,
    pub(crate) is_active: bool,
}

/// Multi-file cache: stores parsed files by path, tracks the active one.
pub(crate) struct FileCache {
    pub(crate) files: HashMap<PathBuf, CachedFile>,
    active: Option<PathBuf>,
}

impl FileCache {
    pub(crate) fn new() -> Self {
        Self {
            files: HashMap::new(),
            active: None,
        }
    }

    /// Insert (or replace) a cached file, and set it as active.
    pub(crate) fn insert(&mut self, path: PathBuf, cached: CachedFile) {
        self.active = Some(path.clone());
        self.files.insert(path, cached);
    }

    /// Get a reference to a cached file by path.
    pub(crate) fn get(&self, path: &PathBuf) -> Option<&CachedFile> {
        self.files.get(path)
    }

    /// Return the active file path, if any.
    pub(crate) fn active_path(&self) -> Option<&PathBuf> {
        self.active.as_ref()
    }

    /// Return info about all cached files, sorted by path.
    pub(crate) fn list(&self) -> Vec<CachedFileInfo> {
        let mut infos: Vec<CachedFileInfo> = self
            .files
            .iter()
            .map(|(path, cached)| CachedFileInfo {
                path: path.clone(),
                source_language: cached.content.source_language.clone(),
                total_keys: cached.content.strings.len(),
                is_active: self.active.as_ref() == Some(path),
            })
            .collect();
        infos.sort_by(|a, b| a.path.cmp(&b.path));
        infos
    }
}

/// Resolve a file from an explicit path or the cache.
/// If `file_path` is provided, reads and parses fresh (and updates cache).
/// If `None`, uses the active cached file.
pub(crate) async fn resolve_file(
    store: &dyn FileStore,
    cache: &Mutex<FileCache>,
    file_path: Option<&str>,
) -> Result<(PathBuf, XcStringsFile), XcStringsError> {
    if let Some(path_str) = file_path {
        let path = PathBuf::from(path_str);
        match path.extension().and_then(|e| e.to_str()) {
            Some("xcstrings") => {}
            _ => return Err(XcStringsError::NotXcStrings { path }),
        }
        let raw = store.read(&path)?;
        let file = parser::parse(&raw)?;
        let mtime = store.modified_time(&path)?;
        let mut guard = cache.lock().await;
        guard.insert(
            path.clone(),
            CachedFile {
                path: path.clone(),
                content: file.clone(),
                modified: mtime,
            },
        );
        Ok((path, file))
    } else {
        let guard = cache.lock().await;
        let active_path = guard
            .active_path()
            .cloned()
            .ok_or(XcStringsError::NoActiveFile)?;
        let cached = guard
            .get(&active_path)
            .ok_or(XcStringsError::NoActiveFile)?;

        // Validate mtime — re-read if file changed externally
        if let Ok(current_mtime) = store.modified_time(&cached.path)
            && current_mtime != cached.modified
        {
            let path = cached.path.clone();
            drop(guard);
            let raw = store.read(&path)?;
            let file = parser::parse(&raw)?;
            let mtime = store.modified_time(&path)?;
            let mut guard = cache.lock().await;
            guard.insert(
                path.clone(),
                CachedFile {
                    path: path.clone(),
                    content: file.clone(),
                    modified: mtime,
                },
            );
            return Ok((path, file));
        }
        Ok((cached.path.clone(), cached.content.clone()))
    }
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::time::SystemTime;

    use crate::error::XcStringsError;
    use crate::io::FileStore;

    pub(crate) struct MemoryStore {
        files: Mutex<HashMap<PathBuf, (String, SystemTime)>>,
    }

    impl MemoryStore {
        pub fn new() -> Self {
            Self {
                files: Mutex::new(HashMap::new()),
            }
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

        fn write(&self, path: &Path, content: &str) -> Result<(), XcStringsError> {
            self.files
                .lock()
                .unwrap()
                .insert(path.to_path_buf(), (content.to_string(), SystemTime::now()));
            Ok(())
        }

        fn modified_time(&self, path: &Path) -> Result<SystemTime, XcStringsError> {
            self.files
                .lock()
                .unwrap()
                .get(path)
                .map(|(_, t)| *t)
                .ok_or_else(|| XcStringsError::FileNotFound {
                    path: path.to_path_buf(),
                })
        }

        fn exists(&self, path: &Path) -> bool {
            self.files.lock().unwrap().contains_key(path)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_helpers::{MemoryStore, SIMPLE_FIXTURE};

    #[tokio::test]
    async fn test_resolve_file_with_path() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());

        let (path, file) = resolve_file(&store, &cache, Some("/test/file.xcstrings"))
            .await
            .unwrap();
        assert_eq!(path, PathBuf::from("/test/file.xcstrings"));
        assert_eq!(file.source_language, "en");

        let guard = cache.lock().await;
        assert!(guard.get(&PathBuf::from("/test/file.xcstrings")).is_some());
    }

    #[tokio::test]
    async fn test_resolve_file_from_cache() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(FileCache::new());

        resolve_file(&store, &cache, Some("/test/file.xcstrings"))
            .await
            .unwrap();

        let (path, file) = resolve_file(&store, &cache, None).await.unwrap();
        assert_eq!(path, PathBuf::from("/test/file.xcstrings"));
        assert_eq!(file.source_language, "en");
    }

    #[tokio::test]
    async fn test_resolve_file_no_cache_no_path() {
        let store = MemoryStore::new();
        let cache = Mutex::new(FileCache::new());

        let result = resolve_file(&store, &cache, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resolve_file_rejects_non_xcstrings() {
        let store = MemoryStore::new();
        store.add_file("/test/file.json", "{}");
        let cache = Mutex::new(FileCache::new());

        let result = resolve_file(&store, &cache, Some("/test/file.json")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_cache_insert_sets_active() {
        let mut cache = FileCache::new();
        assert!(cache.active_path().is_none());

        let path = PathBuf::from("/a.xcstrings");
        cache.insert(
            path.clone(),
            CachedFile {
                path: path.clone(),
                content: crate::service::parser::parse(SIMPLE_FIXTURE).unwrap(),
                modified: std::time::SystemTime::now(),
            },
        );
        assert_eq!(cache.active_path(), Some(&path));
    }

    #[tokio::test]
    async fn test_file_cache_list_sorted() {
        let mut cache = FileCache::new();
        let fixture = crate::service::parser::parse(SIMPLE_FIXTURE).unwrap();

        for p in ["/z.xcstrings", "/a.xcstrings", "/m.xcstrings"] {
            let path = PathBuf::from(p);
            cache.insert(
                path.clone(),
                CachedFile {
                    path,
                    content: fixture.clone(),
                    modified: std::time::SystemTime::now(),
                },
            );
        }

        let list = cache.list();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].path, PathBuf::from("/a.xcstrings"));
        assert_eq!(list[1].path, PathBuf::from("/m.xcstrings"));
        assert_eq!(list[2].path, PathBuf::from("/z.xcstrings"));
        // Only the last inserted (/m.xcstrings) should be active
        let active: Vec<_> = list.iter().filter(|i| i.is_active).collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].path, PathBuf::from("/m.xcstrings"));
    }
}
