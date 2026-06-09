use std::env;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::git;
use crate::metadata::{Pin, Roster, PINS_DIR, ROSTER_PATH};
use crate::trailers;

pub fn check(base: String, head: String) -> Result<()> {
    let cwd = env::current_dir()?;
    let repo = git::root(&cwd).context("not in a Git repository")?;
    let workspace_root = repo.join(ROSTER_PATH).exists();
    let commits = commits_in_range(&repo, &base, &head)?;
    let trailer_report = trailer_report(&repo, &commits, workspace_root)?;
    let pin_report = if workspace_root {
        Some(validate_pin_reachability(&repo, &head)?)
    } else {
        None
    };

    println!("CI check:");
    println!("  range: {base}..{head}");
    if trailer_report.failures.is_empty() {
        println!("  commit trailers: ok ({} checked)", trailer_report.checked);
    } else {
        println!(
            "  commit trailers: failed ({} of {} missing or malformed)",
            trailer_report.failures.len(),
            trailer_report.checked
        );
        for failure in &trailer_report.failures {
            println!("    {} {}", short_commit(&failure.sha), failure.subject);
        }
    }

    if let Some(report) = &pin_report {
        if report.failures.is_empty() {
            println!(
                "  pin reachability: ok ({} member pins checked)",
                report.checked
            );
        } else {
            println!(
                "  pin reachability: failed ({} member pins checked)",
                report.checked
            );
            for failure in &report.failures {
                println!("    {failure}");
            }
        }
    } else {
        println!("  pin reachability: skipped (not a Gnit workspace root)");
    }

    let pin_failed = pin_report
        .as_ref()
        .is_some_and(|report| !report.failures.is_empty());
    if !trailer_report.failures.is_empty() || pin_failed {
        bail!("ci-check failed");
    }
    Ok(())
}

#[derive(Debug)]
struct Commit {
    sha: String,
    subject: String,
}

#[derive(Debug)]
struct TrailerFailure {
    sha: String,
    subject: String,
}

#[derive(Debug)]
struct TrailerReport {
    checked: usize,
    failures: Vec<TrailerFailure>,
}

#[derive(Debug)]
struct PinReport {
    checked: usize,
    failures: Vec<String>,
}

fn commits_in_range(repo: &Path, base: &str, head: &str) -> Result<Vec<Commit>> {
    let range = format!("{base}..{head}");
    let output = git::output_in_args(repo, ["rev-list", "--reverse", range.as_str()])
        .with_context(|| format!("list commits in {range}"))?;
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|sha| {
            let subject = git::output_in_args(repo, ["show", "-s", "--format=%s", sha])
                .with_context(|| format!("read commit subject for {}", short_commit(sha)))?
                .trim()
                .to_string();
            Ok(Commit {
                sha: sha.to_string(),
                subject,
            })
        })
        .collect()
}

fn trailer_report(repo: &Path, commits: &[Commit], workspace_root: bool) -> Result<TrailerReport> {
    let mut report = TrailerReport {
        checked: 0,
        failures: Vec::new(),
    };
    for commit in commits {
        if !commit_requires_trailer(repo, &commit.sha, workspace_root)? {
            continue;
        }
        report.checked += 1;
        let body = git::output_in_args(repo, ["show", "-s", "--format=%B", commit.sha.as_str()])
            .with_context(|| format!("read commit message for {}", short_commit(&commit.sha)))?;
        if trailers::change_id(&body).is_none() {
            report.failures.push(TrailerFailure {
                sha: commit.sha.clone(),
                subject: commit.subject.clone(),
            });
        }
    }
    Ok(report)
}

fn commit_requires_trailer(repo: &Path, sha: &str, workspace_root: bool) -> Result<bool> {
    if !workspace_root {
        return Ok(true);
    }
    let paths = changed_paths(repo, sha)?;
    if paths.is_empty() {
        return Ok(true);
    }
    Ok(!paths.iter().all(|path| is_workspace_metadata_path(path)))
}

