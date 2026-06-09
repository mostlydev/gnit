use std::env;

use anyhow::{Context, Result};

use crate::change;
use crate::git;
use crate::metadata::Pin;
use crate::workspace;

pub fn review(target: String) -> Result<()> {
    if target.starts_with("GCH-") {
        return change::diff(target);
    }

    let cwd = env::current_dir()?;
    let root = workspace::find_gnit_workspace(&cwd)
        .context("not in a Gnit workspace; run `gnit init` first")?;
    let pin = Pin::load(&root, &target)?;

    println!("Review Pin {}", pin.id);
    if let Some(label) = &pin.label {
        println!("Label: {label}");
    }
    if !pin.provenance.changes.is_empty() {
        println!("Changes: {}", pin.provenance.changes.join(", "));
    }

    for member in &pin.members {
        println!("\n== {} ({}) ==", member.id, member.path);
        println!("commit {}", member.commit);
        if let Some(branch) = &member.branch_hint {
            println!("branch hint {branch}");
        }

        let member_root = root.join(&member.path);
        if !git::is_git_repo_root(&member_root) {
            println!("not available locally");
            continue;
        }
        if !git::status_in_args(
            &member_root,
            ["cat-file", "-e", &format!("{}^{{commit}}", member.commit)],
        )? {
            println!(
                "commit not available locally; run `gnit checkout {}` or `git -C {} fetch origin`",
                pin.id,
                member_root.display()
            );
            continue;
        }
        let summary = git::output_in_args(
            &member_root,
            ["show", "--stat", "--oneline", member.commit.as_str()],
        )?;
        print!("{summary}");
    }
    Ok(())
}
