//! Ref-keyed cache of change discovery, so `gnit status`/`log`/`change`
//! don't re-run `git log --all` over every member on every invocation.
//!
//! One JSON file per repo under `.gnit/cache/`, holding the extracted
//! `(commit, time, subject, change_id)` tuples plus the repo's ref-state
//! (raw `for-each-ref` listing + HEAD) as the invalidation key. The cache is
//! purely local (excluded like `.gnit/lock`), self-healing on corruption, and
//! written with a tempfile + atomic rename so lock-free readers can race
//! safely.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::git;
use crate::trailers;

pub(crate) const CACHE_EXCLUDE: &str = ".gnit/cache/";
pub(crate) const CACHE_DIR: &str = ".gnit/cache";

const CACHE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedCommit {
    pub commit: String,
    pub time: i64,
    pub subject: String,
    pub change_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    ref_state: String,
    commits: Vec<CachedCommit>,
}

/// Commits carrying a `Gnit-Change-Id` for one repo, served from the
/// ref-keyed cache when fresh, rescanned (and re-cached) when stale.
pub fn change_commits(
    workspace_root: &Path,
    repo_id: &str,
    repo_root: &Path,
) -> Result<Vec<CachedCommit>> {
    let ref_state = ref_state(repo_root);
    let path = cache_path(workspace_root, repo_id);

    if let Some(cached) = read_cache(&path, &ref_state) {
        return Ok(cached);
    }

    let commits = scan(repo_root);
    write_cache(workspace_root, &path, &ref_state, &commits);
    Ok(commits)
}

/// The invalidation key: every ref with its target, plus HEAD. Any ref
/// movement (commit, fetch, branch create/delete, amend) changes the key.
fn ref_state(repo_root: &Path) -> String {
    let refs = git::output_in_args(
        repo_root,
        ["for-each-ref", "--format=%(refname) %(objectname)"],
    )
    .unwrap_or_default();
    let head = git::output_in(repo_root, ["rev-parse", "HEAD"]).unwrap_or_default();
    format!("{}\nHEAD {}", refs.trim_end(), head.trim())
}

fn cache_path(workspace_root: &Path, repo_id: &str) -> PathBuf {
    let safe_id: String = repo_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    workspace_root
        .join(CACHE_DIR)
        .join(format!("changes-{safe_id}.json"))
}

fn read_cache(path: &Path, ref_state: &str) -> Option<Vec<CachedCommit>> {
    let text = fs::read_to_string(path).ok()?;
    let file: CacheFile = serde_json::from_str(&text).ok()?;
    (file.version == CACHE_VERSION && file.ref_state == ref_state).then_some(file.commits)
}

/// Best effort: discovery must keep working when the cache directory is
/// unwritable, so write failures are swallowed and the next run rescans.
fn write_cache(workspace_root: &Path, path: &Path, ref_state: &str, commits: &[CachedCommit]) {
    let file = CacheFile {
        version: CACHE_VERSION,
        ref_state: ref_state.to_string(),
        commits: commits.to_vec(),
    };
    let Ok(text) = serde_json::to_string(&file) else {
        return;
    };
    let dir = workspace_root.join(CACHE_DIR);
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    if fs::write(&temp, text).is_ok() && fs::rename(&temp, path).is_err() {
        let _ = fs::remove_file(&temp);
    }
}

fn scan(repo_root: &Path) -> Vec<CachedCommit> {
    let log = git::output_in_args(
        repo_root,
        ["log", "--all", "--format=%H%x1f%ct%x1f%s%x1f%B%x1e"],
    )
    .unwrap_or_default();
    let mut commits = Vec::new();
    for record in log.split('\x1e') {
        let record = record.trim();
        if record.is_empty() {
            continue;
        }
        let mut fields = record.splitn(4, '\x1f');
        let (Some(hash), Some(ct), Some(subject), Some(body)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let Some(change_id) = trailers::change_id(body) else {
            continue;
        };
        commits.push(CachedCommit {
            commit: hash.to_string(),
            time: ct.trim().parse().unwrap_or(0),
            subject: subject.trim().to_string(),
            change_id,
        });
    }
    commits
}
