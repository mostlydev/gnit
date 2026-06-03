use std::env;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};

use crate::git;
use crate::metadata::{Pin, PinMember, Roster};
use crate::workspace;

pub fn create(label: Option<String>, no_commit: bool) -> Result<()> {
    create_with_changes(label, Vec::new(), no_commit)
}

pub fn create_with_changes(
    label: Option<String>,
    changes: Vec<String>,
    no_commit: bool,
) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = workspace::find_nit_workspace(&cwd)
        .context("not in a Nit workspace; run `nit init` first")?;
    let roster = Roster::read(&root)?;

    if roster.members.is_empty() {
        bail!("cannot pin a workspace with no members");
    }
    ensure_clean(&root, "workspace root")?;

    let mut pin = Pin::new(generate_id(label.as_deref()));
    pin.label = label;
    pin.provenance.changes = changes;

    for member in roster.members {
        let member_root = root.join(&member.path);
        if !git::is_git_repo(&member_root) {
            bail!(
                "member {} at {} is not a Git repository",
                member.id,
                member.path
            );
        }
        ensure_clean(&member_root, &format!("member {}", member.id))?;
        let commit = git::output_in(&member_root, ["rev-parse", "HEAD"])
            .with_context(|| format!("read HEAD for member {}", member.id))?
            .trim()
            .to_string();
        let branch = git::output_in(&member_root, ["rev-parse", "--abbrev-ref", "HEAD"])
            .ok()
            .map(|branch| branch.trim().to_string())
            .filter(|branch| branch != "HEAD" && !branch.is_empty());

        pin.members.push(PinMember {
            id: member.id,
            path: member.path,
            commit,
            branch_hint: branch,
        });
    }

    let path = pin.write(&root)?;
    if !no_commit {
        workspace::commit_metadata(&root, &format!("Create Nit pin {}", pin.id))?;
    }

    println!("created Pin {}", pin.id);
    println!("  path: {}", path.display());
    Ok(())
}

fn ensure_clean(repo: &Path, name: &str) -> Result<()> {
    let status = git::output_in(repo, ["status", "--porcelain"])
        .with_context(|| format!("check status for {name}"))?;
    if !status.trim().is_empty() {
        bail!("{name} has uncommitted changes");
    }
    Ok(())
}

fn generate_id(label: Option<&str>) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_millis();
    match label.map(sanitize_label).filter(|label| !label.is_empty()) {
        Some(label) => format!("PIN-{millis}-{label}"),
        None => format!("PIN-{millis}"),
    }
}

fn sanitize_label(label: &str) -> String {
    label
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_id_includes_sanitized_label() {
        let id = generate_id(Some("Release 2026.06"));
        assert!(id.starts_with("PIN-"));
        assert!(id.ends_with("-release-2026-06"));
    }
}
