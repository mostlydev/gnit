use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "gnit")]
#[command(version, about = "Git-native multi-repo workspaces", long_about = None)]
pub struct Cli {
    /// Skip automatic non-destructive upkeep for this invocation.
    #[arg(long, global = true, env = "GNIT_NO_UPKEEP", hide = true)]
    pub no_upkeep: bool,

    /// Show upkeep actions that would normally be quiet.
    #[arg(long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Clone a Gnit workspace and hydrate its member repos.
    Clone {
        /// Workspace/root repository URL.
        url: String,
        /// Destination path. Defaults to the repository name.
        path: Option<PathBuf>,
        /// Materialize this Pin after cloning.
        #[arg(long)]
        pin: Option<String>,
    },
    /// Stage paths across workspace members.
    Add {
        /// Stage all changes in the workspace.
        #[arg(short = 'A', long)]
        all: bool,
        /// Interpret paths relative to one member id.
        #[arg(long)]
        repo: Option<String>,
        /// Paths to stage.
        paths: Vec<PathBuf>,
    },
    /// Commit staged workspace changes with a shared Gnit-Change-Id.
    Commit {
        /// Commit message.
        #[arg(short, long)]
        message: String,
    },
    /// Commit staged changes and create a Pin.
    Land {
        /// Optional label for the resulting Pin.
        name: Option<String>,
        /// Commit message.
        #[arg(short, long)]
        message: String,
    },
    /// Materialize a Pin across workspace members.
    Checkout {
        /// Pin id or label.
        pin: String,
        /// Reset and clean dirty member worktrees.
        #[arg(long)]
        exact: bool,
    },
    /// Create Gnit metadata in this workspace.
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
        /// Do not auto-commit Gnit metadata changes.
        #[arg(long)]
        no_commit: bool,
    },
    /// Exclude non-member paths from the workspace root.
    Ignore {
        /// Paths to ignore in the root repo.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Convert a tracked Git submodule into a Gnit member.
    ImportSubmodule {
        /// Submodule path.
        path: PathBuf,
        /// Member id.
        #[arg(long)]
        id: Option<String>,
    },
    /// Diagnose the Gnit installation and current workspace.
    Doctor,
    /// Show the current Gnit workspace state.
    Status,
    /// Show the unified workspace timeline of Changes and Pins.
    Log,
    /// Create a reproducible pin for committed member state.
    Pin {
        /// Optional human label for the pin.
        name: Option<String>,
        /// Record provenance from a Change.
        #[arg(long)]
        change: Option<String>,
        /// Do not auto-commit Gnit metadata changes.
        #[arg(long)]
        no_commit: bool,
    },
    /// Inspect changes reconstructed from Gnit-Change-Id trailers.
    Change {
        #[command(subcommand)]
        command: ChangeCommands,
    },
    /// Push member repos first, then the workspace metadata repo.
    Push {
        /// Retry the ordered push after a previous failure.
        #[arg(long)]
        resume: bool,
    },
    /// Show or open linked GitHub PRs for a workspace Change.
    Pr {
        #[command(subcommand)]
        command: Option<PrCommands>,
        #[command(flatten)]
        args: PrStatusArgs,
    },
    /// Render a combined review artifact for a Change or Pin.
    Review {
        /// Change id or Pin id/label.
        target: String,
    },
    /// Update the gnit binary from the latest GitHub release.
    Update {
        /// Check the latest release and refresh cached release metadata.
        #[arg(long, conflicts_with_all = ["dry_run", "force"])]
        check: bool,
        /// Print the update action without running it.
        #[arg(long)]
        dry_run: bool,
        /// Allow update from a dev build.
        #[arg(long)]
        force: bool,
    },
    /// Install the bundled Gnit skill into agent harnesses.
    Skills {
        #[command(subcommand)]
        command: SkillsCommands,
    },
}

#[derive(Debug, Subcommand)]
pub enum ChangeCommands {
    /// Show participant commits for a Change.
    Show { id: String },
    /// Show projection/ambiguity status for a Change.
    Status { id: String },
    /// List Changes, or commits for one Change.
    Log { id: Option<String> },
    /// Show per-repo diffs for a Change.
    Diff { id: String },
}

#[derive(Debug, Subcommand)]
pub enum PrCommands {
    /// Create, adopt, and refresh linked draft PRs.
    Open(PrOpenArgs),
}

#[derive(Debug, Args, Clone)]
pub struct PrStatusArgs {
    /// Change id to project. Defaults to the single Change on the current branch.
    #[arg(long, conflicts_with = "pin")]
    pub change: Option<String>,
    /// Pin id or label. Must record exactly one provenance Change.
    #[arg(long, conflicts_with = "change")]
    pub pin: Option<String>,
    /// Base branch for all repos. Defaults to each repo's origin default branch.
    #[arg(long)]
    pub base: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct PrOpenArgs {
    /// Change id to project. Defaults to the single Change on the current branch.
    #[arg(long, conflicts_with = "pin")]
    pub change: Option<String>,
    /// Pin id or label. Must record exactly one provenance Change.
    #[arg(long, conflicts_with = "change")]
    pub pin: Option<String>,
    /// Base branch for all repos. Defaults to each repo's origin default branch.
    #[arg(long)]
    pub base: Option<String>,
    /// Pull request title. Defaults to the Change commit subject.
    #[arg(long)]
    pub title: Option<String>,
    /// Head branch override for detached worktrees.
    #[arg(long)]
    pub branch: Option<String>,
    /// Open PRs ready for review. The default is draft.
    #[arg(long)]
    pub ready: bool,
}

#[derive(Debug, Subcommand)]
pub enum SkillsCommands {
    /// Install the bundled Gnit skill.
    Install(SkillsInstallArgs),
    /// Remove the bundled Gnit skill from harness skill directories.
    Uninstall(SkillsUninstallArgs),
    /// Show installed Gnit skill state.
    List,
}

#[derive(Debug, Args)]
pub struct SkillsInstallArgs {
    /// Harnesses to install into: claude, claude-code, codex, opencode, grok, grok-build.
    pub harnesses: Vec<String>,
    /// Install into every detected supported harness.
    #[arg(long)]
    pub all: bool,
    /// Copy the skill instead of linking it.
    #[arg(long, conflicts_with = "link")]
    pub copy: bool,
    /// Link the skill to Gnit's managed source. This is the default.
    #[arg(long)]
    pub link: bool,
    /// Print planned actions without changing files.
    #[arg(long)]
    pub print: bool,
    /// Replace an existing non-Gnit-owned skill target.
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct SkillsUninstallArgs {
    /// Harnesses to uninstall from: claude, claude-code, codex, opencode, grok, grok-build.
    pub harnesses: Vec<String>,
    /// Uninstall from every detected supported harness.
    #[arg(long)]
    pub all: bool,
    /// Print planned actions without changing files.
    #[arg(long)]
    pub print: bool,
}
