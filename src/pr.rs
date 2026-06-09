use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::Value;

use crate::cli::{PrOpenArgs, PrStatusArgs};
use crate::git;
use crate::metadata::{Pin, Roster};
use crate::workspace;

const TRAILER: &str = "Gnit-Change-Id";
const MARKER_START: &str = "<!-- gnit-pr-sync:start -->";
const MARKER_END: &str = "<!-- gnit-pr-sync:end -->";
const MIN_GH_VERSION: (u64, u64, u64) = (2, 0, 0);

pub fn status(args: PrStatusArgs) -> Result<()> {
    let root = current_root()?;
    let options = ResolveOptions {
        change: args.change,
        pin: args.pin,
        base: args.base,
        branch: None,
        refresh_base: false,
    };
    let projection = build_projection(&root, &options)?;
    print_status(&projection, false)?;
    Ok(())
}

pub fn open(args: PrOpenArgs) -> Result<()> {
    let root = current_root()?;
    let options = ResolveOptions {
        change: args.change,
        pin: args.pin,
        base: args.base,
        branch: args.branch,
        refresh_base: true,
    };
    let mut projection = build_projection(&root, &options)?;
    let title = args
        .title
        .clone()
        .unwrap_or_else(|| projection.title.clone());

    preflight_open(&projection)?;
    println!("Opening PRs for Change {}", projection.change_id);
    println!("Title: {title}");
    println!("Mode: {}", if args.ready { "ready" } else { "draft" });

    let mut failures = Vec::new();
    for participant in &mut projection.participants {
        if !participant.pr_capable {
            if !is_optional_anchor(participant) {
                continue;
            }
            println!(
                "  {:<24} skipped: {}",
                participant.id,
                participant
                    .local_problem
                    .as_deref()
                    .unwrap_or("not PR-capable")
            );
            continue;
        }
        match ensure_pr(participant, &projection.change_id, &title, !args.ready) {
            Ok(action) => {
                participant.pr_action = Some(action.clone());
                println!("  {:<24} {}", participant.id, action);
            }
            Err(error) => {
                failures.push(format!("{}: {error}", participant.id));
                println!("  {:<24} failed: {error}", participant.id);
            }
        }
    }

    if !failures.is_empty() {
        bail!(
            "pr open incomplete; resolve failures and rerun `gnit pr open{}`",
            selector_hint(&projection)
        );
    }

    let marker = marker_block(&projection);
    let mut update_failures = Vec::new();
    for participant in &projection.participants {
        if !participant.pr_capable {
            continue;
        }
        let Some(pr) = &participant.pr else {
            continue;
        };
        let body = replace_marker(pr.body.as_deref().unwrap_or_default(), &marker);
        if let Err(error) = gh_output(
            Some(&participant.root),
            [
                "pr",
                "edit",
                pr.number.to_string().as_str(),
                "-R",
                participant.repo_slug.as_deref().unwrap_or_default(),
                "--body",
                body.as_str(),
            ],
        ) {
            update_failures.push(format!("{}: {error}", participant.id));
        }
    }
    if !update_failures.is_empty() {
        bail!(
            "pr body update incomplete; resolve failures and rerun `gnit pr open{}`: {}",
            selector_hint(&projection),
            update_failures.join("; ")
        );
    }

    println!("PRs synchronized.");
    Ok(())
}

#[derive(Debug)]
struct ResolveOptions {
    change: Option<String>,
    pin: Option<String>,
    base: Option<String>,
    branch: Option<String>,
    refresh_base: bool,
}

#[derive(Debug, Clone)]
struct RepoInfo {
    id: String,
    path: String,
    root: PathBuf,
    is_root: bool,
}

#[derive(Debug)]
struct Projection {
    change_id: String,
    pin: Option<PinContext>,
    title: String,
    participants: Vec<Participant>,
}

#[derive(Debug)]
struct PinContext {
    id: String,
    label: Option<String>,
}

#[derive(Debug)]
struct Participant {
    id: String,
    root: PathBuf,
    repo_slug: Option<String>,
    path: String,
    branch: Option<String>,
    base: Option<String>,
    head: Option<String>,
    participant_commit: String,
    is_root: bool,
    metadata_anchor: bool,
    pr_capable: bool,
    local_problem: Option<String>,
    pr: Option<PullRequest>,
    pr_status: PrLookupStatus,
    pr_action: Option<String>,
}

