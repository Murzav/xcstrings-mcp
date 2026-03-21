pub mod error;
pub mod io;
pub mod model;
pub(crate) mod prompts;
pub mod server;
pub(crate) mod service;
pub(crate) mod tools;

pub use error::XcStringsError;
pub use io::FileStore;
pub use model::xcstrings::XcStringsFile;
pub use server::XcStringsMcpServer;

/// Re-exports for integration testing. Not part of the public API.
#[doc(hidden)]
pub mod _test_support {
    pub mod service {
        pub use crate::service::context;
        pub use crate::service::coverage;
        pub use crate::service::creator;
        pub use crate::service::diff;
        pub use crate::service::extractor;
        pub use crate::service::file_validator;
        pub use crate::service::formatter;
        pub use crate::service::glossary;
        pub use crate::service::locale;
        pub use crate::service::merger;
        pub use crate::service::parser;
        pub use crate::service::plural_extractor;
        pub use crate::service::validator;
        pub use crate::service::xliff;
    }
}
