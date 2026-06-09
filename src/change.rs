use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::git;
use crate::ids;
use crate::metadata::Roster;
use crate::pin;
use crate::trailers;
use crate::trailers::TRAILER;
use crate::workspace;

#[derive(Debug, Clone)]
pub struct ChangeCommit {
    pub repo_id: String,
    pub repo_path: String,
    pub repo_root: PathBuf,
    pub commit: String,
    pub subject: String,
}

#[derive(Debug)]
struct Repo {
    id: String,
    path: String,
    root: PathBuf,
    is_workspace_root: bool,
}

pub fn add(paths: Vec<PathBuf>, all: bool, repo: Option<String>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = workspace::find_gnit_workspace(&cwd)
        .context("not in a Gnit workspace; run `gnit init` first")?;
    let roster = Roster::read(&root)?;
    let repos = workspace_repos(&root, &roster);

    if all && (!paths.is_empty() || repo.is_some()) {
        bail!("use either `gnit add -A` or explicit paths, not both");
    }
    if !all && paths.is_empty() {
        bail!("nothing specified; use `gnit add <path>...` or `gnit add -A`");
    }

    if all {
        for repo in &repos {
            if repo.is_workspace_root {
                git::output_in_args(&repo.root, ["add", "-A", "--", ".", ":(exclude).gnit"])?;
            } else {
                git::output_in(&repo.root, ["add", "-A"])?;
            }
        }
        println!("staged workspace changes");
        return Ok(());
    }

    if let Some(repo_id) = repo {
        let target = repos
            .iter()
            .find(|candidate| candidate.id == repo_id)
            .with_context(|| format!("unknown repo {repo_id}"))?;
        let args = add_args_for_paths(&target.root, &paths)?;
        git::output_in_args(&target.root, args)?;
        println!("staged paths in {}", target.id);
        return Ok(());
    }

    let mut grouped: BTreeMap<String, (&Repo, Vec<PathBuf>)> = BTreeMap::new();
    for path in paths {
        let abs = absolutize(&cwd, &path);
        let owner = owner_for_path(&repos, &abs)
            .with_context(|| format!("{} is outside the Gnit workspace", path.display()))?;
        let repo_path = relative_to(&owner.root, &abs)?;
        grouped
            .entry(owner.id.clone())
            .or_insert((owner, Vec::new()))
            .1
            .push(repo_path);
    }

    for (_, (repo, paths)) in grouped {
        let args = add_args(&paths);
        git::output_in_args(&repo.root, args)?;
    }
    println!("staged paths");
    Ok(())
}

pub fn commit(message: String) -> Result<String> {
    let cwd = env::current_dir()?;
    let root = workspace::find_gnit_workspace(&cwd)
        .context("not in a Gnit workspace; run `gnit init` first")?;
    commit_staged(&root, &message)
}

pub fn land(message: String, name: Option<String>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root = workspace::find_gnit_workspace(&cwd)
        .context("not in a Gnit workspace; run `gnit init` first")?;
    let change_id = commit_staged(&root, &message)?;
    pin::create_with_changes(name, vec![change_id.clone()], false)?;
    println!("landed Change {change_id}");
    Ok(())
}

pub fn show(id: String) -> Result<()> {
    let commits = commits_for_change(&id)?;
    print_change_commits(&id, &commits);
    Ok(())
}

pub fn status(id: String) -> Result<()> {
    let commits = commits_for_change(&id)?;
    println!("Change {id}");
    let mut by_repo: BTreeMap<&str, Vec<&ChangeCommit>> = BTreeMap::new();
    for commit in &commits {
        by_repo.entry(&commit.repo_id).or_default().push(commit);
    }
    for (repo, commits) in by_repo {
        if commits.len() > 1 {
            println!("  {repo}: ambiguous ({} commits)", commits.len());
        } else if let Some(commit) = commits.first() {
            println!("  {repo}: {}", short(&commit.commit));
        }
    }
    Ok(())
}

pub fn log(id: Option<String>) -> Result<()> {
    let root = current_root()?;
    let roster = Roster::read(&root)?;
    let repos = workspace_repos(&root, &roster);
    let commits = scan_change_commits(&repos)?;

    if let Some(id) = id {
        let commits = commits
            .into_iter()
            .filter(|(_, commit)| commit == &id)
            .map(|(change, _)| change)
            .collect::<Vec<_>>();
        if commits.is_empty() {
            bail!("change {id} not found");
        }
        print_change_commits(&id, &commits);
        return Ok(());
    }

    let ids = commits
        .into_iter()
        .map(|(_, id)| id)
        .collect::<BTreeSet<_>>();
    if ids.is_empty() {
        println!("No Gnit changes found.");
    } else {
        for id in ids {
            println!("{id}");
        }
    }
    Ok(())
}

pub fn diff(id: String) -> Result<()> {
    let commits = commits_for_change(&id)?;
    println!("Change {id}");
    for commit in commits {
        println!("\n== {} ({}) ==", commit.repo_id, commit.repo_path);
        let diff = git::output_in_args(
            &commit.repo_root,
            ["show", "--stat", "--oneline", &commit.commit],
        )?;
        print!("{diff}");
    }
    Ok(())
}