#[derive(Debug)]
struct LocalRepoProjection {
    id: String,
    path: String,
    root: PathBuf,
    is_root: bool,
    branch: Option<String>,
    base: Option<String>,
    head: Option<String>,
    repo_slug: Option<String>,
    local_problem: Option<String>,
    changes: BTreeMap<String, Vec<LocalCommit>>,
    metadata_anchors: BTreeMap<String, LocalCommit>,
}

#[derive(Debug, Clone)]
struct LocalCommit {
    hash: String,
    subject: String,
}

#[derive(Debug, Clone)]
struct PullRequest {
    number: u64,
    state: String,
    url: String,
    body: Option<String>,
    checks: String,
}

#[derive(Debug)]
enum PrLookupStatus {
    Missing,
    Found,
    Offline(String),
    Ambiguous(String),
}

#[derive(Debug, Deserialize)]
struct GhPr {
    number: u64,
    #[serde(default)]
    state: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default, rename = "statusCheckRollup")]
    status_check_rollup: Value,
}

fn current_root() -> Result<PathBuf> {
    let cwd = env::current_dir()?;
    workspace::find_gnit_workspace(&cwd).context("not in a Gnit workspace; run `gnit init` first")
}

fn build_projection(root: &Path, options: &ResolveOptions) -> Result<Projection> {
    let pin = resolve_pin(root, options.pin.as_deref())?;
    let explicit_change = match (&options.change, &pin) {
        (Some(change), _) => Some(change.clone()),
        (None, Some(pin)) => Some(pin.change_id.clone()),
        (None, None) => None,
    };

    let roster = Roster::read(root)?;
    let repos = workspace_repos(root, &roster);
    let mut all_repos = Vec::new();
    for repo in repos {
        all_repos.push(project_repo(&repo, options)?);
    }

    let change_id = match explicit_change {
        Some(change) => change,
        None => infer_current_change(&all_repos)?,
    };

    let mut participants = Vec::new();
    let mut title = None;
    for repo in &all_repos {
        let change_commit = repo
            .changes
            .get(&change_id)
            .and_then(|commits| commits.last())
            .cloned();
        let metadata_anchor = repo.metadata_anchors.get(&change_id).cloned();
        if title.is_none() {
            if let Some(commit) = &change_commit {
                if !commit.subject.trim().is_empty() {
                    title = Some(commit.subject.clone());
                }
            }
        }
        let selected = match (change_commit, metadata_anchor) {
            (Some(_commit), Some(anchor)) if repo.is_root => Some((anchor, true)),
            (Some(commit), _) => Some((commit, false)),
            (None, Some(anchor)) if repo.is_root => Some((anchor, true)),
            _ => None,
        };
        let Some((commit, is_metadata_anchor)) = selected else {
            continue;
        };
        let local_problem = if repo.local_problem.is_none() && repo.repo_slug.is_none() {
            Some(format!(
                "{} has no GitHub repo identity; set origin to a GitHub remote",
                repo.id
            ))
        } else {
            repo.local_problem.clone()
        };
        let mut participant = Participant {
            id: repo.id.clone(),
            root: repo.root.clone(),
            repo_slug: repo.repo_slug.clone(),
            path: repo.path.clone(),
            branch: repo.branch.clone(),
            base: repo.base.clone(),
            head: repo.head.clone(),
            participant_commit: commit.hash,
            is_root: repo.is_root,
            metadata_anchor: is_metadata_anchor,
            pr_capable: local_problem.is_none() && repo.repo_slug.is_some(),
            local_problem,
            pr: None,
            pr_status: PrLookupStatus::Missing,
            pr_action: None,
        };
        if participant.is_root && participant.metadata_anchor && participant.repo_slug.is_none() {
            participant.pr_capable = false;
            participant.local_problem =
                Some("control root has no GitHub origin; member PRs can still open".to_string());
        }
        participants.push(participant);
    }

    if participants.is_empty() {
        bail!(
            "change {change_id} has no commits on the current PR branch; run `gnit change show {change_id}` to inspect it"
        );
    }

    let mut projection = Projection {
        change_id,
        pin: pin.map(|pin| PinContext {
            id: pin.id,
            label: pin.label,
        }),
        title: title.unwrap_or_else(|| "Gnit workspace change".to_string()),
        participants,
    };

    lookup_prs(&mut projection, options.refresh_base)?;
    Ok(projection)
}

