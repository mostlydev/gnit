mod cli;
mod git;
mod metadata;
mod pin;
mod update;
mod upkeep;
mod workspace;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};

fn main() -> Result<()> {
    let cli = Cli::parse();

    if !cli.no_upkeep && !matches!(cli.command, Commands::Update { .. }) {
        upkeep::run_transparent_upkeep(cli.verbose);
    }

    match cli.command {
        Commands::Init {
            control,
            local,
            remote,
        } => workspace::init(control, local, remote),
        Commands::Adopt {
            paths,
            id,
            no_commit,
        } => workspace::adopt(paths, id, no_commit),
        Commands::Doctor => workspace::doctor(),
        Commands::Status => workspace::status(),
        Commands::Pin { name, no_commit } => pin::create(name, no_commit),
        Commands::Update { dry_run, force } => update::run(dry_run, force),
    }
}
