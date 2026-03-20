pub(crate) mod coverage;
pub(crate) mod extract;
pub(crate) mod manage;
pub(crate) mod parse;
pub(crate) mod plural;
pub(crate) mod translate;

use std::path::PathBuf;

use tokio::sync::Mutex;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::model::xcstrings::XcStringsFile;
use crate::service::parser;
use parse::CachedFile;

/// Resolve a file from an explicit path or the cache.
/// If `file_path` is provided, reads and parses fresh (and updates cache).
/// If `None`, uses the cached file.
pub(crate) async fn resolve_file(
    store: &dyn FileStore,
    cache: &Mutex<Option<CachedFile>>,
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
        *guard = Some(CachedFile {
            path: path.clone(),
            content: file.clone(),
            modified: mtime,
        });
        Ok((path, file))
    } else {
        let guard = cache.lock().await;
        match guard.as_ref() {
            Some(cached) => {
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
                    *guard = Some(CachedFile {
                        path: path.clone(),
                        content: file.clone(),
                        modified: mtime,
                    });
                    return Ok((path, file));
                }
                Ok((cached.path.clone(), cached.content.clone()))
            }
            None => Err(XcStringsError::NoActiveFile),
        }
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
    }

    pub(crate) const SIMPLE_FIXTURE: &str = include_str!("../../tests/fixtures/simple.xcstrings");
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_helpers::{MemoryStore, SIMPLE_FIXTURE};

    #[tokio::test]
    async fn test_resolve_file_with_path() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(None);

        let (path, file) = resolve_file(&store, &cache, Some("/test/file.xcstrings"))
            .await
            .unwrap();
        assert_eq!(path, PathBuf::from("/test/file.xcstrings"));
        assert_eq!(file.source_language, "en");

        let guard = cache.lock().await;
        assert!(guard.is_some());
    }

    #[tokio::test]
    async fn test_resolve_file_from_cache() {
        let store = MemoryStore::new();
        store.add_file("/test/file.xcstrings", SIMPLE_FIXTURE);
        let cache = Mutex::new(None);

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
        let cache = Mutex::new(None);

        let result = resolve_file(&store, &cache, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_resolve_file_rejects_non_xcstrings() {
        let store = MemoryStore::new();
        store.add_file("/test/file.json", "{}");
        let cache = Mutex::new(None);

        let result = resolve_file(&store, &cache, Some("/test/file.json")).await;
        assert!(result.is_err());
    }
}
