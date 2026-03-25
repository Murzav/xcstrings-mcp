use std::path::{Path, PathBuf};
use std::process::ExitCode;

use xcstrings_mcp::error::XcStringsError;
use xcstrings_mcp::io::FileStore;
use xcstrings_mcp::io::fs::FsFileStore;
use xcstrings_mcp::model::xcstrings::XcStringsFile;
use xcstrings_mcp::service::{discovery, formatter, parser};

pub const EXIT_OK: u8 = 0;
pub const EXIT_ERROR: u8 = 1;
pub const EXIT_VALIDATION_ISSUES: u8 = 2;

/// Resolve .xcstrings file path: use explicit or auto-discover from cwd.
pub fn resolve_file(file: Option<PathBuf>) -> Result<PathBuf, XcStringsError> {
    if let Some(path) = file {
        return Ok(path);
    }
    let cwd = std::env::current_dir().map_err(|e| {
        XcStringsError::InvalidFormat(format!("cannot determine current directory: {e}"))
    })?;
    let found = discovery::find_xcstrings_files(&cwd);
    match found.as_slice() {
        [] => Err(XcStringsError::InvalidFormat(
            "no .xcstrings files found in current directory tree (specify path or run from project root)".into(),
        )),
        [single] => Ok(single.clone()),
        multiple => Err(XcStringsError::InvalidFormat(format!(
            "found {} .xcstrings files, specify one:\n{}",
            multiple.len(),
            multiple.iter().map(|p| format!("  {}", p.display())).collect::<Vec<_>>().join("\n")
        ))),
    }
}

/// Load and parse an .xcstrings file.
pub fn load_file(file: Option<PathBuf>) -> Result<(PathBuf, XcStringsFile), XcStringsError> {
    let path = resolve_file(file)?;
    let store = FsFileStore::new();
    let content = store.read(&path)?;
    let parsed = parser::parse(&content)?;
    Ok((path, parsed))
}

/// Format and atomically write an .xcstrings file.
pub fn save_file(path: &Path, file: &XcStringsFile) -> Result<(), XcStringsError> {
    let store = FsFileStore::new();
    let formatted = formatter::format_xcstrings(file)?;
    store.write(path, &formatted)?;
    Ok(())
}

/// Print error to stderr and return error exit code.
pub fn handle_error(err: XcStringsError) -> ExitCode {
    eprintln!("error: {err}");
    ExitCode::from(EXIT_ERROR)
}
