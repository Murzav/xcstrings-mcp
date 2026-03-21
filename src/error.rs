use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum XcStringsError {
    #[error("file not found: {path}")]
    FileNotFound { path: PathBuf },

    #[error("invalid path {path}: {reason}")]
    InvalidPath { path: PathBuf, reason: String },

    #[error("not an .xcstrings file: {path}")]
    NotXcStrings { path: PathBuf },

    #[error("invalid format: {0}")]
    InvalidFormat(String),

    #[error("JSON parse error: {0}")]
    JsonParse(String),

    #[error("locale not found: {0}")]
    LocaleNotFound(String),

    #[error("locale already exists: {0}")]
    LocaleAlreadyExists(String),

    #[error("no active file — call parse_xcstrings first")]
    NoActiveFile,

    #[error("invalid batch size: {0}")]
    InvalidBatchSize(String),

    #[error("file too large: {size_mb}MB (max {max_mb}MB)")]
    FileTooLarge { size_mb: u64, max_mb: u64 },

    #[error("file is locked by another process (likely Xcode): {path}")]
    FileLocked { path: PathBuf },

    #[error("cannot remove source locale: {0}")]
    CannotRemoveSourceLocale(String),

    #[error("glossary error: {0}")]
    GlossaryError(String),

    #[error("XLIFF parse error: {0}")]
    XliffParse(String),

    #[error("XLIFF format error: {0}")]
    XliffFormat(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

impl From<XcStringsError> for rmcp::model::ErrorData {
    fn from(e: XcStringsError) -> Self {
        rmcp::model::ErrorData::new(rmcp::model::ErrorCode::INTERNAL_ERROR, e.to_string(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_converts_to_mcp_error() {
        let err = XcStringsError::NoActiveFile;
        let mcp_err: rmcp::model::ErrorData = err.into();
        assert!(mcp_err.message.contains("no active file"));
    }

    #[test]
    fn error_display() {
        let err = XcStringsError::FileNotFound {
            path: PathBuf::from("/test.xcstrings"),
        };
        assert!(err.to_string().contains("/test.xcstrings"));

        let err = XcStringsError::FileTooLarge {
            size_mb: 100,
            max_mb: 50,
        };
        assert!(err.to_string().contains("100"));
        assert!(err.to_string().contains("50"));
    }
}
