use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::git;
use crate::metadata::{Pin, Roster, PINS_DIR};
use crate::workspace;

/// Rich, grouped `nit status`: per-member staged/unstaged/untracked counts,
/// branch, missing members, drift from the current pin, and discovered-but-
/// unadopted nested repos. Falls back to a clear message outside a workspace.
pub fn status() -> Result<()> {
    let cwd = env::current_dir()?;
    let Some(root) = workspace::find_nit_workspace(&cwd) else {
        println!("No Nit workspace found.");
        if let Some(git_root) = git::root(&cwd) {
            println!("Git root: {}", git_root.display());
        }
        println!("Run `nit init` to create one.");
        return Ok(());
    };

    let roster = Roster::read(&root)?;
    let current_pin = newest_pin(&root)?;

    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.display().to_string());
    println!("Workspace {name}   root: {}", root.display());
    match &current_pin {
        Some(pin) => println!("Current pin: {}", pin.label.as_deref().unwrap_or(&pin.id)),
        None => println!("Current pin: none"),
    }

    println!("\nMembers");
    let mut listed = false;
    // The root repo participates in commits and pins too, so show its own
    // working-tree state first (excluding `.nit/`, which `nit add -A` excludes).
    if git::is_git_repo_root(&root) {
        let line = repo_line(&root, "root", true, current_pin.as_ref());
        println!("  {:<10}  {}", "root", line);
        listed = true;
    }
    for member in &roster.members {
        let member_root = root.join(&member.path);
        let line = repo_line(&member_root, &member.id, false, current_pin.as_ref());
        println!("  {:<10}  {}", member.id, line);
        listed = true;
    }
    if !listed {
        println!("  (none yet -- run `nit adopt <repo>`)");
    }

    let discovered = discover_unadopted(&root, &roster);
    if !discovered.is_empty() {
        println!("\nDiscovered (not adopted)");
        for path in discovered {
            println!("  {:<10}  -> nit adopt {path} | nit ignore {path}", path);
        }
    }

    Ok(())
}

fn repo_line(repo_root: &Path, id: &str, is_root: bool, pin: Option<&Pin>) -> String {
    if !repo_root.exists() {
        return "missing locally  -> nit checkout <pin>".to_string();
    }
    if !git::is_git_repo_root(repo_root) {
        return "not a git repo".to_string();
    }

    let branch = git::output_in(repo_root, ["rev-parse", "--abbrev-ref", "HEAD"])
        .map(|b| b.trim().to_string())
        .unwrap_or_default();
    let on = if branch == "HEAD" || branch.is_empty() {
        "detached".to_string()
    } else {
        format!("on {branch}")
    };

    let work = match git::output_in(repo_root, ["status", "--porcelain"]) {
        Ok(porcelain) => {
            // The root tracks `.nit/` metadata; exclude it so the line reflects
            // the root's *code* state, matching what `nit add -A`/`nit commit` do.
            let text = if is_root {
                porcelain
                    .lines()
                    .filter(|line| !is_nit_entry(line))
                    .map(|line| format!("{line}\n"))
                    .collect::<String>()
            } else {
                porcelain
            };
            describe_worktree(&text)
        }
        Err(_) => "status unavailable".to_string(),
    };

    let drift = pin
        .and_then(|pin| pin.members.iter().find(|m| m.id == id))
        .and_then(|pinned| {
            let head = git::output_in(repo_root, ["rev-parse", "HEAD"]).ok()?;
            (head.trim() != pinned.commit).then_some("  drifted from pin")
        })
        .unwrap_or("");

    format!("{work}   {on}{drift}")
}

/// True for a `git status --porcelain` line whose path is the Nit metadata dir.
fn is_nit_entry(porcelain_line: &str) -> bool {
    porcelain_line
        .get(3..)
        .map(|path| path == ".nit" || path.starts_with(".nit/"))
        .unwrap_or(false)
}

fn describe_worktree(porcelain: &str) -> String {
    let (mut staged, mut modified, mut untracked) = (0u32, 0u32, 0u32);
    for line in porcelain.lines() {
        if line.len() < 2 {
            continue;
        }
        let bytes = line.as_bytes();
        let (x, y) = (bytes[0] as char, bytes[1] as char);
        if x == '?' && y == '?' {
            untracked += 1;
            continue;
        }
        if x != ' ' && x != '?' {
            staged += 1;
        }
        if y != ' ' && y != '?' {
            modified += 1;
        }
    }
    if staged == 0 && modified == 0 && untracked == 0 {
        return "clean".to_string();
    }
    let mut parts = Vec::new();
    if staged > 0 {
        parts.push(format!("{staged} staged"));
    }
    if modified > 0 {
        parts.push(format!("{modified} modified"));
    }
    if untracked > 0 {
        parts.push(format!("{untracked} untracked"));
    }
    parts.join(", ")
}

/// The newest pin in `.nit/pins/` by id (ids embed a millis timestamp, so the
/// lexicographically greatest id is the most recent).
pub fn newest_pin(root: &Path) -> Result<Option<Pin>> {
    let pins_dir = root.join(PINS_DIR);
    if !pins_dir.exists() {
        return Ok(None);
    }
    let mut ids: Vec<String> = Vec::new();
    for entry in fs::read_dir(&pins_dir).with_context(|| format!("read {}", pins_dir.display()))? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                ids.push(stem.to_string());
            }
        }
    }
    let Some(newest) = ids.into_iter().max() else {
        return Ok(None);
    };
    Ok(Some(Pin::read(root, &newest)?))
}

/// Find nested Git repositories under the workspace that are neither adopted
/// members nor ignored. Prunes at repo boundaries and caps depth so it stays
/// fast on large trees.
fn discover_unadopted(root: &Path, roster: &Roster) -> Vec<String> {
    let members: BTreeSet<PathBuf> = roster.members.iter().map(|m| root.join(&m.path)).collect();
    let ignored: BTreeSet<PathBuf> = roster.ignored.iter().map(|p| root.join(p)).collect();
    let mut found = Vec::new();
    walk(root, root, &members, &ignored, 0, &mut found);
    found.sort();
    found
}

fn walk(
    root: &Path,
    dir: &Path,
    members: &BTreeSet<PathBuf>,
    ignored: &BTreeSet<PathBuf>,
    depth: usize,
    found: &mut Vec<String>,
) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if name == ".git" || name == ".nit" {
            continue;
        }
        if ignored.contains(&path) {
            continue;
        }
        let is_member = members.contains(&path);
        let is_repo = path.join(".git").exists();
        if is_repo {
            // Stop at any repo boundary; report it only if it is unmanaged.
            if !is_member {
                if let Ok(rel) = path.strip_prefix(root) {
                    found.push(rel.to_string_lossy().to_string());
                }
            }
            continue;
        }
        walk(root, &path, members, ignored, depth + 1, found);
    }
}
