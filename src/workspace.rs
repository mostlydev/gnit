use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::git;
use crate::metadata::{Roster, RosterMember, ROSTER_PATH};

pub fn init(control: bool, local: bool, remote: Option<String>) -> Result<()> {
    let cwd = env::current_dir()?;
    let nit_dir = cwd.join(".nit");
    let roster = cwd.join(ROSTER_PATH);

    if roster.exists() {
        bail!("Nit workspace already exists at {}", cwd.display());
    }

    if control && !git::is_git_repo(&cwd) {
        git::output_in(&cwd, ["init"]).context("initialize control git repo")?;
    }

    fs::create_dir_all(&nit_dir).context("create .nit")?;
    let mode = if local {
        "local"
    } else if control {
        "control"
    } else {
        "shared"
    };
    Roster::new(mode, remote).write(&cwd)?;

    println!("initialized Nit workspace");
    println!("  root: {}", cwd.display());
    println!("  roster: {}", roster.display());
    Ok(())
}

pub fn adopt(paths: Vec<PathBuf>, id: Option<String>, no_commit: bool) -> Result<()> {
    if id.is_some() && paths.len() != 1 {
        bail!("--id can only be used when adopting one path");
    }

    let cwd = env::current_dir()?;
    let root = find_nit_workspace(&cwd).context("not in a Nit workspace; run `nit init` first")?;
    let mut roster = Roster::read(&root)?;
    let mut adopted = Vec::new();

    for path in paths {
        let abs = absolutize(&cwd, &path);
        if !git::is_git_repo_root(&abs) {
            if git::is_git_repo(&abs) {
                bail!(
                    "{} is a subdirectory of a Git repo, not a repository root; adopt the repo itself",
                    path.display()
                );
            }
            bail!("{} is not a Git repository", path.display());
        }

        let rel = relative_to(&root, &abs)?;
        let member_id = id.clone().unwrap_or_else(|| {
            rel.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
        if roster.contains_id(&member_id) {
            bail!("member id {member_id} already exists");
        }

        let remote = git::output_in(&abs, ["remote", "get-url", "origin"])
            .ok()
            .map(|remote| remote.trim().to_string())
            .filter(|remote| !remote.is_empty());
        let exclude_path = rel.to_string_lossy().to_string();
        roster.members.push(RosterMember {
            id: member_id.clone(),
            path: exclude_path.clone(),
            remote,
            required_excludes: vec![exclude_path],
        });
        adopted.push((member_id, rel));
    }

    roster.write(&root)?;
    repair_root_excludes(&root, &adopted)?;

    if !no_commit {
        commit_metadata(&root, "Update Nit roster").ok();
    }

    let ids = adopted
        .iter()
        .map(|(id, _)| id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    println!("adopted {ids}");
    Ok(())
}

pub fn doctor() -> Result<()> {
    println!("Nit doctor");
    println!("  version: {}", env!("CARGO_PKG_VERSION"));
    println!("  commit: {}", build_commit());

    match git::output(["--version"]) {
        Ok(version) => println!("  git: {}", version.trim()),
        Err(err) => println!("  git: not available ({err})"),
    }

    match find_nit_workspace(env::current_dir()?.as_path()) {
        Some(root) => {
            println!("  workspace: {}", root.display());
            println!("  roster members: {}", Roster::read(&root)?.members.len());
        }
        None => println!("  workspace: not found"),
    }

    println!("  upkeep: automatic non-destructive upkeep enabled");
    Ok(())
}

pub fn status() -> Result<()> {
    let cwd = env::current_dir()?;
    if let Some(root) = find_nit_workspace(&cwd) {
        let roster = Roster::read(&root)?;
        println!("Workspace {}", root.display());
        if roster.members.is_empty() {
            println!("Members: none");
        } else {
            println!("Members:");
            for member in roster.members {
                println!("  {}  {}", member.id, member.path);
            }
        }
        return Ok(());
    }

    println!("No Nit workspace found.");
    if let Some(root) = git::root(&cwd) {
        println!("Git root: {}", root.display());
    }
    println!("Run `nit init` to create one.");
    Ok(())
}

pub fn find_nit_workspace(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        if dir.join(ROSTER_PATH).exists() {
            return Some(dir.to_path_buf());
        }
    }
    None
}

fn absolutize(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn relative_to(root: &Path, path: &Path) -> Result<PathBuf> {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .with_context(|| format!("{} is outside workspace {}", path.display(), root.display()))
}

fn repair_root_excludes(root: &Path, adopted: &[(String, PathBuf)]) -> Result<()> {
    let exclude = root.join(".git/info/exclude");
    if !exclude.exists() {
        return Ok(());
    }

    let mut text = fs::read_to_string(&exclude).unwrap_or_default();
    for (_, path) in adopted {
        let entry = path.to_string_lossy();
        if !text.lines().any(|line| line == entry) {
            text.push_str(&format!("{entry}\n"));
        }
    }
    fs::write(exclude, text).context("write git exclude")
}

pub(crate) fn commit_metadata(root: &Path, message: &str) -> Result<()> {
    if !git::is_git_repo(root) {
        return Ok(());
    }

    // Local excludes (.git/info/exclude) are intentionally local; never committed.
    git::output_in(root, ["add", ".nit"])?;
    let status = git::output_in(root, ["status", "--porcelain", "--", ".nit"])?;
    if !status.trim().is_empty() {
        // Pathspec-scope the commit to .nit so an unrelated staged change in the
        // root repo is never swept into the Nit metadata commit.
        git::output_in(root, ["commit", "-m", message, "--", ".nit"])?;
    }
    Ok(())
}

fn build_commit() -> &'static str {
    match option_env!("NIT_COMMIT") {
        Some(commit) => commit,
        None => "dev",
    }
}
