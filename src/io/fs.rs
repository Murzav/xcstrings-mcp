use std::fs;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use tracing::{info, warn};

use crate::error::XcStringsError;

use super::FileStore;

pub struct FsFileStore {
    max_file_size: u64,
}

impl Default for FsFileStore {
    fn default() -> Self {
        Self::new()
    }
}

impl FsFileStore {
    pub fn new() -> Self {
        let max_mb = std::env::var("XCSTRINGS_MAX_FILE_SIZE_MB")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(50);

        // Cleanup orphan temp files from previous crashes
        if let Ok(cwd) = std::env::current_dir()
            && let Ok(entries) = fs::read_dir(&cwd)
        {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with(".xcstrings-mcp-") && name_str.ends_with(".tmp") {
                    let _ = fs::remove_file(entry.path());
                    info!("cleaned up orphan temp file: {}", name_str);
                }
            }
        }

        Self {
            max_file_size: max_mb * 1024 * 1024,
        }
    }

    fn validate_path(&self, path: &Path) -> Result<PathBuf, XcStringsError> {
        // Reject path traversal: check for ".." components BEFORE canonicalization
        for component in path.components() {
            if matches!(component, std::path::Component::ParentDir) {
                return Err(XcStringsError::InvalidPath {
                    path: path.to_path_buf(),
                    reason: "path traversal detected (contains '..')".into(),
                });
            }
        }

        // Canonicalize (works for existing files)
        let canonical = match fs::canonicalize(path) {
            Ok(p) => p,
            Err(_) => {
                // File may not exist yet (write case) — canonicalize parent, append filename
                let parent = path.parent().ok_or_else(|| XcStringsError::InvalidPath {
                    path: path.to_path_buf(),
                    reason: "no parent directory".into(),
                })?;
                let filename = path
                    .file_name()
                    .ok_or_else(|| XcStringsError::InvalidPath {
                        path: path.to_path_buf(),
                        reason: "no filename".into(),
                    })?;
                let canonical_parent =
                    fs::canonicalize(parent).map_err(|_| XcStringsError::InvalidPath {
                        path: path.to_path_buf(),
                        reason: "parent directory does not exist".into(),
                    })?;
                canonical_parent.join(filename)
            }
        };

        Ok(canonical)
    }

    fn strip_bom(content: &str) -> &str {
        content.strip_prefix('\u{feff}').unwrap_or(content)
    }
}

impl FileStore for FsFileStore {
    fn read(&self, path: &Path) -> Result<String, XcStringsError> {
        let canonical = self.validate_path(path)?;

        if !canonical.exists() {
            return Err(XcStringsError::FileNotFound { path: canonical });
        }

        let metadata = fs::metadata(&canonical)?;
        let size = metadata.len();
        if size > self.max_file_size {
            return Err(XcStringsError::FileTooLarge {
                size_mb: size / (1024 * 1024),
                max_mb: self.max_file_size / (1024 * 1024),
            });
        }

        let content = fs::read_to_string(&canonical)?;
        Ok(Self::strip_bom(&content).to_string())
    }

    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, XcStringsError> {
        let canonical = self.validate_path(path)?;

        if !canonical.exists() {
            return Err(XcStringsError::FileNotFound { path: canonical });
        }

        let metadata = fs::metadata(&canonical)?;
        let size = metadata.len();
        if size > self.max_file_size {
            return Err(XcStringsError::FileTooLarge {
                size_mb: size / (1024 * 1024),
                max_mb: self.max_file_size / (1024 * 1024),
            });
        }

        Ok(fs::read(&canonical)?)
    }

    fn write(&self, path: &Path, content: &str) -> Result<(), XcStringsError> {
        let canonical = self.validate_path(path)?;
        let dir = canonical
            .parent()
            .ok_or_else(|| XcStringsError::InvalidPath {
                path: canonical.clone(),
                reason: "no parent directory".into(),
            })?;

        // Acquire advisory lock on target file (best-effort: skip if file doesn't exist yet)
        let _lock_file = if canonical.exists() {
            let lock_file = fs::File::open(&canonical)?;
            let fd = lock_file.as_raw_fd();
            // SAFETY: flock is a POSIX syscall, fd is valid because lock_file is alive
            let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
            if ret != 0 {
                let errno = std::io::Error::last_os_error();
                if errno.kind() == std::io::ErrorKind::WouldBlock {
                    return Err(XcStringsError::FileLocked { path: canonical });
                }
                // Non-blocking lock not supported (e.g. network FS) — proceed without lock
                warn!(
                    "advisory flock unavailable for {}: {errno} — proceeding without lock",
                    canonical.display()
                );
                None
            } else {
                Some(lock_file)
            }
        } else {
            None
        };

        let tmp_name = format!(
            ".xcstrings-mcp-{}-{}.tmp",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );
        let tmp_path = dir.join(&tmp_name);

        // Write to temp file, fsync, then atomic rename
        let result = (|| -> Result<(), XcStringsError> {
            let mut file = fs::File::create(&tmp_path)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            fs::rename(&tmp_path, &canonical)?;
            Ok(())
        })();

        // Clean up temp file on failure
        if result.is_err() {
            let _ = fs::remove_file(&tmp_path);
        }

        // Lock is released when _lock_file is dropped
        result?;

        info!("wrote {} bytes to {}", content.len(), canonical.display());
        Ok(())
    }