#[derive(Debug)]
struct ResolvedPin {
    id: String,
    label: Option<String>,
    change_id: String,
}

fn resolve_pin(root: &Path, pin: Option<&str>) -> Result<Option<ResolvedPin>> {
    let Some(spec) = pin else {
        return Ok(None);
    };
    let pin = Pin::load(root, spec)?;
    match pin.provenance.changes.as_slice() {
        [change] => Ok(Some(ResolvedPin {
            id: pin.id,
            label: pin.label,
            change_id: change.clone(),
        })),
        [] => bail!(
            "pin {spec} does not record a provenance Change; rerun with `gnit pr --change <id>`"
        ),
        changes => bail!(
            "pin {spec} records multiple Changes ({}); rerun with one explicit `gnit pr --change <id>`",
            changes.join(", ")
        ),
    }
}

fn workspace_repos(root: &Path, roster: &Roster) -> Vec<RepoInfo> {
    let mut repos = Vec::new();
    if git::is_git_repo(root) {
        repos.push(RepoInfo {
            id: "root".to_string(),
            path: ".".to_string(),
            root: root.to_path_buf(),
            is_root: true,
        });
    }
    for member in &roster.members {
        repos.push(RepoInfo {
            id: member.id.clone(),
            path: member.path.clone(),
            root: root.join(&member.path),
            is_root: false,
        });
    }
    repos
}

fn project_repo(repo: &RepoInfo, options: &ResolveOptions) -> Result<LocalRepoProjection> {
    let mut local_problem = None;
    if !git::is_git_repo_root(&repo.root) {
        local_problem = Some(format!(
            "{} is not available as a Git repository",
            repo.path
        ));
    }

    let branch = options
        .branch
        .clone()
        .or_else(|| current_branch(&repo.root).ok());
    let head = git::output_in(&repo.root, ["rev-parse", "HEAD"])
        .ok()
        .map(|head| head.trim().to_string());
    let base = options
        .base
        .clone()
        .or_else(|| default_base_branch(&repo.root).ok());
    let repo_slug = github_repo_slug(&repo.root).ok();

    let mut changes = BTreeMap::new();
    let mut metadata_anchors = BTreeMap::new();

    if local_problem.is_none() {
        if branch.is_none() {
            local_problem = Some(format!(
                "{} is detached; checkout a branch or rerun with `gnit pr open --branch <name>`",
                repo.id
            ));
        }
        if base.is_none() {
            local_problem = Some(format!(
                "{} cannot resolve its base branch; rerun with `gnit pr --base <branch>`",
                repo.id
            ));
        }
    }

    if local_problem.is_none() {
        if let Some(base) = &base {
            if options.refresh_base {
                let _ = git::output_in_args(&repo.root, ["fetch", "origin", base.as_str()]);
            }
            match commits_ahead_of_base(&repo.root, base) {
                Ok(commits) => {
                    for (change, commit) in commits {
                        changes.entry(change).or_insert_with(Vec::new).push(commit);
                    }
                    if repo.is_root {
                        metadata_anchors = pin_anchors_ahead_of_base(&repo.root, base)?;
                    }
                }
                Err(error) => {
                    local_problem = Some(format!(
                        "{} cannot compare HEAD to origin/{base}; run `git fetch origin {base}`",
                        repo.id
                    ));
                    if options.refresh_base {
                        local_problem = Some(format!(
                            "{} cannot compare HEAD to origin/{base}; run `git -C {} fetch origin {base}` ({error})",
                            repo.id,
                            repo.root.display()
                        ));
                    }
                }
            }
        }
    }

    Ok(LocalRepoProjection {
        id: repo.id.clone(),
        path: repo.path.clone(),
        root: repo.root.clone(),
        is_root: repo.is_root,
        branch,
        base,
        head,
        repo_slug,
        local_problem,
        changes,
        metadata_anchors,
    })
}

