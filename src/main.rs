mod cli;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use rmcp::ServiceExt;
use tracing_subscriber::EnvFilter;

#[derive(clap::Parser)]
#[command(
    name = "xcstrings-mcp",
    about = "MCP server & CLI for iOS/macOS .xcstrings (String Catalog) localization.\n\n\
             26 MCP tools + 11 CLI commands for the full localization lifecycle:\n\
             migrate → create → extract → translate → validate → export.\n\n\
             Without a subcommand, starts the MCP server (stdio transport).\n\
             Use subcommands for direct CLI access to localization operations.",
    version,
    after_help = "MCP SETUP:\n  \
                  Claude Code:  claude mcp add xcstrings-mcp -- xcstrings-mcp\n  \
                  Cursor:       add to .cursor/mcp.json\n  \
                  Windsurf:     add to ~/.codeium/windsurf/mcp_config.json\n  \
                  VS Code:      add to .vscode/mcp.json\n  \
                  Zed:          add to settings.json under context_servers\n\n\
                  CLI EXAMPLES:\n  \
                  xcstrings-mcp coverage              Show translation coverage\n  \
                  xcstrings-mcp validate              Validate translations\n  \
                  xcstrings-mcp add-locale fr         Add a new locale\n  \
                  xcstrings-mcp export --locale de -o out.xliff   Export to XLIFF\n  \
                  xcstrings-mcp completions zsh       Generate shell completions\n\n\
                  ENVIRONMENT:\n  \
                  RUST_LOG=debug xcstrings-mcp        Enable debug logging to stderr"
)]
struct Cli {
    /// Path to glossary JSON file for consistent terminology across translations
    #[arg(long, default_value = "glossary.json")]
    glossary_path: PathBuf,

    /// Output JSON instead of human-readable text (CLI commands only)
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<cli::Command>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    match cli.command {
        None => {
            let store = Arc::new(xcstrings_mcp::io::fs::FsFileStore::new());
            let server = xcstrings_mcp::XcStringsMcpServer::new(store, cli.glossary_path);
            let transport = rmcp::transport::io::stdio();
            match server.serve(transport).await {
                Ok(service) => {
                    if let Err(e) = service.waiting().await {
                        eprintln!("error: {e}");
                        return ExitCode::FAILURE;
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some(cmd) => cli::run(cmd, cli.json),
    }
}