pub fn ensure_exists(id: &str) -> Result<()> {
    commits_for_change(id).map(|_| ())
}

fn commit_staged(root: &Path, message: &str) -> Result<String> {
    let roster = Roster::read(root)?;
    let repos = workspace_repos(root, &roster);
    let change_id = ids::change_id();
    let full_message = format!("{message}\n\n{TRAILER}: {change_id}");
    let mut committed = Vec::new();

    for repo in repos {
        ensure_no_staged_workspace_metadata(&repo)?;
        if !has_staged_changes(&repo)? {
            continue;
        }
        git::output_in(&repo.root, ["commit", "-m", &full_message])?;
        let commit = git::output_in(&repo.root, ["rev-parse", "HEAD"])?
            .trim()
            .to_string();
        committed.push((repo.id, commit));
    }

    if committed.is_empty() {
        bail!("no staged changes to commit");
    }

    println!("created Change {change_id}");
    for (repo, commit) in committed {
        println!("  {repo}: {}", short(&commit));
    }
    Ok(change_id)
}

fn commits_for_change(id: &str) -> Result<Vec<ChangeCommit>> {
    let root = current_root()?;
    let roster = Roster::read(&root)?;
    let repos = workspace_repos(&root, &roster);
    let commits = scan_change_commits(&repos)?
        .into_iter()
        .filter(|(_, change_id)| change_id == id)
        .map(|(commit, _)| commit)
        .collect::<Vec<_>>();
    if commits.is_empty() {
        bail!("change {id} not found");
    }
    Ok(commits)
}

fn scan_change_commits(repos: &[Repo]) -> Result<Vec<(ChangeCommit, String)>> {
    let mut commits = Vec::new();
    for repo in repos {
        let log = git::output_in_args(&repo.root, ["log", "--all", "--format=%H%x1f%s%x1f%B%x1e"])
            .unwrap_or_default();
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
            let Some(change_id) = trailers::change_id(body) else {
                continue;
            };
            commits.push((
                ChangeCommit {
                    repo_id: repo.id.clone(),
                    repo_path: repo.path.clone(),
                    repo_root: repo.root.clone(),
                    commit: hash.to_string(),
                    subject: subject.to_string(),
                },
                change_id,
            ));
        }
    }
    Ok(commits)
}

fn workspace_repos(root: &Path, roster: &Roster) -> Vec<Repo> {
    let mut repos = Vec::new();
    if git::is_git_repo(root) {
        repos.push(Repo {
            id: "root".to_string(),
            path: ".".to_string(),
            root: root.to_path_buf(),
            is_workspace_root: true,
        });
    }
    for member in &roster.members {
        repos.push(Repo {
            id: member.id.clone(),
            path: member.path.clone(),
            root: root.join(&member.path),
            is_workspace_root: false,
        });
    }
    repos
}

fn owner_for_path<'a>(repos: &'a [Repo], abs: &Path) -> Option<&'a Repo> {
    repos
        .iter()
        .filter(|repo| abs.starts_with(&repo.root))
        .max_by_key(|repo| repo.root.as_os_str().len())
}

fn has_staged_changes(repo: &Repo) -> Result<bool> {
    if repo.is_workspace_root {
        return git::status_in_args(
            &repo.root,
            ["diff", "--cached", "--quiet", "--", ".", ":(exclude).gnit"],
        )
        .map(|clean| !clean);
    }
    git::status_in_args(&repo.root, ["diff", "--cached", "--quiet"]).map(|clean| !clean)
}

fn ensure_no_staged_workspace_metadata(repo: &Repo) -> Result<()> {
    if !repo.is_workspace_root {
        return Ok(());
    }
    let metadata_clean =
        git::status_in_args(&repo.root, ["diff", "--cached", "--quiet", "--", ".gnit"])?;
    if !metadata_clean {
        bail!(
            "workspace metadata is staged; commit or unstage .gnit separately before gnit commit"
        );
    }
    Ok(())
}

fn add_args(paths: &[PathBuf]) -> Vec<String> {
    let mut args = vec!["add".to_string(), "--".to_string()];
    args.extend(paths.iter().map(|path| path.to_string_lossy().to_string()));
    args
}

fn add_args_for_paths(root: &Path, paths: &[PathBuf]) -> Result<Vec<String>> {
    let mut repo_paths = Vec::new();
    for path in paths {
        if path.is_absolute() {
            repo_paths.push(relative_to(root, path)?);
        } else {
            repo_paths.push(path.clone());
        }
    }
    Ok(add_args(&repo_paths))
}

fn current_root() -> Result<PathBuf> {
    let cwd = env::current_dir()?;
    workspace::find_gnit_workspace(&cwd).context("not in a Gnit workspace; run `gnit init` first")
}

fn absolutize(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn relative_to(root: &Path, path: &Path) -> Result<PathBuf> {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .with_context(|| format!("{} is outside {}", path.display(), root.display()))
}

fn print_change_commits(id: &str, commits: &[ChangeCommit]) {
    println!("Change {id}");
    for commit in commits {
        println!(
            "  {}  {}  {}",
            commit.repo_id,
            short(&commit.commit),
            commit.subject
        );
    }
}

fn short(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}
