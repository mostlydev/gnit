use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::checkout;
use crate::git;
use crate::metadata::Roster;
use crate::workspace;

pub fn clone_workspace(url: String, path: Option<PathBuf>, pin: Option<String>) -> Result<()> {
    let cwd = env::current_dir()?;
    let target = path.unwrap_or_else(|| default_clone_path(&url));
    if target.exists() {
        bail!("target {} already exists", target.display());
    }

    git::output_in_args(
        &cwd,
        [
            "clone",
            url.as_str(),
            target.to_str().context("non-utf8 path")?,
        ],
    )?;
    let root = if target.is_absolute() {
        target
    } else {
        cwd.join(target)
    };
    let roster = Roster::read(&root)?;

    if let Some(pin) = pin {
        checkout::checkout_in(root.clone(), pin, false)?;
    } else {
        hydrate_members(&root, &roster)?;
    }
    workspace::repair_required_excludes(&root, &roster)?;
    println!("cloned Nit workspace {}", root.display());
    Ok(())
}

pub fn hydrate_members(root: &Path, roster: &Roster) -> Result<()> {
    for member in &roster.members {
        let member_root = root.join(&member.path);
        if member_root.exists() {
            continue;
        }
        let remote = member
            .remote
            .as_ref()
            .with_context(|| format!("member {} is missing and has no remote", member.id))?;
        if let Some(parent) = member_root.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        git::output_in_args(root, ["clone", remote.as_str(), member.path.as_str()])?;
        println!("cloned {} from {}", member.id, remote);
    }
    Ok(())
}

fn default_clone_path(url: &str) -> PathBuf {
    let name = url
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or("workspace")
        .trim_end_matches(".git");
    PathBuf::from(if name.is_empty() { "workspace" } else { name })
}
