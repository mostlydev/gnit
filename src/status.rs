use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::git;
use crate::metadata::{Pin, Roster, PINS_DIR};
use crate::workspace;

/// Rich, grouped `gnit status`: root/member staged/unstaged/untracked counts,
/// branch, missing members, member drift from the current pin, and discovered-
/// but-unadopted nested repos. Falls back to a clear message outside a workspace.
pub fn status() -> Result<()> {
    let cwd = env::current_dir()?;
    let Some(root) = workspace::find_gnit_workspace(&cwd) else {
        println!("No Gnit workspace found.");
        if let Some(git_root) = git::root(&cwd) {
            println!("Git root: {}", git_root.display());
        }
        println!("Run `gnit init` to create one.");
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

    println!("\nRepos");
    let mut listed = false;
    // The root repo participates in workspace commits, so show its own state
    // first. `.gnit/` metadata stays out of this line to match `gnit add -A`.
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
        println!("  (none yet -- run `gnit adopt <repo>`)");
    }

    let discovered = discover_unadopted(&root, &roster);
    if !discovered.is_empty() {
        println!("\nDiscovered (not adopted)");
        for path in discovered {
            println!("  {:<10}  -> gnit adopt {path} | gnit ignore {path}", path);
        }
    }

    Ok(())
}

fn repo_line(repo_root: &Path, id: &str, is_root: bool, pin: Option<&Pin>) -> String {
    if !repo_root.exists() {
        return "missing locally  -> gnit checkout <pin>".to_string();
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

    let work = match git::output_in_args(repo_root, ["status", "--porcelain=v2", "-z"]) {
        Ok(porcelain) => {
            let entries = status_entries(&porcelain);
            // The root tracks `.gnit/` metadata; exclude entries that are wholly
            // metadata so the line reflects the root's code state. Mixed-path
            // renames/copies still count because they move content across the
            // metadata boundary.
            if is_root {
                describe_worktree(entries.iter().filter(|entry| !entry.is_gnit_only()))
            } else {
                describe_worktree(entries.iter())
            }
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct StatusEntry {
    x: char,
    y: char,
    paths: Vec<String>,
}

impl StatusEntry {
    fn is_gnit_only(&self) -> bool {
        !self.paths.is_empty() && self.paths.iter().all(|path| is_gnit_path(path))
    }
}

fn is_gnit_path(path: &str) -> bool {
    path == ".gnit" || path.starts_with(".gnit/")
}

fn status_entries(porcelain: &str) -> Vec<StatusEntry> {
    let mut entries = Vec::new();
    let mut fields = porcelain.split('\0').filter(|field| !field.is_empty());
    while let Some(record) = fields.next() {
        if let Some(path) = record.strip_prefix("? ") {
            entries.push(StatusEntry {
                x: '?',
                y: '?',
                paths: vec![path.to_string()],
            });
            continue;
        }
        if let Some(path) = record.strip_prefix("! ") {
            entries.push(StatusEntry {
                x: '!',
                y: '!',
                paths: vec![path.to_string()],
            });
            continue;
        }
        if let Some(rest) = record.strip_prefix("1 ") {
            let parts = rest.splitn(8, ' ').collect::<Vec<_>>();
            if parts.len() == 8 {
                entries.push(StatusEntry {
                    x: status_char(parts[0], 0),
                    y: status_char(parts[0], 1),
                    paths: vec![parts[7].to_string()],
                });
            }
            continue;
        }
        if let Some(rest) = record.strip_prefix("2 ") {
            let parts = rest.splitn(9, ' ').collect::<Vec<_>>();
            if parts.len() == 9 {
                let original_path = fields.next().unwrap_or_default();
                entries.push(StatusEntry {
                    x: status_char(parts[0], 0),
                    y: status_char(parts[0], 1),
                    paths: vec![parts[8].to_string(), original_path.to_string()],
                });
            }
            continue;
        }
        if let Some(rest) = record.strip_prefix("u ") {
            let parts = rest.splitn(10, ' ').collect::<Vec<_>>();
            if parts.len() == 10 {
                entries.push(StatusEntry {
                    x: status_char(parts[0], 0),
                    y: status_char(parts[0], 1),
                    paths: vec![parts[9].to_string()],
                });
            }
        }
    }
    entries
}

fn status_char(xy: &str, index: usize) -> char {
    xy.chars().nth(index).unwrap_or('.')
}

fn is_changed_status(status: char) -> bool {
    !matches!(status, '.' | ' ' | '?' | '!')
}

fn describe_worktree<'a>(entries: impl IntoIterator<Item = &'a StatusEntry>) -> String {
    let (mut staged, mut modified, mut untracked) = (0u32, 0u32, 0u32);
    for entry in entries {
        if entry.x == '?' && entry.y == '?' {
            untracked += 1;
            continue;
        }
        if is_changed_status(entry.x) {
            staged += 1;
        }
        if is_changed_status(entry.y) {
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

/// The newest pin in `.gnit/pins/` by id (ids embed a millis timestamp, so the
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
        if name == ".git" || name == ".gnit" {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_v2_z_copy_across_gnit_boundary_counts_as_root_work() {
        let porcelain = concat!(
            "2 C. N... 100644 100644 100644 abc abc C100 .gnit/copied.txt\0",
            "src.txt\0",
            "? .gnit/local-noise.txt\0"
        );
        let entries = status_entries(porcelain);

        assert_eq!(entries.len(), 2);
        assert!(!entries[0].is_gnit_only());
        assert!(entries[1].is_gnit_only());
        assert_eq!(
            describe_worktree(entries.iter().filter(|entry| !entry.is_gnit_only())),
            "1 staged"
        );
    }
}
