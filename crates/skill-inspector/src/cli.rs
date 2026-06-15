//! CLI surface (contracts/cli.md): one binary, two subcommands. `scan` is pure-read;
//! mutation happens only via `serve`'s action API.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "cite",
    about = "Inspect, de-duplicate, and safely manage installed AI-agent skills."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Pure-read: build the unified inventory + overlap clusters; emit JSON or static HTML.
    Scan(ScanArgs),
    /// Interactive: serve the control-panel UI + the action API on loopback.
    Serve(ServeArgs),
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Json,
    Html,
}

#[derive(Args, Debug)]
pub struct ScanArgs {
    /// Which agent providers to scan (repeatable). Default: claude-code.
    #[arg(long = "agent")]
    pub agent: Vec<String>,
    /// Project root for project-level + activation context. Default: cwd.
    #[arg(long = "project")]
    pub project: Option<PathBuf>,
    /// Output format.
    #[arg(long = "format", value_enum, default_value_t = Format::Json)]
    pub format: Format,
    /// Output destination. Default: stdout.
    #[arg(long = "out")]
    pub out: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct ServeArgs {
    /// Which agent providers to scan (repeatable). Default: claude-code.
    #[arg(long = "agent")]
    pub agent: Vec<String>,
    /// Project root. Default: cwd.
    #[arg(long = "project")]
    pub project: Option<PathBuf>,
    /// Local port; bound to loopback only. Default: auto.
    #[arg(long = "port")]
    pub port: Option<u16>,
}

/// Resolve the requested agents, defaulting to the primary agent when none given.
pub fn agents_or_default(agents: &[String]) -> Vec<String> {
    if agents.is_empty() {
        vec!["claude-code".to_string()]
    } else {
        agents.to_vec()
    }
}
