use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::git;
use crate::metadata::{Pin, Roster, PINS_DIR};
use crate::workspace;

pub fn push(resume: bool) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = workspace::find_nit_workspace(&cwd)
        .context("not in a Nit workspace; run `nit init` first")?;
    let roster = Roster::read(&root)?;

    if resume {
        println!("resuming ordered push from remote state");
    }

    let mut targets = Vec::new();
    for member in &roster.members {
        let member_root = root.join(&member.path);
        if !git::is_git_repo_root(&member_root) {
            targets.push(PushTarget::failed(
                PushTargetKind::Member,
                format!("member {}", member.id),
                member_root,
                format!("{} is not a Git repository", member.path),
            ));
            continue;
        }
        targets.push(PushTarget::new(
            PushTargetKind::Member,
            format!("member {}", member.id),
            member_root,
        ));
    }

    if git::is_git_repo_root(&root) {
        targets.push(PushTarget::new(
            PushTargetKind::Root,
            "workspace root".to_string(),
            root.clone(),
        ));
    }

    preflight_targets(&mut targets);

    if targets.iter().any(|target| target.result.is_failed()) {
        hold_pending_targets(&mut targets);
        print_report(&targets);
        bail!("push preflight failed; no repos were pushed");
    }

    push_members_in_order(&mut targets);

    let members_complete = targets
        .iter()
        .filter(|target| target.kind == PushTargetKind::Member)
        .all(|target| target.result.is_landed());

    if members_complete {
        if let Some(reason) = find_unreachable_pin_member(&root, &roster)? {
            hold_root(&mut targets, reason);
        } else {
            push_root(&mut targets);
        }
    } else {
        hold_root(
            &mut targets,
            "members incomplete; root not published".to_string(),
        );
    }

    print_report(&targets);

    if targets.iter().all(|target| target.result.is_landed()) {
        println!("push complete");
        Ok(())
    } else {
        bail!("push incomplete; resolve failures and run `nit push --resume`")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PushTargetKind {
    Member,
    Root,
}

#[derive(Debug)]
struct PushTarget {
    kind: PushTargetKind,
    label: String,
    repo: PathBuf,
    result: PushResult,
}

impl PushTarget {
    fn new(kind: PushTargetKind, label: String, repo: PathBuf) -> Self {
        Self {
            kind,
            label,
            repo,
            result: PushResult::Pending,
        }
    }

    fn failed(kind: PushTargetKind, label: String, repo: PathBuf, reason: String) -> Self {
        Self {
            kind,
            label,
            repo,
            result: PushResult::Failed(reason),
        }
    }
}

#[derive(Debug)]
enum PushResult {
    // Internal preflight state. It must be converted to Pushed,
    // NotAttempted, or HeldBack before the final report.
    Pending,
    AlreadyLanded,
    Pushed,
    Failed(String),
    NotAttempted,
    HeldBack(String),
}

impl PushResult {
    fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    fn is_landed(&self) -> bool {
        matches!(self, Self::AlreadyLanded | Self::Pushed)
    }

    fn report(&self) -> String {
        match self {
            Self::Pending => "not attempted".to_string(),
            Self::AlreadyLanded => "already landed".to_string(),
            Self::Pushed => "pushed".to_string(),
            Self::Failed(reason) => format!("failed: {reason}"),
            Self::NotAttempted => "not attempted".to_string(),
            Self::HeldBack(reason) => format!("held back: {reason}"),
        }
    }
}

fn preflight_targets(targets: &mut [PushTarget]) {
    for target in targets {
        if target.result.is_failed() {
            continue;
        }
        target.result = match inspect_repo(&target.repo, &target.label) {
            Ok(PushPlan::AlreadyLanded) => PushResult::AlreadyLanded,
            Ok(PushPlan::NeedsPush) => PushResult::Pending,
            Err(error) => PushResult::Failed(error.to_string()),
        };
    }
}

fn push_members_in_order(targets: &mut [PushTarget]) {
    let mut stopped = false;
    for target in targets
        .iter_mut()
        .filter(|target| target.kind == PushTargetKind::Member)
    {
        if target.result.is_landed() {
            continue;
        }
        if stopped {
            target.result = PushResult::NotAttempted;
            continue;
        }
        if !matches!(target.result, PushResult::Pending) {
            stopped = true;
            continue;
        }
        target.result = match push_repo(&target.repo) {
            Ok(()) => PushResult::Pushed,
            Err(error) => {
                stopped = true;
                PushResult::Failed(error)
            }
        };
    }
}

fn push_root(targets: &mut [PushTarget]) {
    let Some(root) = targets
        .iter_mut()
        .find(|target| target.kind == PushTargetKind::Root)
    else {
        return;
    };

    if root.result.is_landed() {
        return;
    }
    root.result = match &root.result {
        PushResult::Pending => match push_repo(&root.repo) {
            Ok(()) => PushResult::Pushed,
            Err(error) => PushResult::Failed(error),
        },
        PushResult::Failed(reason) => PushResult::Failed(reason.clone()),
        _ => PushResult::HeldBack("root not publishable".to_string()),
    };
}

fn hold_pending_targets(targets: &mut [PushTarget]) {
    for target in targets {
        if matches!(target.result, PushResult::Pending) {
            target.result = match target.kind {
                PushTargetKind::Member => PushResult::NotAttempted,
                PushTargetKind::Root => {
                    PushResult::HeldBack("members incomplete; root not published".to_string())
                }
            };
        }
    }
}

fn hold_root(targets: &mut [PushTarget], reason: String) {
    if let Some(root) = targets
        .iter_mut()
        .find(|target| target.kind == PushTargetKind::Root)
    {
        if !root.result.is_landed() && !root.result.is_failed() {
            root.result = PushResult::HeldBack(reason);
        }
    }
}

fn print_report(targets: &[PushTarget]) {
    println!("Push report:");
    for target in targets {
        println!("  {:<18} {}", target.label, target.result.report());
    }
}

enum PushPlan {
    AlreadyLanded,
    NeedsPush,
}

fn inspect_repo(repo: &Path, label: &str) -> Result<PushPlan> {
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
    let remote_head = git::output_in(repo, ["ls-remote", "origin", &remote_ref])
        .with_context(|| format!("{label} cannot read origin/{branch}"))?;
    if remote_head.split_whitespace().next() == Some(head.as_str()) {
        return Ok(PushPlan::AlreadyLanded);
    }
    Ok(PushPlan::NeedsPush)
}

fn push_repo(repo: &Path) -> std::result::Result<(), String> {
    let output = Command::new("git")
        .current_dir(repo)
        .args(["push", "origin", "HEAD"])
        .output()
        .map_err(|error| format!("run git push: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(summarize_git_failure(&output.stderr))
    }
}

fn summarize_git_failure(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let lower = text.to_lowercase();
    if lower.contains("non-fast-forward")
        || lower.contains("fetch first")
        || lower.contains("rejected")
    {
        "rejected (non-fast-forward)".to_string()
    } else {
        text.lines()
            .rev()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("git push failed")
            .to_string()
    }
}

fn find_unreachable_pin_member(root: &Path, roster: &Roster) -> Result<Option<String>> {
    let paths = git::output_in(root, ["ls-tree", "-r", "--name-only", "HEAD", PINS_DIR])
        .context("list committed pins")?;

    for path in paths.lines().filter(|line| !line.trim().is_empty()) {
        if !path.ends_with(".yaml") {
            continue;
        }
        let spec = format!("HEAD:{path}");
        let text = git::output_in_args(root, ["show", spec.as_str()])
            .with_context(|| format!("read committed pin {path}"))?;
        let pin: Pin =
            serde_yaml::from_str(&text).with_context(|| format!("parse committed pin {path}"))?;
        for member in &pin.members {
            if !roster.contains_id(&member.id) {
                continue;
            }
            let repo = root.join(&member.path);
            if !git::is_git_repo_root(&repo) {
                continue;
            }
            match pin_commit_reachability(&repo, &member.commit)? {
                PinCommitReachability::Reachable => {}
                PinCommitReachability::Unreachable => {
                    let label = pin.label.as_deref().unwrap_or(&pin.id);
                    return Ok(Some(format!(
                        "pin {label} references member {} commit {} not reachable from local HEAD or origin",
                        member.id,
                        short_commit(&member.commit)
                    )));
                }
                PinCommitReachability::Unknown(reason) => {
                    let label = pin.label.as_deref().unwrap_or(&pin.id);
                    return Ok(Some(format!(
                        "pin {label} references member {} commit {} but Nit could not verify it: {reason}",
                        member.id,
                        short_commit(&member.commit)
                    )));
                }
            }
        }
    }

    Ok(None)
}

enum PinCommitReachability {
    Reachable,
    Unreachable,
    Unknown(String),
}

enum AncestorCheck {
    Ancestor,
    NotAncestor,
    Unknown(String),
}

fn pin_commit_reachability(repo: &Path, commit: &str) -> Result<PinCommitReachability> {
    let head_check = is_ancestor(repo, commit)?;
    if matches!(head_check, AncestorCheck::Ancestor) {
        return Ok(PinCommitReachability::Reachable);
    }

    match origin_refs_contain(repo, commit)? {
        RefContainment::Contains => Ok(PinCommitReachability::Reachable),
        RefContainment::DoesNotContain => match head_check {
            AncestorCheck::Ancestor => unreachable!(),
            AncestorCheck::NotAncestor => Ok(PinCommitReachability::Unreachable),
            AncestorCheck::Unknown(reason) => Ok(PinCommitReachability::Unknown(reason)),
        },
        RefContainment::Unknown(reason) => Ok(PinCommitReachability::Unknown(reason)),
    }
}

fn is_ancestor(repo: &Path, commit: &str) -> Result<AncestorCheck> {
    let output = Command::new("git")
        .current_dir(repo)
        .args(["merge-base", "--is-ancestor", commit, "HEAD"])
        .output()
        .with_context(|| format!("verify pin reachability in {}", repo.display()))?;
    match output.status.code() {
        Some(0) => Ok(AncestorCheck::Ancestor),
        Some(1) => Ok(AncestorCheck::NotAncestor),
        _ => Ok(AncestorCheck::Unknown(summarize_git_failure(
            &output.stderr,
        ))),
    }
}

enum RefContainment {
    Contains,
    DoesNotContain,
    Unknown(String),
}

fn origin_refs_contain(repo: &Path, commit: &str) -> Result<RefContainment> {
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
            Ok(RefContainment::Contains)
        } else {
            Ok(RefContainment::DoesNotContain)
        }
    } else {
        Ok(RefContainment::Unknown(summarize_git_failure(
            &output.stderr,
        )))
    }
}

fn short_commit(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}
