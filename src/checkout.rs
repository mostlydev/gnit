use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::git;
use crate::metadata::{Pin, Roster};
use crate::workspace;

pub fn checkout(spec: String, exact: bool) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = workspace::find_nit_workspace(&cwd)
        .context("not in a Nit workspace; run `nit init` first")?;
    checkout_in(root, spec, exact)
}

pub fn checkout_in(root: PathBuf, spec: String, exact: bool) -> Result<()> {
    let roster = Roster::read(&root)?;
    let pin = Pin::load(&root, &spec)?;

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
            git::output_in(&member_root, ["reset", "--hard"])?;
            git::output_in(&member_root, ["clean", "-fd"])?;
        }
        match materialize_commit(&member_root, &pinned.commit, pinned.branch_hint.as_deref())? {
            CheckoutOutcome::Branch(branch) => {
                println!(
                    "checked out {} {} on {}",
                    member.id,
                    short(&pinned.commit),
                    branch
                );
            }
            CheckoutOutcome::Detached(reason) => {
                eprintln!(
                    "warning: member {} is detached at {} ({reason})",
                    member.id,
                    short(&pinned.commit)
                );
                println!("checked out {} {}", member.id, short(&pinned.commit));
            }
        }
    }

    workspace::repair_required_excludes(&root, &roster)?;
    println!("checked out Pin {}", pin.id);
    Ok(())
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

enum CheckoutOutcome {
    Branch(String),
    Detached(&'static str),
}

#[derive(Clone)]
struct GitRef {
    name: String,
    is_remote: bool,
}

fn materialize_commit(
    repo: &Path,
    commit: &str,
    branch_hint: Option<&str>,
) -> Result<CheckoutOutcome> {
    let refs = refs_at_commit(repo, commit)?;
    if let Some(branch) = find_hinted_branch(&refs, branch_hint, false) {
        git::output_in_args(repo, ["checkout", branch.name.as_str()])?;
        return Ok(CheckoutOutcome::Branch(branch.name));
    }

    if let Some(remote) = find_hinted_branch(&refs, branch_hint, true) {
        return materialize_remote_branch(repo, commit, &remote);
    }

    if let Some(branch) = select_branch(&refs, false) {
        git::output_in_args(repo, ["checkout", branch.name.as_str()])?;
        return Ok(CheckoutOutcome::Branch(branch.name));
    }

    if let Some(remote) = select_branch(&refs, true) {
        return materialize_remote_branch(repo, commit, &remote);
    }

    git::output_in(repo, ["checkout", "--detach", commit])?;
    Ok(CheckoutOutcome::Detached("no branch points to this commit"))
}

fn materialize_remote_branch(
    repo: &Path,
    commit: &str,
    remote: &GitRef,
) -> Result<CheckoutOutcome> {
    let Some(local_name) = local_name_for_remote(&remote.name) else {
        git::output_in(repo, ["checkout", "--detach", commit])?;
        return Ok(CheckoutOutcome::Detached(
            "remote branch name is not localizable",
        ));
    };

    if local_branch_exists(repo, &local_name)? {
        if can_fast_forward_branch(repo, &local_name, commit)? {
            git::output_in_args(repo, ["checkout", local_name.as_str()])?;
            git::output_in_args(repo, ["merge", "--ff-only", commit])?;
            return Ok(CheckoutOutcome::Branch(local_name));
        }
        git::output_in(repo, ["checkout", "--detach", commit])?;
        return Ok(CheckoutOutcome::Detached(
            "matching remote branch exists, but the local branch cannot fast-forward",
        ));
    }

    git::output_in_args(
        repo,
        [
            "branch",
            "--track",
            local_name.as_str(),
            remote.name.as_str(),
        ],
    )?;
    git::output_in_args(repo, ["checkout", local_name.as_str()])?;
    Ok(CheckoutOutcome::Branch(local_name))
}

fn refs_at_commit(repo: &Path, commit: &str) -> Result<Vec<GitRef>> {
    let refs = git::output_in(
        repo,
        [
            "for-each-ref",
            "--format=%(refname)%00%(refname:short)%00%(objectname)",
            "refs/heads",
            "refs/remotes",
        ],
    )?;
    Ok(refs
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\0');
            let full_name = fields.next()?;
            let name = fields.next()?;
            let object = fields.next()?;
            if object != commit || name.ends_with("/HEAD") {
                return None;
            }
            Some(GitRef {
                name: name.to_string(),
                is_remote: full_name.starts_with("refs/remotes/"),
            })
        })
        .collect())
}

fn find_hinted_branch(refs: &[GitRef], branch_hint: Option<&str>, remote: bool) -> Option<GitRef> {
    let hint = branch_hint?;
    refs.iter()
        .find(|branch| {
            branch.is_remote == remote
                && (branch.name == hint
                    || (branch.is_remote
                        && local_name_for_remote(&branch.name).as_deref() == Some(hint)))
        })
        .cloned()
}

fn select_branch(refs: &[GitRef], remote: bool) -> Option<GitRef> {
    refs.iter()
        .find(|branch| branch.is_remote == remote)
        .cloned()
}

fn local_name_for_remote(remote: &str) -> Option<String> {
    remote.split_once('/').map(|(_, branch)| branch.to_string())
}

fn local_branch_exists(repo: &Path, branch: &str) -> Result<bool> {
    git::status_in_args(
        repo,
        [
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
}

fn can_fast_forward_branch(repo: &Path, branch: &str, commit: &str) -> Result<bool> {
    git::status_in_args(
        repo,
        [
            "merge-base",
            "--is-ancestor",
            &format!("refs/heads/{branch}"),
            commit,
        ],
    )
}

fn short(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}
