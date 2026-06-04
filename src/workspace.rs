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
    repair_required_excludes(&root, &roster)?;

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
            let roster = Roster::read(&root)?;
            println!("  roster members: {}", roster.members.len());
            repair_required_excludes(&root, &roster)?;
            println!("  exclude repair: ok");
            report_member_health(&root, &roster)?;
            report_pin_health(&root)?;
        }
        None => println!("  workspace: not found"),
    }

    println!("  upkeep: automatic non-destructive upkeep enabled");
    Ok(())
}

pub fn ignore(paths: Vec<PathBuf>) -> Result<()> {
    if paths.is_empty() {
        bail!("nothing specified; use `nit ignore <path>...`");
    }
    let cwd = env::current_dir()?;
    let root = find_nit_workspace(&cwd).context("not in a Nit workspace; run `nit init` first")?;
    let mut roster = Roster::read(&root)?;
    for path in paths {
        let rel = relative_to(&root, &absolutize(&cwd, &path))?;
        let entry = rel.to_string_lossy().to_string();
        if !roster.ignored.iter().any(|ignored| ignored == &entry) {
            roster.ignored.push(entry);
        }
    }
    roster.write(&root)?;
    repair_required_excludes(&root, &roster)?;
    commit_metadata(&root, "Update Nit ignored paths").ok();
    println!("updated ignored paths");
    Ok(())
}

pub fn import_submodule(path: PathBuf, id: Option<String>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = find_nit_workspace(&cwd).context("not in a Nit workspace; run `nit init` first")?;
    let abs = absolutize(&cwd, &path);
    let rel = relative_to(&root, &abs)?;
    let rel_text = rel.to_string_lossy().to_string();
    let status = git::output_in(&root, ["status", "--porcelain", "--untracked-files=no"])?;
    if !status.trim().is_empty() {
        bail!("root has tracked changes; commit or stash them before import-submodule");
    }
    let stage = git::output_in_args(&root, ["ls-files", "--stage", "--", rel_text.as_str()])?;
    if !stage.starts_with("160000 ") {
        bail!("{} is not a tracked Git submodule", path.display());
    }

    git::output_in_args(&root, ["rm", "--cached", rel_text.as_str()])?;
    let section = format!("submodule.{rel_text}");
    git::output_in_args(
        &root,
        ["config", "-f", ".gitmodules", "--remove-section", &section],
    )
    .ok();

    adopt(vec![path], id, true)?;
    git::output_in(&root, ["add", ".nit"])?;
    if root.join(".gitmodules").exists() {
        git::output_in(&root, ["add", ".gitmodules"])?;
    }
    git::output_in(
        &root,
        ["commit", "-m", &format!("Import Nit member {rel_text}")],
    )?;
    println!("imported submodule {rel_text}");
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

/// Reapply the roster's required member excludes and ignored paths to the root
/// repo's local `.git/info/exclude`. Returns the number of entries added; only
/// writes when something was missing, so it is a cheap no-op on a healthy tree.
pub(crate) fn repair_required_excludes(root: &Path, roster: &Roster) -> Result<usize> {
    let exclude = root.join(".git/info/exclude");
    if !exclude.exists() {
        return Ok(0);
    }

    let mut text = fs::read_to_string(&exclude).unwrap_or_default();
    let mut added = 0;
    for member in &roster.members {
        for entry in &member.required_excludes {
            added += append_exclude(&mut text, entry);
        }
    }
    for entry in &roster.ignored {
        added += append_exclude(&mut text, entry);
    }
    if added > 0 {
        fs::write(exclude, text).context("write git exclude")?;
    }
    Ok(added)
}

fn append_exclude(text: &mut String, entry: &str) -> usize {
    if text.lines().any(|line| line == entry) {
        return 0;
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(entry);
    text.push('\n');
    1
}

fn report_member_health(root: &Path, roster: &Roster) -> Result<()> {
    for member in &roster.members {
        let member_root = root.join(&member.path);
        if !member_root.exists() {
            println!("  member {}: missing", member.id);
            continue;
        }
        if !git::is_git_repo_root(&member_root) {
            println!("  member {}: not a git repo", member.id);
            continue;
        }
        if let Some(expected) = &member.remote {
            let actual = git::output_in(&member_root, ["remote", "get-url", "origin"])
                .unwrap_or_default()
                .trim()
                .to_string();
            if !actual.is_empty() && actual != *expected {
                println!("  member {}: remote drift", member.id);
            }
        }
    }
    Ok(())
}

fn report_pin_health(root: &Path) -> Result<()> {
    let pins_dir = root.join(crate::metadata::PINS_DIR);
    if !pins_dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(&pins_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("yaml") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let pin = crate::metadata::Pin::read(root, id)?;
        for member in &pin.members {
            let member_root = root.join(&member.path);
            if member_root.exists()
                && !git::status_in_args(
                    &member_root,
                    ["cat-file", "-e", &format!("{}^{{commit}}", member.commit)],
                )?
            {
                println!("  pin {}: dangling {}", pin.id, member.id);
            }
        }
    }
    Ok(())
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