fn changed_paths(repo: &Path, sha: &str) -> Result<Vec<String>> {
    let output = git::output_in_args(
        repo,
        [
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "-r",
            "--root",
            sha,
        ],
    )
    .with_context(|| format!("list paths changed by {}", short_commit(sha)))?;
    Ok(output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn is_workspace_metadata_path(path: &str) -> bool {
    path.starts_with(".gnit/") || matches!(path, "AGENTS.md" | "CLAUDE.md")
}

fn validate_pin_reachability(root: &Path, head: &str) -> Result<PinReport> {
    let roster = read_roster_at(root, head)?;
    let mut report = PinReport {
        checked: 0,
        failures: Vec::new(),
    };
    let pins = pins_at(root, head)?;

    for path in pins {
        let pin = read_pin_at(root, head, &path)?;
        for member_pin in &pin.members {
            let Some(roster_member) = roster
                .members
                .iter()
                .find(|member| member.id == member_pin.id)
            else {
                continue;
            };
            report.checked += 1;
            let repo = root.join(&roster_member.path);
            if !git::is_git_repo_root(&repo) {
                report.failures.push(format!(
                    "pin {} references member {} but {} is not a Git repository",
                    pin_label(&pin),
                    member_pin.id,
                    roster_member.path
                ));
                continue;
            }
            if let Err(reason) = fetch_origin(&repo) {
                report.failures.push(format!(
                    "pin {} references member {} commit {} but origin could not be fetched: {reason}",
                    pin_label(&pin),
                    member_pin.id,
                    short_commit(&member_pin.commit)
                ));
                continue;
            }
            match origin_contains(&repo, &member_pin.commit)? {
                OriginReachability::Reachable => {}
                OriginReachability::Unreachable => report.failures.push(format!(
                    "pin {} references member {} commit {} not reachable from origin",
                    pin_label(&pin),
                    member_pin.id,
                    short_commit(&member_pin.commit)
                )),
                OriginReachability::Unknown(reason) => report.failures.push(format!(
                    "pin {} references member {} commit {} but origin reachability could not be verified: {reason}",
                    pin_label(&pin),
                    member_pin.id,
                    short_commit(&member_pin.commit)
                )),
            }
        }
    }

    Ok(report)
}

fn fetch_origin(repo: &Path) -> std::result::Result<(), String> {
    let output = Command::new("git")
        .current_dir(repo)
        .args(["fetch", "--quiet", "origin"])
        .output()
        .map_err(|error| format!("run git fetch: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(summarize_git_failure(&output.stderr))
    }
}

fn read_roster_at(root: &Path, head: &str) -> Result<Roster> {
    let spec = format!("{head}:{ROSTER_PATH}");
    let text = git::output_in_args(root, ["show", spec.as_str()])
        .with_context(|| format!("read roster at {head}"))?;
    serde_yaml::from_str(&text).with_context(|| format!("parse roster at {head}"))
}

fn pins_at(root: &Path, head: &str) -> Result<Vec<String>> {
    let output = git::output_in_args(root, ["ls-tree", "-r", "--name-only", head, PINS_DIR])
        .with_context(|| format!("list pins at {head}"))?;
    Ok(output
        .lines()
        .map(str::trim)
        .filter(|line| line.ends_with(".yaml"))
        .map(ToOwned::to_owned)
        .collect())
}

fn read_pin_at(root: &Path, head: &str, path: &str) -> Result<Pin> {
    let spec = format!("{head}:{path}");
    let text = git::output_in_args(root, ["show", spec.as_str()])
        .with_context(|| format!("read pin {path} at {head}"))?;
    serde_yaml::from_str(&text).with_context(|| format!("parse pin {path} at {head}"))
}

enum OriginReachability {
    Reachable,
    Unreachable,
    Unknown(String),
}

fn origin_contains(repo: &Path, commit: &str) -> Result<OriginReachability> {
    let output = Command::new("git")
        .current_dir(repo)
        .args([
            "for-each-ref",
            "--format=%(refname)",
            "--count=1",
            "--contains",
            commit,
            "refs/remotes/origin",
        ])
        .output()
        .with_context(|| format!("verify origin reachability in {}", repo.display()))?;

    if output.status.success() {
        if output.stdout.iter().any(|byte| !byte.is_ascii_whitespace()) {
            Ok(OriginReachability::Reachable)
        } else {
            Ok(OriginReachability::Unreachable)
        }
    } else {
        Ok(OriginReachability::Unknown(summarize_git_failure(
            &output.stderr,
        )))
    }
}

fn summarize_git_failure(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("git failed")
        .to_string()
}

fn pin_label(pin: &Pin) -> &str {
    pin.label.as_deref().unwrap_or(&pin.id)
}

fn short_commit(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}
