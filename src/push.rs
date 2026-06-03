use std::env;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::git;
use crate::metadata::Roster;
use crate::workspace;

pub fn push(resume: bool) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = workspace::find_nit_workspace(&cwd)
        .context("not in a Nit workspace; run `nit init` first")?;
    let roster = Roster::read(&root)?;

    if resume {
        println!("resuming ordered push");
    }

    for member in &roster.members {
        let member_root = root.join(&member.path);
        if !git::is_git_repo_root(&member_root) {
            bail!(
                "member {} at {} is not a Git repository",
                member.id,
                member.path
            );
        }
        push_repo(&member_root, &format!("member {}", member.id))?;
    }

    if git::is_git_repo_root(&root) {
        push_repo(&root, "workspace root")?;
    }

    println!("push complete");
    Ok(())
}

fn push_repo(repo: &Path, label: &str) -> Result<()> {
    let branch = git::output_in(repo, ["rev-parse", "--abbrev-ref", "HEAD"])
        .with_context(|| format!("read branch for {label}"))?
        .trim()
        .to_string();
    if branch == "HEAD" {
        bail!("{label} is detached; checkout a branch before pushing");
    }
    git::output_in(repo, ["remote", "get-url", "origin"])
        .with_context(|| format!("{label} has no origin remote"))?;
    let head = git::output_in(repo, ["rev-parse", "HEAD"])?
        .trim()
        .to_string();
    let remote_ref = format!("refs/heads/{branch}");
    let remote_head =
        git::output_in(repo, ["ls-remote", "origin", &remote_ref]).unwrap_or_default();
    if remote_head.split_whitespace().next() == Some(head.as_str()) {
        println!("{label} already pushed");
        return Ok(());
    }
    println!("pushing {label}");
    git::output_in(repo, ["push", "origin", "HEAD"])?;
    Ok(())
}
