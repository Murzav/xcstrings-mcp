use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use rmcp::ServiceExt;
use tracing_subscriber::EnvFilter;

#[derive(clap::Parser)]
#[command(
    name = "xcstrings-mcp",
    about = "MCP server for .xcstrings localization files",
    version
)]
struct Cli {
    /// Path to glossary file for consistent terminology
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