fn current_branch(repo: &Path) -> Result<String> {
    let branch = git::output_in(repo, ["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();
    if branch == "HEAD" || branch.is_empty() {
        bail!("detached HEAD");
    }
    Ok(branch)
}

fn default_base_branch(repo: &Path) -> Result<String> {
    if let Ok(value) = git::output_in(
        repo,
        ["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    ) {
        let value = value.trim();
        if let Some(branch) = value.strip_prefix("origin/") {
            if !branch.is_empty() {
                return Ok(branch.to_string());
            }
        }
    }
    let remote = git::output_in(repo, ["remote", "show", "-n", "origin"])?;
    for line in remote.lines() {
        let line = line.trim();
        if let Some(branch) = line.strip_prefix("HEAD branch:") {
            let branch = branch.trim();
            if !branch.is_empty() && branch != "(unknown)" {
                return Ok(branch.to_string());
            }
        }
    }
    bail!("origin default branch unavailable")
}

fn commits_ahead_of_base(repo: &Path, base: &str) -> Result<Vec<(String, LocalCommit)>> {
    let base_ref = format!("refs/remotes/origin/{base}");
    let merge_base = git::output_in_args(repo, ["merge-base", base_ref.as_str(), "HEAD"])?
        .trim()
        .to_string();
    let range = format!("{merge_base}..HEAD");
    let log = git::output_in_args(
        repo,
        ["log", "--reverse", "--format=%H%x1f%s%x1f%B%x1e", &range],
    )?;
    let mut commits = Vec::new();
    for record in log.split('\x1e') {
        let record = record.trim();
        if record.is_empty() {
            continue;
        }
        let mut fields = record.splitn(3, '\x1f');
        let Some(hash) = fields.next() else { continue };
        let Some(subject) = fields.next() else {
            continue;
        };
        let Some(body) = fields.next() else { continue };
        let Some(change) = trailer_value(body) else {
            continue;
        };
        commits.push((
            change,
            LocalCommit {
                hash: hash.to_string(),
                subject: subject.to_string(),
            },
        ));
    }
    Ok(commits)
}

fn pin_anchors_ahead_of_base(repo: &Path, base: &str) -> Result<BTreeMap<String, LocalCommit>> {
    let base_ref = format!("refs/remotes/origin/{base}");
    let merge_base = git::output_in_args(repo, ["merge-base", base_ref.as_str(), "HEAD"])?
        .trim()
        .to_string();
    let range = format!("{merge_base}..HEAD");
    let commits = git::output_in_args(repo, ["log", "--reverse", "--format=%H%x1f%s", &range])?;
    let mut anchors = BTreeMap::new();
    for line in commits.lines() {
        let Some((hash, subject)) = line.split_once('\x1f') else {
            continue;
        };
        let files = git::output_in_args(
            repo,
            ["diff-tree", "--no-commit-id", "--name-only", "-r", hash],
        )?;
        for file in files.lines() {
            if !file.starts_with(".gnit/pins/") || !file.ends_with(".yaml") {
                continue;
            }
            let spec = format!("{hash}:{file}");
            let Ok(text) = git::output_in_args(repo, ["show", spec.as_str()]) else {
                continue;
            };
            let Ok(pin) = serde_yaml::from_str::<Pin>(&text) else {
                continue;
            };
            for change in pin.provenance.changes {
                anchors.insert(
                    change,
                    LocalCommit {
                        hash: hash.to_string(),
                        subject: subject.to_string(),
                    },
                );
            }
        }
    }
    Ok(anchors)
}

fn infer_current_change(repos: &[LocalRepoProjection]) -> Result<String> {
    let mut ids = BTreeSet::new();
    for repo in repos {
        ids.extend(repo.changes.keys().cloned());
        ids.extend(repo.metadata_anchors.keys().cloned());
    }
    match ids.len() {
        0 => {
            bail!(
                "no current Gnit Change found on the PR branch; rerun with `gnit pr --change <id>`"
            )
        }
        1 => Ok(ids.into_iter().next().unwrap()),
        _ => {
            let choices = ids.into_iter().collect::<Vec<_>>().join(", ");
            bail!(
                "multiple Gnit Changes found on the PR branch ({choices}); rerun with `gnit pr --change <id>`"
            )
        }
    }
}

fn lookup_prs(projection: &mut Projection, strict: bool) -> Result<()> {
    for participant in &mut projection.participants {
        if !participant.pr_capable {
            continue;
        }
        match find_pr(participant, &projection.change_id) {
            Ok(Some(pr)) => {
                participant.pr = Some(pr);
                participant.pr_status = PrLookupStatus::Found;
            }
            Ok(None) => {
                participant.pr_status = PrLookupStatus::Missing;
            }
            Err(error) if strict => return Err(error),
            Err(error) => {
                let text = error.to_string();
                if text.contains("multiple PRs") {
                    participant.pr_status = PrLookupStatus::Ambiguous(text);
                } else {
                    participant.pr_status = PrLookupStatus::Offline(text);
                }
            }
        }
    }
    Ok(())
}

fn find_pr(participant: &Participant, change_id: &str) -> Result<Option<PullRequest>> {
    let repo = participant
        .repo_slug
        .as_deref()
        .context("missing GitHub repo identity")?;
    let marker_query = format!("\"{TRAILER}: {change_id}\"");
    let marker_matches = gh_pr_list(
        &participant.root,
        repo,
        &["--search", marker_query.as_str()],
    )?;
    let marker_matches = marker_matches
        .into_iter()
        .filter(|pr| {
            pr.body
                .as_deref()
                .map(|body| body.contains(MARKER_START) && body.contains(change_id))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    if marker_matches.len() > 1 {
        bail!(
            "multiple PRs in {repo} contain {TRAILER}: {change_id}; close or edit duplicates, then rerun `gnit pr`"
        );
    }
    if let Some(pr) = marker_matches.into_iter().next() {
        return Ok(Some(pr));
    }

    let Some(branch) = participant.branch.as_deref() else {
        return Ok(None);
    };
    let head_matches = gh_pr_list(&participant.root, repo, &["--head", branch])?;
    match head_matches.len() {
        0 => Ok(None),
        1 => Ok(head_matches.into_iter().next()),
        _ => bail!(
            "multiple PRs in {repo} use head branch {branch}; add the Gnit marker to one or close duplicates"
        ),
    }
}

fn gh_pr_list(repo_root: &Path, repo: &str, extra: &[&str]) -> Result<Vec<PullRequest>> {
    let mut args = vec![
        "pr",
        "list",
        "-R",
        repo,
        "--state",
        "all",
        "--json",
        "number,state,url,title,headRefName,body,statusCheckRollup",
    ];
    args.extend(extra.iter().copied());
    let text = gh_output(Some(repo_root), args)?;
    let raw: Vec<GhPr> = serde_json::from_str(&text).context("parse gh pr list JSON")?;
    Ok(raw
        .into_iter()
        .map(|pr| PullRequest {
            number: pr.number,
            state: normalize_state(&pr.state),
            url: pr.url,
            body: pr.body,
            checks: summarize_checks(&pr.status_check_rollup),
        })
        .collect())
}

fn preflight_open(projection: &Projection) -> Result<()> {
    ensure_gh_available()?;
    let mut blockers = Vec::new();
    for participant in &projection.participants {
        if !participant.pr_capable {
            if is_optional_anchor(participant) {
                continue;
            }
            blockers.push(format!(
                "{}: {}",
                participant.id,
                participant
                    .local_problem
                    .as_deref()
                    .unwrap_or("not PR-capable")
            ));
            continue;
        }
        if let Some(problem) = &participant.local_problem {
            blockers.push(format!("{}: {problem}", participant.id));
            continue;
        }
        let Some(branch) = participant.branch.as_deref() else {
            blockers.push(format!(
                "{}: detached; checkout a branch and run `gnit push`, or rerun with `gnit pr open --branch <name>`",
                participant.id
            ));
            continue;
        };
        let Some(base) = participant.base.as_deref() else {
            blockers.push(format!(
                "{}: base branch unknown; rerun with `gnit pr open --base <branch>`",
                participant.id
            ));
            continue;
        };
        if let Err(error) = verify_remote_branch(participant, branch) {
            blockers.push(format!("{}: {error}", participant.id));
        }
        if !git::status_in_args(
            &participant.root,
            [
                "cat-file",
                "-e",
                &format!("refs/remotes/origin/{base}^{{commit}}"),
            ],
        )? {
            blockers.push(format!(
                "{}: origin/{base} not available; run `git -C {} fetch origin {base}`",
                participant.id,
                participant.root.display()
            ));
        }
    }
    if !blockers.is_empty() {
        bail!(
            "pr open blocked before creating PRs:\n  {}",
            blockers.join("\n  ")
        );
    }
    Ok(())
}

fn ensure_gh_available() -> Result<()> {
    let version = gh_output(None, ["--version"])
        .context("GitHub CLI `gh` is required; install gh and run `gh auth login`")?;
    let parsed = parse_gh_version(&version).unwrap_or((0, 0, 0));
    if parsed < MIN_GH_VERSION {
        bail!("GitHub CLI is too old ({version}); upgrade gh, then rerun `gnit pr open`");
    }
    gh_output(None, ["auth", "status"])
        .context("GitHub auth unavailable; run `gh auth login`, then rerun `gnit pr open`")?;
    Ok(())
}

fn verify_remote_branch(participant: &Participant, branch: &str) -> Result<()> {
    let remote_ref = format!("refs/heads/{branch}");
    let remote = git::output_in_args(&participant.root, ["ls-remote", "origin", &remote_ref])
        .with_context(|| {
            format!(
                "cannot read origin/{branch}; run `gnit push` from {}",
                participant.root.display()
            )
        })?;
    let remote_head = remote.split_whitespace().next();
    let local_head = participant.head.as_deref().context("missing local HEAD")?;
    if remote_head != Some(local_head) {
        bail!(
            "origin/{branch} is not at local HEAD {}; run `gnit push` before `gnit pr open`",
            short(local_head)
        );
    }
    Ok(())
}

fn ensure_pr(
    participant: &mut Participant,
    change_id: &str,
    title: &str,
    draft: bool,
) -> Result<String> {
    if let Some(pr) = participant.pr.clone() {
        let had_marker = pr
            .body
            .as_deref()
            .map(|body| body.contains(MARKER_START) && body.contains(change_id))
            .unwrap_or(false);
        let marker = provisional_marker(change_id, participant);
        let body = replace_marker(pr.body.as_deref().unwrap_or_default(), &marker);
        gh_output(
            Some(&participant.root),
            [
                "pr",
                "edit",
                pr.number.to_string().as_str(),
                "-R",
                participant.repo_slug.as_deref().unwrap_or_default(),
                "--body",
                body.as_str(),
            ],
        )?;
        participant.pr = Some(PullRequest {
            body: Some(body),
            ..pr
        });
        let adopted = if had_marker {
            "already open"
        } else {
            "adopted"
        };
        return Ok(adopted.to_string());
    }

    let repo = participant
        .repo_slug
        .as_deref()
        .context("missing GitHub repo identity")?;
    let branch = participant.branch.as_deref().context("missing branch")?;
    let base = participant.base.as_deref().context("missing base")?;
    let marker = provisional_marker(change_id, participant);
    let mut args = vec![
        "pr",
        "create",
        "-R",
        repo,
        "--head",
        branch,
        "--base",
        base,
        "--title",
        title,
        "--body",
        marker.as_str(),
    ];
    if draft {
        args.push("--draft");
    }
    let output = gh_output(Some(&participant.root), args)?;
    let url = output.trim().to_string();
    participant.pr = Some(PullRequest {
        number: parse_pr_number(&url).unwrap_or(0),
        state: "open".to_string(),
        url,
        body: Some(marker),
        checks: "-".to_string(),
    });
    Ok("created".to_string())
}

fn print_status(projection: &Projection, strict: bool) -> Result<()> {
    println!("Workspace change {}", projection.change_id);
    if let Some(pin) = &projection.pin {
        println!("Pin: {}", pin.label.as_deref().unwrap_or(&pin.id));
    }
    println!(
        "repo                         branch              base        pr        state     checks"
    );
    for participant in &projection.participants {
        let branch = participant.branch.as_deref().unwrap_or("detached");
        let base = participant.base.as_deref().unwrap_or("-");
        let (pr, state, checks) = if !participant.pr_capable {
            (
                "unknown".to_string(),
                if is_optional_anchor(participant) {
                    "blocked".to_string()
                } else {
                    "offline".to_string()
                },
                participant
                    .local_problem
                    .clone()
                    .unwrap_or_else(|| "not PR-capable".to_string()),
            )
        } else {
            match &participant.pr_status {
                PrLookupStatus::Found => {
                    let pr = participant.pr.as_ref().unwrap();
                    (
                        if pr.number == 0 {
                            pr.url.clone()
                        } else {
                            format!("#{}", pr.number)
                        },
                        pr.state.clone(),
                        pr.checks.clone(),
                    )
                }
                PrLookupStatus::Missing => {
                    ("missing".to_string(), "-".to_string(), "-".to_string())
                }
                PrLookupStatus::Offline(reason) => (
                    "unknown".to_string(),
                    "offline".to_string(),
                    format!("unknown ({})", concise(reason)),
                ),
                PrLookupStatus::Ambiguous(reason) => (
                    "ambiguous".to_string(),
                    "blocked".to_string(),
                    concise(reason),
                ),
            }
        };
        println!(
            "{:<28} {:<19} {:<11} {:<9} {:<9} {}",
            status_label(participant),
            branch,
            base,
            pr,
            state,
            checks
        );
    }

    if projection.participants.is_empty() && strict {
        bail!("no PR participants for Change {}", projection.change_id);
    }
    Ok(())
}

fn status_label(participant: &Participant) -> String {
    if participant.is_root && participant.metadata_anchor {
        format!("{} (metadata)", participant.id)
    } else {
        participant.id.clone()
    }
}

fn is_optional_anchor(participant: &Participant) -> bool {
    participant.is_root && participant.metadata_anchor
}

fn marker_block(projection: &Projection) -> String {
    let mut text = String::new();
    text.push_str(MARKER_START);
    text.push('\n');
    text.push_str(&format!("{TRAILER}: {}\n", projection.change_id));
    if let Some(pin) = &projection.pin {
        text.push_str(&format!(
            "Gnit-Pin: {}\n",
            pin.label.as_deref().unwrap_or(&pin.id)
        ));
    }
    text.push('\n');
    if let Some(root) = projection.participants.iter().find(|p| p.is_root) {
        text.push_str("Workspace PR: ");
        text.push_str(&pr_ref(root));
        text.push('\n');
    }
    text.push_str("\nMember PRs:\n");
    for participant in projection.participants.iter().filter(|p| !p.is_root) {
        text.push_str("- ");
        text.push_str(&pr_ref(participant));
        text.push_str(&format!(" @ {}\n", short(&participant.participant_commit)));
    }
    text.push_str("\nCommits:\n");
    for participant in &projection.participants {
        text.push_str(&format!(
            "- {}: {} @ {}\n",
            participant.id,
            participant.path,
            short(&participant.participant_commit)
        ));
    }
    text.push_str("\nRecover:\n");
    text.push_str("  gnit pr\n");
    text.push_str(MARKER_END);
    text
}

fn provisional_marker(change_id: &str, participant: &Participant) -> String {
    format!(
        "{MARKER_START}\n{TRAILER}: {change_id}\n\nCommits:\n- {}: {} @ {}\n\nRecover:\n  gnit pr\n{MARKER_END}",
        participant.id,
        participant.path,
        short(&participant.participant_commit)
    )
}

fn pr_ref(participant: &Participant) -> String {
    let repo = participant.repo_slug.as_deref().unwrap_or(&participant.id);
    if let Some(pr) = &participant.pr {
        if pr.number == 0 {
            format!("{repo} {}", pr.url)
        } else {
            format!("{repo}#{}", pr.number)
        }
    } else {
        format!("{repo} missing")
    }
}

fn replace_marker(body: &str, marker: &str) -> String {
    if let Some(start) = body.find(MARKER_START) {
        if let Some(end_rel) = body[start..].find(MARKER_END) {
            let end = start + end_rel + MARKER_END.len();
            let mut next = String::new();
            next.push_str(body[..start].trim_end());
            if !next.is_empty() {
                next.push_str("\n\n");
            }
            next.push_str(marker);
            let suffix = body[end..].trim_start();
            if !suffix.is_empty() {
                next.push_str("\n\n");
                next.push_str(suffix);
            }
            return next;
        }
    }
    let mut next = body.trim_end().to_string();
    if !next.is_empty() {
        next.push_str("\n\n");
    }
    next.push_str(marker);
    next
}

fn github_repo_slug(repo: &Path) -> Result<String> {
    let remote = git::output_in(repo, ["remote", "get-url", "origin"])?;
    parse_github_remote(remote.trim())
        .or_else(|| gh_repo_slug(repo).ok())
        .with_context(|| {
            format!(
                "could not determine GitHub repo for {}; set origin to a GitHub URL",
                repo.display()
            )
        })
}

fn parse_github_remote(remote: &str) -> Option<String> {
    let without_git = remote.strip_suffix(".git").unwrap_or(remote);
    if let Some(rest) = without_git.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        return repo_slug_from_host_path(host, path);
    }
    if let Some(rest) = without_git.strip_prefix("ssh://git@") {
        let (host, path) = rest.split_once('/')?;
        return repo_slug_from_host_path(host, path);
    }
    for prefix in ["https://", "http://"] {
        if let Some(rest) = without_git.strip_prefix(prefix) {
            let (host, path) = rest.split_once('/')?;
            return repo_slug_from_host_path(host, path);
        }
    }
    None
}

fn repo_slug_from_host_path(host: &str, path: &str) -> Option<String> {
    let mut parts = path.split('/').filter(|part| !part.is_empty());
    let owner = parts.next()?;
    let repo = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if host == "github.com" {
        Some(format!("{owner}/{repo}"))
    } else {
        Some(format!("{host}/{owner}/{repo}"))
    }
}

fn gh_repo_slug(repo: &Path) -> Result<String> {
    let text = gh_output(Some(repo), ["repo", "view", "--json", "nameWithOwner"])?;
    let value: Value = serde_json::from_str(&text).context("parse gh repo view JSON")?;
    value
        .get("nameWithOwner")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .context("gh repo view did not return nameWithOwner")
}

fn gh_output<I, S>(dir: Option<&Path>, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let bin = env::var("GNIT_GH_BIN").unwrap_or_else(|_| "gh".to_string());
    let mut command = Command::new(bin);
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let output = command.args(args).output().context("run gh")?;
    if !output.status.success() {
        bail!(
            "gh exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn trailer_value(body: &str) -> Option<String> {
    body.lines()
        .find_map(|line| line.trim().strip_prefix("Gnit-Change-Id: "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_state(state: &str) -> String {
    match state.to_ascii_uppercase().as_str() {
        "OPEN" => "open".to_string(),
        "CLOSED" => "closed".to_string(),
        "MERGED" => "merged".to_string(),
        "" => "-".to_string(),
        other => other.to_ascii_lowercase(),
    }
}

fn summarize_checks(value: &Value) -> String {
    let Some(items) = value.as_array() else {
        return "-".to_string();
    };
    if items.is_empty() {
        return "-".to_string();
    }
    let mut pending = false;
    for item in items {
        let status = item
            .get("status")
            .or_else(|| item.get("state"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_uppercase();
        let conclusion = item
            .get("conclusion")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_uppercase();
        if matches!(
            conclusion.as_str(),
            "FAILURE" | "CANCELLED" | "TIMED_OUT" | "ACTION_REQUIRED"
        ) {
            return "fail".to_string();
        }
        if status != "COMPLETED" && conclusion != "SUCCESS" {
            pending = true;
        }
    }
    if pending {
        "pending".to_string()
    } else {
        "pass".to_string()
    }
}

fn parse_gh_version(text: &str) -> Option<(u64, u64, u64)> {
    let line = text.lines().next()?;
    let version = line.split_whitespace().find(|part| {
        part.chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
    })?;
    let mut parts = version.split('.');
    Some((
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ))
}

fn parse_pr_number(url: &str) -> Option<u64> {
    url.trim_end_matches('/').rsplit('/').next()?.parse().ok()
}

fn selector_hint(projection: &Projection) -> String {
    format!(" --change {}", projection.change_id)
}

fn short(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}

fn concise(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(text)
        .to_string()
}
