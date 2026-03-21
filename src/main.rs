use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use rmcp::ServiceExt;
use tracing_subscriber::EnvFilter;

#[derive(clap::Parser)]
#[command(
    name = "xcstrings-mcp",
    about = "MCP server for iOS/macOS .xcstrings (String Catalog) localization.\n\n\
             22 tools + 7 prompts for the full localization lifecycle:\n\
             create → extract → translate → validate → export.\n\n\
             Communicates via stdio using the Model Context Protocol (MCP).\n\
             Works with any MCP client: Claude Code, Cursor, Windsurf, VS Code, Zed, OpenAI Codex.",
    version,
    after_help = "SETUP:\n  \
                  Claude Code:  claude mcp add xcstrings-mcp -- xcstrings-mcp\n  \
                  Cursor:       add to .cursor/mcp.json\n  \
                  Windsurf:     add to ~/.codeium/windsurf/mcp_config.json\n  \
                  VS Code:      add to .vscode/mcp.json\n  \
                  Zed:          add to settings.json under context_servers\n\n\
                  EXAMPLES:\n  \
                  xcstrings-mcp                              Start with default settings\n  \
                  xcstrings-mcp --glossary-path ./terms.json  Use custom glossary file\n\n\
                  ENVIRONMENT:\n  \
                  RUST_LOG=debug xcstrings-mcp               Enable debug logging to stderr"
)]
struct Cli {
    /// Path to glossary JSON file for consistent terminology across translations
    #[arg(long, default_value = "glossary.json")]
    glossary_path: PathBuf,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let store = Arc::new(xcstrings_mcp::io::fs::FsFileStore::new());
    let server = xcstrings_mcp::XcStringsMcpServer::new(store, cli.glossary_path);

    let transport = rmcp::transport::io::stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;

    Ok(())
}
