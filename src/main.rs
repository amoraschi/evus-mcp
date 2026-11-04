mod auth;
mod bb_client;
mod cli;
mod credentials;
mod extract;
mod mcp;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "evus-mcp", about = "ev.us.es (Blackboard Learn / UVUS) MCP server")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the MCP stdio server (default if no subcommand is given)
    Serve,
    /// Standalone SAML+TOTP login test against ev.us.es
    Login,
    /// Prompt for UVUS credentials and store them in the OS keychain
    SetCreds,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => mcp::run().await,
        Command::Login => cli::login::run().await,
        Command::SetCreds => cli::set_creds::run(),
    }
}
