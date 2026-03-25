use std::process::ExitCode;

use clap::CommandFactory;
use clap_complete::Shell;

use super::common::EXIT_OK;

pub fn run_shell(shell: Shell) -> ExitCode {
    let mut cmd = crate::Cli::command();
    clap_complete::generate(shell, &mut cmd, "xcstrings-mcp", &mut std::io::stdout());
    ExitCode::from(EXIT_OK)
}
