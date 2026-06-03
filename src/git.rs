use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

pub fn output<const N: usize>(args: [&str; N]) -> Result<String> {
    output_args(args)
}

pub fn output_args<I, S>(args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("git").args(args).output().context("run git")?;
    if !output.status.success() {
        bail!(
            "git exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn output_in<const N: usize>(dir: &Path, args: [&str; N]) -> Result<String> {
    output_in_args(dir, args)
}

pub fn output_in_args<I, S>(dir: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .with_context(|| format!("run git in {}", dir.display()))?;
    if !output.status.success() {
        bail!(
            "git exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn status_in_args<I, S>(dir: &Path, args: I) -> Result<bool>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .with_context(|| format!("run git in {}", dir.display()))?;
    Ok(status.success())
}

pub fn is_git_repo(dir: &Path) -> bool {
    output_in(dir, ["rev-parse", "--is-inside-work-tree"])
        .map(|out| out.trim() == "true")
        .unwrap_or(false)
}

pub fn root(dir: &Path) -> Option<PathBuf> {
    output_in(dir, ["rev-parse", "--show-toplevel"])
        .ok()
        .map(|out| PathBuf::from(out.trim()))
}

/// True only when `dir` is the ROOT of a Git repository, not merely a path
/// inside one. `is_git_repo` (rev-parse --is-inside-work-tree) returns true for
/// any subdirectory of a repo, so adopting a plain subdir would wrongly succeed;
/// this compares the canonical toplevel to the canonical path.
pub fn is_git_repo_root(dir: &Path) -> bool {
    let Some(top) = root(dir) else {
        return false;
    };
    match (top.canonicalize(), dir.canonicalize()) {
        (Ok(top), Ok(dir)) => top == dir,
        _ => false,
    }
}
