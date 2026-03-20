pub mod fs;

use std::path::Path;
use std::time::SystemTime;

use crate::error::XcStringsError;

pub trait FileStore: Send + Sync {
    fn read(&self, path: &Path) -> Result<String, XcStringsError>;
    fn write(&self, path: &Path, content: &str) -> Result<(), XcStringsError>;
    fn modified_time(&self, path: &Path) -> Result<SystemTime, XcStringsError>;
    fn exists(&self, path: &Path) -> bool;
}
