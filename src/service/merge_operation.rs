use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::XcStringsError;
use crate::io::FileStore;
use crate::model::xcstrings::XcStringsFile;

use super::semantic_merge::{
    ConflictResolution, ExpectedFingerprints, FileFingerprint, MergeOptions, MergeReport,
    file_fingerprint, fingerprint, prepare_merge,
};

#[derive(Debug, Clone)]
pub struct MergeRequest {
    pub base_path: PathBuf,
    pub current_path: PathBuf,
    pub incoming_path: PathBuf,
    pub output_path: PathBuf,
    pub dry_run: bool,
    pub resolutions: Vec<ConflictResolution>,
    pub expected_fingerprints: Option<ExpectedFingerprints>,
    pub conflict_offset: usize,
    pub conflict_limit: usize,
}

#[derive(Debug)]
pub struct MergeExecution {
    pub report: MergeReport,
    pub(crate) parsed: XcStringsFile,
}

/// Read explicit inputs, recompute the semantic merge, and optionally apply it
/// with exact-byte compare-and-swap semantics.
pub fn execute_merge(
    store: &dyn FileStore,
    request: &MergeRequest,
) -> Result<MergeExecution, XcStringsError> {
    validate_catalog_path(&request.base_path)?;
    validate_catalog_path(&request.current_path)?;
    validate_catalog_path(&request.incoming_path)?;
    validate_catalog_path(&request.output_path)?;

    let base = store.read_bytes(&request.base_path)?;
    let current = store.read_bytes(&request.current_path)?;
    let incoming = store.read_bytes(&request.incoming_path)?;
    let output_before = match read_if_present(store, &request.output_path) {
        Err(XcStringsError::FileNotFound { .. }) if !request.dry_run => {
            return Err(XcStringsError::StaleMergeFingerprint {
                input: "output".into(),
            });
        }
        result => result?,
    };

    if !request.dry_run {
        let expected = request
            .expected_fingerprints
            .as_ref()
            .ok_or(XcStringsError::MergeExpectedFingerprintsRequired)?;
        verify_fingerprint("base", &base, &expected.base)?;
        verify_fingerprint("current", &current, &expected.current)?;
        verify_fingerprint("incoming", &incoming, &expected.incoming)?;
        verify_output_fingerprint(output_before.as_deref(), expected.output.as_deref())?;
    }

    let options = MergeOptions {
        resolutions: request.resolutions.clone(),
        conflict_offset: request.conflict_offset,
        conflict_limit: request.conflict_limit,
    };
    let mut prepared = prepare_merge(&base, &current, &incoming, &options)?;
    let output_fingerprint = output_before
        .as_deref()
        .map(file_fingerprint_without_schema);
    let expected = ExpectedFingerprints {
        base: prepared.report.fingerprints.base.sha256.clone(),
        current: prepared.report.fingerprints.current.sha256.clone(),
        incoming: prepared.report.fingerprints.incoming.sha256.clone(),
        output: output_fingerprint
            .as_ref()
            .map(|fingerprint| fingerprint.sha256.clone()),
    };
    prepared.report.fingerprints.output_before = output_fingerprint;
    prepared.report.expected_fingerprints = Some(expected);
    prepared.report.output_path = Some(request.output_path.display().to_string());
    prepared.report.dry_run = request.dry_run;

    if !request.dry_run {
        if prepared.report.unresolved_conflict_total != 0 {
            return Err(XcStringsError::MergeConflicts {
                count: prepared.report.unresolved_conflict_total,
            });
        }
        let introduced_error_count = prepared
            .report
            .introduced_validation_issues
            .iter()
            .filter(|issue| issue.severity == "error")
            .count();
        if introduced_error_count != 0 {
            return Err(XcStringsError::MergeIntroducedValidation {
                count: introduced_error_count,
            });
        }
        store.write_if_matches(
            &request.output_path,
            output_before.as_deref(),
            &prepared.content,
        )?;
        prepared.report.written = true;
    }

    Ok(MergeExecution {
        report: prepared.report,
        parsed: prepared.parsed,
    })
}

fn validate_catalog_path(path: &Path) -> Result<(), XcStringsError> {
    if path.extension().and_then(|extension| extension.to_str()) == Some("xcstrings") {
        Ok(())
    } else {
        Err(XcStringsError::NotXcStrings {
            path: path.to_path_buf(),
        })
    }
}

fn read_if_present(store: &dyn FileStore, path: &Path) -> Result<Option<Vec<u8>>, XcStringsError> {
    if store.exists(path) {
        store.read_bytes(path).map(Some)
    } else {
        Ok(None)
    }
}

fn verify_fingerprint(label: &str, bytes: &[u8], expected: &str) -> Result<(), XcStringsError> {
    if fingerprint(bytes) == expected {
        Ok(())
    } else {
        Err(XcStringsError::StaleMergeFingerprint {
            input: label.to_string(),
        })
    }
}

fn verify_output_fingerprint(
    bytes: Option<&[u8]>,
    expected: Option<&str>,
) -> Result<(), XcStringsError> {
    match (bytes, expected) {
        (None, None) => Ok(()),
        (Some(bytes), Some(expected)) if fingerprint(bytes) == expected => Ok(()),
        _ => Err(XcStringsError::StaleMergeFingerprint {
            input: "output".into(),
        }),
    }
}

fn file_fingerprint_without_schema(bytes: &[u8]) -> FileFingerprint {
    let value = super::semantic_merge::parse_raw(bytes, "output").unwrap_or(Value::Null);
    file_fingerprint(bytes, &value)
}
