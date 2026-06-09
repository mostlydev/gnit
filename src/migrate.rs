use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::git;
use crate::workspace;

const LEGACY_ROSTER_PATH: &str = ".nit/roster.yaml";
pub(crate) const LEGACY_GUIDANCE_START: &str = "<!-- nit:workspace:start -->";
const LEGACY_GUIDANCE_END: &str = "<!-- nit:workspace:end -->";

/// Nearest ancestor that still carries pre-rename `.nit/` metadata.
pub fn find_legacy_workspace(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join(LEGACY_ROSTER_PATH).exists())
        .map(Path::to_path_buf)
}

/// Migrate a pre-rename nit workspace in place: move `.nit/` to `.gnit/`,
/// replace the legacy agent-guidance block with the current one, and commit
/// the result as one metadata commit. Re-running is a no-op.
pub fn run() -> Result<()> {
    let cwd = env::current_dir()?;
    let gnit_root = workspace::find_gnit_workspace(&cwd);
    let legacy_root = find_legacy_workspace(&cwd);

    match (gnit_root, legacy_root) {
        (Some(gnit), Some(legacy)) if gnit == legacy => bail!(
            "both .nit and .gnit metadata exist at {}; keep one and remove the other, then re-run",
            gnit.display()
        ),
        (_, Some(root)) => migrate_root(&root),
        (Some(root), None) => refresh_guidance(&root),
        (None, None) => bail!(
            "no nit or gnit workspace found from {}; run `gnit init` to create one",
            cwd.display()
        ),
    }
}

fn migrate_root(root: &Path) -> Result<()> {
    let old = root.join(".nit");
    let new = root.join(".gnit");
    if new.exists() {
        bail!(
            "{} already exists; keep one metadata directory and remove the other, then re-run",
            new.display()
        );
    }

    // `git mv` keeps the rename staged in one step when `.nit` is tracked;
    // fall back to a plain rename for untracked metadata (local mode).
    if !git::is_git_repo(root) || git::output_in(root, ["mv", ".nit", ".gnit"]).is_err() {
        fs::rename(&old, &new).with_context(|| format!("rename {} to .gnit", old.display()))?;
    }

    println!("migrated workspace");
    println!("  root: {}", root.display());
    println!("  metadata: .nit -> .gnit");

    let guidance = refresh_guidance_files(root)?;
    report_guidance(&guidance);

    if commit_migration(root, &guidance)? {
        println!("  committed: Migrate workspace metadata to gnit");
    }
    println!("note: commits keep their old Nit-Change-Id trailers; new commits use Gnit-Change-Id");
    Ok(())
}

/// Already-renamed workspace: only the agent guidance may still be stale.
fn refresh_guidance(root: &Path) -> Result<()> {
    let guidance = refresh_guidance_files(root)?;
    if guidance.is_empty() {
        println!("nothing to migrate; workspace already uses .gnit metadata");
        return Ok(());
    }
    report_guidance(&guidance);
    if commit_migration(root, &guidance)? {
        println!("  committed: Migrate workspace metadata to gnit");
    }
    Ok(())
}

/// Strip well-formed legacy guidance blocks, then let the regular guidance
/// repair lay down the current block. Returns every file that changed.
fn refresh_guidance_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut changed = Vec::new();
    for name in ["AGENTS.md", "CLAUDE.md"] {
        let path = root.join(name);
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if let Some(stripped) = strip_legacy_block(&text) {
            fs::write(&path, stripped)
                .with_context(|| format!("write agent guidance {}", path.display()))?;
            changed.push(PathBuf::from(name));
        }
    }
    for added in workspace::ensure_agent_guidance(root)? {
        if !changed.contains(&added) {
            changed.push(added);
        }
    }
    Ok(changed)
}

fn report_guidance(changed: &[PathBuf]) {
    if changed.is_empty() {
        println!("  agent guidance: ok");
    } else {
        let names = changed
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        println!("  agent guidance: refreshed ({names})");
    }
}

fn strip_legacy_block(text: &str) -> Option<String> {
    let start = text.find(LEGACY_GUIDANCE_START)?;
    let end_rel = text[start..].find(LEGACY_GUIDANCE_END)?;
    let mut end = start + end_rel + LEGACY_GUIDANCE_END.len();
    if text[end..].starts_with('\n') {
        end += 1;
    }
    Some(format!("{}{}", &text[..start], &text[end..]))
}

/// Commit the rename and guidance refresh, pathspec-scoped like every other
/// Gnit metadata commit so unrelated staged work is never swept in.
fn commit_migration(root: &Path, guidance: &[PathBuf]) -> Result<bool> {
    if !git::is_git_repo(root) {
        return Ok(false);
    }

    let mut paths = vec![".gnit".to_string(), ".nit".to_string()];
    for path in guidance {
        let path = path.to_string_lossy().to_string();
        git::output_in_args(root, ["add", "--", path.as_str()])?;
        paths.push(path);
    }

    let mut status_args = vec!["status".to_string(), "--porcelain".to_string()];
    status_args.push("--".to_string());
    status_args.extend(paths.iter().cloned());
    let status = git::output_in_args(root, &status_args)?;
    if status.trim().is_empty() {
        return Ok(false);
    }

    let mut commit_args = vec![
        "commit".to_string(),
        "-m".to_string(),
        "Migrate workspace metadata to gnit".to_string(),
    ];
    commit_args.push("--".to_string());
    commit_args.extend(paths);
    git::output_in_args(root, &commit_args)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_legacy_block_removes_block_and_trailing_newline() {
        let text =
            "# Repo\n\n<!-- nit:workspace:start -->\nold\n<!-- nit:workspace:end -->\nrest\n";
        let stripped = strip_legacy_block(text).expect("block found");
        assert_eq!(stripped, "# Repo\n\nrest\n");
    }

    #[test]
    fn strip_legacy_block_requires_both_markers() {
        assert!(strip_legacy_block("<!-- nit:workspace:start -->\ndangling\n").is_none());
        assert!(strip_legacy_block("no markers at all\n").is_none());
    }
}
