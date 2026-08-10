use std::path::PathBuf;
use std::process::ExitCode;

use xcstrings_mcp::error::XcStringsError;
use xcstrings_mcp::io::fs::FsFileStore;
use xcstrings_mcp::service::merge_operation::{MergeRequest, execute_merge};
use xcstrings_mcp::service::semantic_merge::{
    ConflictChoice, ConflictResolution, ExpectedFingerprints,
};

use super::common::{EXIT_ERROR, EXIT_OK, EXIT_VALIDATION_ISSUES};

#[allow(clippy::too_many_arguments)]
pub fn run(
    base: PathBuf,
    current: PathBuf,
    incoming: PathBuf,
    output: PathBuf,
    dry_run: bool,
    resolution_args: Vec<String>,
    expected_json: Option<String>,
    conflict_offset: usize,
    conflict_limit: usize,
    pretty: bool,
) -> ExitCode {
    let result = parse_inputs(resolution_args, expected_json).and_then(
        |(resolutions, expected_fingerprints)| {
            execute_merge(
                &FsFileStore::new(),
                &MergeRequest {
                    base_path: base,
                    current_path: current,
                    incoming_path: incoming,
                    output_path: output,
                    dry_run,
                    resolutions,
                    expected_fingerprints,
                    conflict_offset,
                    conflict_limit,
                },
            )
        },
    );

    match result {
        Ok(execution) => {
            let serialized = if pretty {
                serde_json::to_string_pretty(&execution.report)
            } else {
                serde_json::to_string(&execution.report)
            };
            match serialized {
                Ok(json) => {
                    println!("{json}");
                    let exit = if execution.report.unresolved_conflict_total == 0 {
                        EXIT_OK
                    } else {
                        EXIT_VALIDATION_ISSUES
                    };
                    ExitCode::from(exit)
                }
                Err(error) => {
                    eprintln!("error: cannot serialize merge report: {error}");
                    ExitCode::from(EXIT_ERROR)
                }
            }
        }
        Err(error) => {
            let exit = if matches!(
                error,
                XcStringsError::MergeConflicts { .. }
                    | XcStringsError::MergeIntroducedValidation { .. }
            ) {
                EXIT_VALIDATION_ISSUES
            } else {
                EXIT_ERROR
            };
            eprintln!("error: {error}");
            ExitCode::from(exit)
        }
    }
}

fn parse_inputs(
    resolution_args: Vec<String>,
    expected_json: Option<String>,
) -> Result<(Vec<ConflictResolution>, Option<ExpectedFingerprints>), XcStringsError> {
    let resolutions = resolution_args
        .into_iter()
        .map(|value| parse_resolution(&value))
        .collect::<Result<Vec<_>, _>>()?;
    let expected = expected_json
        .map(|value| {
            serde_json::from_str(&value).map_err(|error| {
                XcStringsError::InvalidFormat(format!(
                    "invalid --expected-fingerprints JSON: {error}"
                ))
            })
        })
        .transpose()?;
    Ok((resolutions, expected))
}

fn parse_resolution(value: &str) -> Result<ConflictResolution, XcStringsError> {
    let Some((conflict_id, choice)) = value.rsplit_once('=') else {
        return Err(XcStringsError::InvalidFormat(
            "--resolution must be <conflict-id>=current|incoming|base".into(),
        ));
    };
    if conflict_id.is_empty() {
        return Err(XcStringsError::InvalidFormat(
            "--resolution conflict ID cannot be empty".into(),
        ));
    }
    let choice = match choice {
        "current" => ConflictChoice::Current,
        "incoming" => ConflictChoice::Incoming,
        "base" => ConflictChoice::Base,
        _ => {
            return Err(XcStringsError::InvalidFormat(
                "--resolution choice must be current, incoming, or base".into(),
            ));
        }
    };
    Ok(ConflictResolution {
        conflict_id: conflict_id.to_string(),
        choice,
    })
}
