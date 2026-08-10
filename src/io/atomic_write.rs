use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use crate::error::XcStringsError;

pub(super) enum ExpectedContent<'a> {
    Any,
    Exact(Option<&'a [u8]>),
}

pub(super) fn write(
    target: &Path,
    content: &str,
    expected: ExpectedContent<'_>,
) -> Result<(), XcStringsError> {
    write_with_replace(target, content, expected, |temp, target| {
        fs::rename(temp, target)
    })
}

fn write_with_replace(
    target: &Path,
    content: &str,
    expected: ExpectedContent<'_>,
    replace: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), XcStringsError> {
    let directory = target.parent().ok_or_else(|| XcStringsError::InvalidPath {
        path: target.to_path_buf(),
        reason: "no parent directory".into(),
    })?;
    let lock_path = stable_lock_path(target)?;
    let lock_file = open_stable_lock(&lock_path)?;
    lock_exclusive(&lock_file)?;

    let temp_path = stable_temp_path(target)?;
    cleanup_orphan(&temp_path)?;
    compare_expected(target, expected)?;
    let result = write_and_replace(&temp_path, target, directory, content, replace);
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn open_stable_lock(lock_path: &Path) -> Result<fs::File, XcStringsError> {
    match fs::symlink_metadata(lock_path) {
        Ok(metadata) => validate_lock_metadata(lock_path, &metadata)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(lock_path)
        .map_err(|error| {
            if error.raw_os_error() == Some(libc::ELOOP) {
                invalid_lock(lock_path)
            } else {
                error.into()
            }
        })?;
    let opened = file.metadata()?;
    let linked = fs::symlink_metadata(lock_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            invalid_lock(lock_path)
        } else {
            error.into()
        }
    })?;
    validate_lock_metadata(lock_path, &opened)?;
    validate_lock_metadata(lock_path, &linked)?;
    if opened.dev() != linked.dev() || opened.ino() != linked.ino() {
        return Err(invalid_lock(lock_path));
    }
    Ok(file)
}

fn validate_lock_metadata(lock_path: &Path, metadata: &fs::Metadata) -> Result<(), XcStringsError> {
    if metadata.file_type().is_file() && metadata.nlink() == 1 {
        Ok(())
    } else {
        Err(invalid_lock(lock_path))
    }
}

fn invalid_lock(lock_path: &Path) -> XcStringsError {
    XcStringsError::InvalidPath {
        path: lock_path.to_path_buf(),
        reason: "stable lock must be a uniquely linked regular sidecar".into(),
    }
}

fn stable_temp_path(target: &Path) -> Result<PathBuf, XcStringsError> {
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| XcStringsError::InvalidPath {
            path: target.to_path_buf(),
            reason: "filename is not valid UTF-8".into(),
        })?;
    Ok(target.with_file_name(format!(".{file_name}.xcstrings-mcp.tmp")))
}

fn cleanup_orphan(temp_path: &Path) -> Result<(), XcStringsError> {
    match fs::symlink_metadata(temp_path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            fs::remove_file(temp_path)?;
            Ok(())
        }
        Ok(_) => Err(XcStringsError::InvalidPath {
            path: temp_path.to_path_buf(),
            reason: "target-owned temp path is not a regular file".into(),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn stable_lock_path(target: &Path) -> Result<PathBuf, XcStringsError> {
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| XcStringsError::InvalidPath {
            path: target.to_path_buf(),
            reason: "filename is not valid UTF-8".into(),
        })?;
    Ok(target.with_file_name(format!("{file_name}.xcstrings-mcp.lock")))
}

fn lock_exclusive(file: &fs::File) -> Result<(), XcStringsError> {
    // SAFETY: `file` owns a valid descriptor for the full duration of this call.
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

fn compare_expected(target: &Path, expected: ExpectedContent<'_>) -> Result<(), XcStringsError> {
    let ExpectedContent::Exact(expected) = expected else {
        return Ok(());
    };
    let actual_exists = match fs::symlink_metadata(target) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    let actual = if actual_exists {
        match fs::read(target) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        }
    } else {
        None
    };
    let matches = match expected {
        None => !actual_exists,
        Some(expected) => actual.as_deref() == Some(expected),
    };
    if matches {
        return Ok(());
    }
    Err(XcStringsError::ConditionalWriteConflict {
        path: target.to_path_buf(),
        expected_exists: expected.is_some(),
        actual_exists,
    })
}

fn write_and_replace(
    temp_path: &Path,
    target: &Path,
    directory: &Path,
    content: &str,
    replace: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), XcStringsError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_path)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    replace(temp_path, target)?;
    fs::File::open(directory)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn injected_replace_failure_after_temp_creation_cleans_temp_and_preserves_target() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("output.xcstrings");
        fs::write(&target, b"original").unwrap();
        let error = write_with_replace(
            &target,
            "complete replacement",
            ExpectedContent::Exact(Some(b"original")),
            |_temp, _target| Err(std::io::Error::other("injected replace failure")),
        )
        .unwrap_err();

        assert!(error.to_string().contains("injected replace failure"));
        assert_eq!(fs::read(&target).unwrap(), b"original");
        let entries = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(
            entries
                .iter()
                .any(|name| name.ends_with(".xcstrings-mcp.lock"))
        );
        assert!(!entries.iter().any(|name| name.ends_with(".tmp")));
    }
}
