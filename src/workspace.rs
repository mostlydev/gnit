use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::git;

const ROSTER: &str = ".nit/roster.yaml";

pub fn init(control: bool, local: bool, remote: Option<String>) -> Result<()> {
    let cwd = env::current_dir()?;
    let nit_dir = cwd.join(".nit");
    let roster = cwd.join(ROSTER);

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
    let remote_line = remote
        .as_ref()
        .map(|url| format!("remote: {url}\n"))
        .unwrap_or_default();
    fs::write(
        &roster,
        format!("version: 1\nmode: {mode}\n{remote_line}members:\n"),
    )
    .context("write roster")?;

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
    let roster_path = root.join(ROSTER);
    let mut roster = fs::read_to_string(&roster_path).context("read roster")?;
    let mut adopted = Vec::new();

    for path in paths {
        let abs = absolutize(&cwd, &path);
        if !git::is_git_repo(&abs) {
            bail!("{} is not a Git repository", path.display());
        }

        let rel = relative_to(&root, &abs)?;
        let member_id = id.clone().unwrap_or_else(|| {
            rel.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
        if roster_contains_id(&roster, &member_id) {
            bail!("member id {member_id} already exists");
        }

        roster.push_str(&format!(
            "  - id: {member_id}\n    path: {}\n",
            rel.display()
        ));
        if let Ok(remote) = git::output_in(&abs, ["remote", "get-url", "origin"]) {
            roster.push_str(&format!("    remote: {}\n", remote.trim()));
        }
        adopted.push((member_id, rel));
    }

    fs::write(&roster_path, roster).context("write roster")?;
    repair_root_excludes(&root, &adopted)?;

    if !no_commit {
        auto_commit_metadata(&root, "Update Nit roster").ok();
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
            println!(
                "  roster members: {}",
                roster_members(&root.join(ROSTER))?.len()
            );
        }
        None => println!("  workspace: not found"),
    }

    println!("  upkeep: automatic non-destructive upkeep enabled");
    Ok(())
}

pub fn status() -> Result<()> {
    let cwd = env::current_dir()?;
    if let Some(root) = find_nit_workspace(&cwd) {
        let members = roster_members(&root.join(ROSTER))?;
        println!("Workspace {}", root.display());
        if members.is_empty() {
            println!("Members: none");
        } else {
            println!("Members:");
            for member in members {
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
        if dir.join(ROSTER).exists() {
            return Some(dir.to_path_buf());
        }
    }
    None
}

#[derive(Debug, Eq, PartialEq)]
struct Member {
    id: String,
    path: String,
}

fn roster_members(path: &Path) -> Result<Vec<Member>> {
    let text = fs::read_to_string(path).context("read roster")?;
    let mut members = Vec::new();
    let mut current_id: Option<String> = None;

    for line in text.lines() {
        if let Some(id) = line.strip_prefix("  - id: ") {
            current_id = Some(id.to_string());
        } else if let Some(member_path) = line.strip_prefix("    path: ") {
            if let Some(id) = current_id.take() {
                members.push(Member {
                    id,
                    path: member_path.to_string(),
                });
            }
        }
    }

    Ok(members)
}

fn roster_contains_id(roster: &str, id: &str) -> bool {
    roster.lines().any(|line| line == format!("  - id: {id}"))
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

fn auto_commit_metadata(root: &Path, message: &str) -> Result<()> {
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
