use std::env;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::git;
use crate::metadata::{Pin, Roster, PINS_DIR};
use crate::workspace;

pub fn checkout(spec: String, exact: bool) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = workspace::find_nit_workspace(&cwd)
        .context("not in a Nit workspace; run `nit init` first")?;
    let roster = Roster::read(&root)?;
    let pin = load_pin(&root, &spec)?;

    for pinned in &pin.members {
        let Some(member) = roster.members.iter().find(|member| member.id == pinned.id) else {
            bail!("pin references unknown member {}", pinned.id);
        };
        let member_root = root.join(&member.path);
        if !member_root.exists() {
            let remote = member
                .remote
                .as_ref()
                .with_context(|| format!("member {} is missing and has no remote", member.id))?;
            if let Some(parent) = member_root.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create {}", parent.display()))?;
            }
            git::output_in_args(&root, ["clone", remote.as_str(), member.path.as_str()])?;
            println!("cloned {} from {}", member.id, remote);
        }
        if !git::is_git_repo_root(&member_root) {
            bail!(
                "member {} at {} is not a Git repository",
                member.id,
                member.path
            );
        }
        ensure_commit_available(&member_root, &pinned.commit)?;
        ensure_clean(&member_root, &member.id, exact)?;
        if exact {
            git::output_in(&member_root, ["reset", "--hard", &pinned.commit])?;
            git::output_in(&member_root, ["clean", "-fd"])?;
        }
        git::output_in(&member_root, ["checkout", "--detach", &pinned.commit])?;
        println!("checked out {} {}", member.id, short(&pinned.commit));
    }

    workspace::repair_required_excludes(&root, &roster)?;
    println!("checked out Pin {}", pin.id);
    Ok(())
}

fn load_pin(root: &Path, spec: &str) -> Result<Pin> {
    let direct = Pin::path(root, spec);
    if direct.exists() {
        return Pin::read(root, spec);
    }

    let pins_dir = root.join(PINS_DIR);
    let mut matches = Vec::new();
    for entry in fs::read_dir(&pins_dir).with_context(|| format!("read {}", pins_dir.display()))? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("yaml") {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .context("pin filename is not utf-8")?;
        let pin = Pin::read(root, id)?;
        if pin.id == spec || pin.label.as_deref() == Some(spec) {
            matches.push(pin);
        }
    }

    match matches.len() {
        0 => bail!("pin {spec} not found"),
        1 => Ok(matches.remove(0)),
        _ => bail!("pin label {spec} is ambiguous"),
    }
}

fn ensure_commit_available(repo: &Path, commit: &str) -> Result<()> {
    if git::status_in_args(repo, ["cat-file", "-e", &format!("{commit}^{{commit}}")])? {
        return Ok(());
    }
    git::output_in(repo, ["fetch", "--all", "--tags"])?;
    if git::status_in_args(repo, ["cat-file", "-e", &format!("{commit}^{{commit}}")])? {
        return Ok(());
    }
    bail!("commit {} is not available in {}", commit, repo.display())
}

fn ensure_clean(repo: &Path, id: &str, exact: bool) -> Result<()> {
    let status = git::output_in(repo, ["status", "--porcelain"])?;
    if !status.trim().is_empty() && !exact {
        bail!("member {id} has uncommitted changes; use --exact to reset it");
    }
    Ok(())
}

fn short(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}
