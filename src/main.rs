mod change;
mod checkout;
mod cli;
mod clone;
mod git;
mod ids;
mod lock;
mod log;
mod metadata;
mod migrate;
mod pin;
mod pr;
mod push;
mod review;
mod skills;
mod status;
mod trailers;
mod update;
mod upkeep;
mod workspace;

use anyhow::Result;
use clap::Parser;
use cli::{ChangeCommands, Cli, Commands, PrCommands, SkillsCommands};

fn main() -> Result<()> {
    // Reset SIGPIPE to default so piping into `head`/`less` (closed early) exits
    // quietly like git/grep, instead of panicking with "Broken pipe".
    sigpipe::reset();

    let cli = Cli::parse();
    let command_lock = acquire_command_lock(&cli.command)?;

    if !cli.no_upkeep && !matches!(cli.command, Commands::Update { .. }) {
        if command_lock.is_some() {
            upkeep::run_transparent_upkeep_locked(cli.verbose);
        } else {
            upkeep::run_transparent_upkeep(cli.verbose);
        }
    }
    if !cli.no_upkeep && !matches!(cli.command, Commands::Update { .. }) {
        update::maybe_print_update_notice(cli.verbose);
    }

    match cli.command {
        Commands::Clone { url, path, pin } => clone::clone_workspace(url, path, pin),
        Commands::Add { paths, all, repo } => change::add(paths, all, repo),
        Commands::Commit { message, change } => change::commit(message, change).map(|_| ()),
        Commands::Land { name, message } => change::land(message, name),
        Commands::Checkout { pin, exact } => checkout::checkout(pin, exact),
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
        Commands::Ignore { paths } => workspace::ignore(paths),
        Commands::ImportSubmodule { path, id } => workspace::import_submodule(path, id),
        Commands::Doctor => workspace::doctor(),
        Commands::Migrate => migrate::run(),
        Commands::Status => status::status(),
        Commands::Log => log::workspace_log(),
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
        Commands::Push { resume } => push::push(resume),
        Commands::Pr { command, args } => match command {
            Some(PrCommands::Open(open_args)) => pr::open(open_args),
            None => pr::status(args),
        },
        Commands::Review { target } => review::review(target),
        Commands::Update {
            check,
            dry_run,
            force,
        } => {
            if check {
                update::check()
            } else {
                update::run(dry_run, force)
            }
        }
        Commands::Skills { command } => match command {
            SkillsCommands::Install(args) => skills::install(args),
            SkillsCommands::Uninstall(args) => skills::uninstall(args),
            SkillsCommands::List => skills::list(),
        },
    }
}

fn acquire_command_lock(command: &Commands) -> Result<Option<lock::WorkspaceLock>> {
    if !command_requires_workspace_lock(command) {
        return Ok(None);
    }
    let cwd = std::env::current_dir()?;
    let Some(root) = workspace::find_gnit_workspace(&cwd) else {
        return Ok(None);
    };
    lock::WorkspaceLock::acquire(&root).map(Some)
}

fn command_requires_workspace_lock(command: &Commands) -> bool {
    match command {
        Commands::Add { .. }
        | Commands::Commit { .. }
        | Commands::Land { .. }
        | Commands::Checkout { .. }
        | Commands::Adopt { .. }
        | Commands::Ignore { .. }
        | Commands::ImportSubmodule { .. }
        | Commands::Doctor
        | Commands::Migrate
        | Commands::Pin { .. }
        | Commands::Push { .. } => true,
        Commands::Clone { .. }
        | Commands::Init { .. }
        | Commands::Status
        | Commands::Log
        | Commands::Change { .. }
        | Commands::Pr { .. }
        | Commands::Review { .. }
        | Commands::Update { .. }
        | Commands::Skills { .. } => false,
    }
}
