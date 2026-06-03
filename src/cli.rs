use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "nit")]
#[command(version, about = "Git-native multi-repo workspaces", long_about = None)]
pub struct Cli {
    /// Skip automatic non-destructive upkeep for this invocation.
    #[arg(long, global = true, env = "NIT_NO_UPKEEP", hide = true)]
    pub no_upkeep: bool,

    /// Show upkeep actions that would normally be quiet.
    #[arg(long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Create Nit metadata in this workspace.
    Init {
        /// Create a tiny workspace-control repo when no natural root exists.
        #[arg(long)]
        control: bool,
        /// Keep metadata local-only.
        #[arg(long, conflicts_with = "control")]
        local: bool,
        /// Record the control repo remote.
        #[arg(long)]
        remote: Option<String>,
    },
    /// Register existing repo(s) as workspace members.
    Adopt {
        /// Existing Git repo path(s) to adopt.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Member id. Only valid when adopting one path.
        #[arg(long)]
        id: Option<String>,
        /// Do not auto-commit Nit metadata changes.
        #[arg(long)]
        no_commit: bool,
    },
    /// Diagnose the Nit installation and current workspace.
    Doctor,
    /// Show the current Nit workspace state.
    Status,
    /// Update the nit binary from the latest GitHub release.
    Update {
        /// Print the update action without running it.
        #[arg(long)]
        dry_run: bool,
        /// Allow update from a dev build.
        #[arg(long)]
        force: bool,
    },
}
