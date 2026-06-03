mod change;
mod cli;
mod git;
mod metadata;
mod pin;
mod update;
mod upkeep;
mod workspace;

use anyhow::Result;
use clap::Parser;
use cli::{ChangeCommands, Cli, Commands};

fn main() -> Result<()> {
    let cli = Cli::parse();

    if !cli.no_upkeep && !matches!(cli.command, Commands::Update { .. }) {
        upkeep::run_transparent_upkeep(cli.verbose);
    }

    match cli.command {
        Commands::Add { paths, all, repo } => change::add(paths, all, repo),
        Commands::Commit { message } => change::commit(message).map(|_| ()),
        Commands::Land { name, message } => change::land(message, name),
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
        Commands::Pin {
            name,
            change,
            no_commit,
        } => {
            if let Some(change) = change {
                change::ensure_exists(&change)?;
                pin::create_with_changes(name, vec![change], no_commit)
            } else {
                pin::create(name, no_commit)
            }
        }
        Commands::Change { command } => match command {
            ChangeCommands::Show { id } => change::show(id),
            ChangeCommands::Status { id } => change::status(id),
            ChangeCommands::Log { id } => change::log(id),
            ChangeCommands::Diff { id } => change::diff(id),
        },
        Commands::Update { dry_run, force } => update::run(dry_run, force),
    }
}
