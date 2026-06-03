use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

pub fn output<const N: usize>(args: [&str; N]) -> Result<String> {
    let output = Command::new("git").args(args).output().context("run git")?;
    if !output.status.success() {
        bail!("git exited with {}", output.status);
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn output_in<const N: usize>(dir: &Path, args: [&str; N]) -> Result<String> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .with_context(|| format!("run git in {}", dir.display()))?;
    if !output.status.success() {
        bail!("git exited with {}", output.status);
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
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