    fn modified_time(&self, path: &Path) -> Result<SystemTime, XcStringsError> {
        let canonical = self.validate_path(path)?;
        let metadata = fs::metadata(&canonical)?;
        Ok(metadata.modified()?)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn create_parent_dirs(&self, path: &Path) -> Result<(), XcStringsError> {
        fs::create_dir_all(path.parent().unwrap_or(path))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_read_write_roundtrip() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.xcstrings");
        let store = FsFileStore::new();

        let content = r#"{"sourceLanguage":"en","strings":{},"version":"1.0"}"#;
        store.write(&file_path, content).unwrap();

        let read_back = store.read(&file_path).unwrap();
        assert_eq!(read_back, content);
    }

    #[test]
    fn test_bom_stripping() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("bom.xcstrings");

        let content = "hello world";
        let with_bom = format!("\u{feff}{content}");
        std::fs::write(&file_path, with_bom.as_bytes()).unwrap();

        let store = FsFileStore::new();
        let read_back = store.read(&file_path).unwrap();
        assert_eq!(read_back, content);
    }

    #[test]
    fn test_file_too_large() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("big.xcstrings");
        std::fs::write(&file_path, "ab").unwrap();

        let store = FsFileStore {
            max_file_size: 1, // 1 byte max
        };
        let err = store.read(&file_path).unwrap_err();
        assert!(
            matches!(err, XcStringsError::FileTooLarge { .. }),
            "expected FileTooLarge, got: {err}"
        );
    }

    #[test]
    fn test_path_traversal_rejected() {
        let store = FsFileStore::new();
        // ".." components are rejected before canonicalization
        let result = store.validate_path(Path::new("/tmp/../etc/passwd"));
        assert!(result.is_err(), "path traversal should be rejected");
        let err = result.unwrap_err();
        assert!(
            matches!(err, XcStringsError::InvalidPath { .. }),
            "expected InvalidPath, got: {err}"
        );
    }

    #[test]
    fn test_file_not_found() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("nope.xcstrings");
        let store = FsFileStore::new();

        let err = store.read(&file_path).unwrap_err();
        assert!(
            matches!(err, XcStringsError::FileNotFound { .. }),
            "expected FileNotFound, got: {err}"
        );
    }

    #[test]
    fn test_validate_path_no_parent() {
        let store = FsFileStore::new();
        let result = store.validate_path(Path::new(""));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_path_parent_not_exists() {
        let store = FsFileStore::new();
        let result = store.validate_path(Path::new("/no_such_parent_dir_xyz/file.txt"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(err, XcStringsError::InvalidPath { .. }),
            "expected InvalidPath, got: {err}"
        );
    }

    #[test]
    fn test_write_creates_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("new_file.xcstrings");
        let store = FsFileStore::new();

        assert!(!file_path.exists());
        store.write(&file_path, "content").unwrap();
        assert!(file_path.exists());
        assert_eq!(std::fs::read_to_string(&file_path).unwrap(), "content");
    }

    #[test]
    fn test_modified_time() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("timed.xcstrings");
        let store = FsFileStore::new();

        store.write(&file_path, "content").unwrap();
        let mtime = store.modified_time(&file_path).unwrap();
        let elapsed = SystemTime::now().duration_since(mtime).unwrap();
        assert!(elapsed.as_secs() < 5);
    }

    #[test]
    fn test_exists() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("exists.xcstrings");
        let store = FsFileStore::new();

        assert!(!store.exists(&file_path));
        store.write(&file_path, "content").unwrap();
        assert!(store.exists(&file_path));
    }

    #[test]
    fn test_default_impl() {
        let store = FsFileStore::default();
        assert!(!store.exists(Path::new("/nonexistent")));
    }

    #[test]
    fn test_flock_blocks_concurrent_write() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("locked.xcstrings");
        let store = FsFileStore::new();

        // Create the file first
        store.write(&file_path, "initial").unwrap();

        // Hold an exclusive lock on the file
        let lock_file = fs::File::open(&file_path).unwrap();
        let fd = lock_file.as_raw_fd();
        // SAFETY: fd is valid, lock_file is alive
        let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
        assert_eq!(ret, 0, "should acquire lock");

        // Attempt to write while locked — should fail with FileLocked
        let err = store.write(&file_path, "updated").unwrap_err();
        assert!(
            matches!(err, XcStringsError::FileLocked { .. }),
            "expected FileLocked, got: {err}"
        );

        // Release lock
        // SAFETY: fd is valid, lock_file is alive
        unsafe { libc::flock(fd, libc::LOCK_UN) };
        drop(lock_file);

        // Now write should succeed
        store.write(&file_path, "updated").unwrap();
        let content = store.read(&file_path).unwrap();
        assert_eq!(content, "updated");
    }

    #[test]
    fn test_read_bytes_roundtrip() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("binary.dat");
        let content = b"\x00\x01\x02\xFF\xFE\xFD";
        std::fs::write(&file_path, content).unwrap();

        let store = FsFileStore::new();
        let read_back = store.read_bytes(&file_path).unwrap();
        assert_eq!(read_back, content);
    }

    #[test]
    fn test_read_bytes_file_not_found() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("nope.bin");
        let store = FsFileStore::new();

        let err = store.read_bytes(&file_path).unwrap_err();
        assert!(
            matches!(err, XcStringsError::FileNotFound { .. }),
            "expected FileNotFound, got: {err}"
        );
    }

    #[test]
    fn test_read_bytes_file_too_large() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("big.bin");
        std::fs::write(&file_path, b"ab").unwrap();

        let store = FsFileStore {
            max_file_size: 1, // 1 byte max
        };
        let err = store.read_bytes(&file_path).unwrap_err();
        assert!(
            matches!(err, XcStringsError::FileTooLarge { .. }),
            "expected FileTooLarge, got: {err}"
        );
    }

    #[test]
    fn test_atomic_write_no_orphans() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("clean.xcstrings");
        let store = FsFileStore::new();

        store.write(&file_path, "content").unwrap();

        // No .tmp files should remain
        let tmp_files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(
            tmp_files.is_empty(),
            "orphan tmp files found: {tmp_files:?}"
        );
    }
}
