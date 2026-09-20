mod atomic_write;
pub mod fs;
#[cfg(test)]
mod guard_contract_tests;

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::XcStringsError;

/// An exact input snapshot that must remain unchanged until a guarded write commits.
pub struct FilePrecondition<'a> {
    pub path: &'a Path,
    pub expected: Option<&'a [u8]>,
}

pub trait FileStore: Send + Sync {
    /// Stable cache identity for a path. Stores without alias awareness retain
    /// the caller's spelling; filesystem stores should canonicalize aliases.
    fn file_identity(&self, path: &Path) -> Result<PathBuf, XcStringsError> {
        Ok(path.to_path_buf())
    }
    fn read(&self, path: &Path) -> Result<String, XcStringsError>;
    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, XcStringsError>;
    fn write(&self, path: &Path, content: &str) -> Result<(), XcStringsError>;
    /// Atomically write only if the target still contains `expected` bytes.
    /// `None` means that no filesystem entry may exist at the target, including
    /// a dangling symlink. Implementations that cannot provide one indivisible
    /// compare-and-write operation must fail closed.
    fn write_if_matches(
        &self,
        path: &Path,
        expected: Option<&[u8]>,
        content: &str,
    ) -> Result<(), XcStringsError> {
        let _ = (expected, content);
        Err(XcStringsError::ConditionalWriteUnsupported {
            path: path.to_path_buf(),
        })
    }
    /// Compare the output and every input under one ordered lock set, then replace
    /// only the output. A sequence of independent comparisons is not sufficient.
    fn write_if_inputs_match(
        &self,
        path: &Path,
        expected: Option<&[u8]>,
        inputs: &[FilePrecondition<'_>],
        content: &str,
    ) -> Result<(), XcStringsError> {
        let _ = (expected, inputs, content);
        Err(XcStringsError::ConditionalWriteUnsupported {
            path: path.to_path_buf(),
        })
    }
    fn modified_time(&self, path: &Path) -> Result<SystemTime, XcStringsError>;
    fn exists(&self, path: &Path) -> bool;
    fn create_parent_dirs(&self, path: &Path) -> Result<(), XcStringsError>;
}
