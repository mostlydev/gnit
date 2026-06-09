use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::git;
use crate::metadata::{Roster, RosterMember, ROSTER_PATH};

const AGENT_GUIDANCE_START: &str = "<!-- gnit:workspace:start -->";
const AGENT_GUIDANCE_BLOCK: &str = r#"<!-- gnit:workspace:start -->
> **Gnit workspace** — this repository is one of several Git repos coordinated by Gnit.
> For changes that span more than one repo, drive them with the `gnit` CLI and the Gnit
> skill (run `gnit --help`) instead of hand-managing submodules or raw `git` across repos.
<!-- gnit:workspace:end -->
"#;

pub fn init(control: bool, local: bool, remote: Option<String>) -> Result<()> {
    let cwd = env::current_dir()?;
    let gnit_dir = cwd.join(".gnit");
    let roster = cwd.join(ROSTER_PATH);

    if roster.exists() {
        bail!("Gnit workspace already exists at {}", cwd.display());
    }

    if control && !git::is_git_repo(&cwd) {
        git::output_in(&cwd, ["init"]).context("initialize control git repo")?;
    }

    fs::create_dir_all(&gnit_dir).context("create .gnit")?;
    let _lock = crate::lock::WorkspaceLock::acquire(&cwd)?;
    if roster.exists() {
        bail!("Gnit workspace already exists at {}", cwd.display());
    }

    let mode = if local {
        "local"
    } else if control {
        "control"
    } else {
        "shared"
    };
    let roster_doc = Roster::new(mode, remote);
    roster_doc.write(&cwd)?;
    repair_required_excludes(&cwd, &roster_doc)?;
    let agent_guidance = ensure_agent_guidance(&cwd)?;

    if !local {
        commit_metadata_with_paths(&cwd, "Initialize Gnit workspace", &agent_guidance).ok();
    }

    println!("initialized Gnit workspace");
    println!("  root: {}", cwd.display());
    println!("  roster: {}", roster.display());
    Ok(())
}

pub fn adopt(paths: Vec<PathBuf>, id: Option<String>, no_commit: bool) -> Result<()> {
    if id.is_some() && paths.len() != 1 {
        bail!("--id can only be used when adopting one path");
    }

    let cwd = env::current_dir()?;
    let root =
        find_gnit_workspace(&cwd).context("not in a Gnit workspace; run `gnit init` first")?;
    let mut roster = Roster::read(&root)?;
    let mut adopted = Vec::new();

    for path in paths {
        let abs = absolutize(&cwd, &path);
        if !git::is_git_repo_root(&abs) {
            if git::is_git_repo(&abs) {
                bail!(
                    "{} is a subdirectory of a Git repo, not a repository root; adopt the repo itself",
                    path.display()
                );
            }
            bail!("{} is not a Git repository", path.display());
        }

        let rel = relative_to(&root, &abs)?;
        let member_id = id.clone().unwrap_or_else(|| {
            rel.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
        if roster.contains_id(&member_id) {
            bail!("member id {member_id} already exists");
        }

        let remote = git::output_in(&abs, ["remote", "get-url", "origin"])
            .ok()
            .map(|remote| remote.trim().to_string())
            .filter(|remote| !remote.is_empty());
        let exclude_path = rel.to_string_lossy().to_string();
        roster.members.push(RosterMember {
            id: member_id.clone(),
            path: exclude_path.clone(),
            remote,
            required_excludes: vec![exclude_path],
        });
        adopted.push((member_id, rel));
    }

    roster.write(&root)?;
    repair_required_excludes(&root, &roster)?;

    if !no_commit {
        commit_metadata(&root, "Update Gnit roster").ok();
    }

    let ids = adopted
        .iter()
        .map(|(id, _)| id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    println!("adopted {ids}");
    Ok(())
}

pub fn doctor() -> Result<()> {
    println!("Gnit doctor");
    println!("  version: {}", env!("CARGO_PKG_VERSION"));
    println!("  commit: {}", build_commit());

    match git::output(["--version"]) {
        Ok(version) => println!("  git: {}", version.trim()),
        Err(err) => println!("  git: not available ({err})"),
    }
    report_gh_health();

    match find_gnit_workspace(env::current_dir()?.as_path()) {
        Some(root) => {
            println!("  workspace: {}", root.display());
            let roster = Roster::read(&root)?;
            println!("  roster members: {}", roster.members.len());
            repair_required_excludes(&root, &roster)?;
            println!("  exclude repair: ok");
            let guidance_added = ensure_agent_guidance(&root)?;
            let guidance_status = if guidance_added.is_empty() {
                "ok"
            } else {
                "added"
            };
            println!("  agent guidance: {guidance_status}");
            report_legacy_guidance(&root);
            report_member_health(&root, &roster)?;
            report_pin_health(&root)?;
        }
        None => match crate::migrate::find_legacy_workspace(env::current_dir()?.as_path()) {
            Some(root) => println!(
                "  workspace: legacy nit metadata at {}; run `gnit migrate`",
                root.display()
            ),
            None => println!("  workspace: not found"),
        },
    }

    println!("  upkeep: automatic non-destructive upkeep enabled");
    Ok(())
}

/// A migrated workspace can still carry the pre-rename guidance block, which
/// tells agents to drive the workspace with the old `nit` CLI.
fn report_legacy_guidance(root: &Path) {
    for name in ["AGENTS.md", "CLAUDE.md"] {
        if let Ok(text) = fs::read_to_string(root.join(name)) {
            if text.contains(crate::migrate::LEGACY_GUIDANCE_START) {
                println!("  agent guidance: legacy nit block in {name}; run `gnit migrate`");
            }
        }
    }
}

fn report_gh_health() {
    let gh = env::var("GNIT_GH_BIN").unwrap_or_else(|_| "gh".to_string());
    match Command::new(&gh).arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("gh available")
                .to_string();
            let auth = Command::new(&gh).args(["auth", "status"]).output();
            match auth {
                Ok(auth) if auth.status.success() => println!("  gh: {version}; auth ok"),
                Ok(auth) => println!(
                    "  gh: {version}; auth unavailable ({})",
                    String::from_utf8_lossy(&auth.stderr).trim()
                ),
                Err(err) => println!("  gh: {version}; auth check failed ({err})"),
            }
        }
        Ok(output) => println!(
            "  gh: not available ({})",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Err(err) => println!("  gh: not available ({err})"),
    }
}

pub fn ignore(paths: Vec<PathBuf>) -> Result<()> {
    if paths.is_empty() {
        bail!("nothing specified; use `gnit ignore <path>...`");
    }
    let cwd = env::current_dir()?;
    let root =
        find_gnit_workspace(&cwd).context("not in a Gnit workspace; run `gnit init` first")?;
    let mut roster = Roster::read(&root)?;
    for path in paths {
        let rel = relative_to(&root, &absolutize(&cwd, &path))?;
        let entry = rel.to_string_lossy().to_string();
        if !roster.ignored.iter().any(|ignored| ignored == &entry) {
            roster.ignored.push(entry);
        }
    }
    roster.write(&root)?;
    repair_required_excludes(&root, &roster)?;
    commit_metadata(&root, "Update Gnit ignored paths").ok();
    println!("updated ignored paths");
    Ok(())
}

pub fn import_submodule(path: PathBuf, id: Option<String>) -> Result<()> {
    let cwd = env::current_dir()?;
    let root =
        find_gnit_workspace(&cwd).context("not in a Gnit workspace; run `gnit init` first")?;
    let abs = absolutize(&cwd, &path);
    let rel = relative_to(&root, &abs)?;
    let rel_text = rel.to_string_lossy().to_string();
    let status = git::output_in(&root, ["status", "--porcelain", "--untracked-files=no"])?;
    if !status.trim().is_empty() {
        bail!("root has tracked changes; commit or stash them before import-submodule");
    }
    let stage = git::output_in_args(&root, ["ls-files", "--stage", "--", rel_text.as_str()])?;
    if !stage.starts_with("160000 ") {
        bail!("{} is not a tracked Git submodule", path.display());
    }

    git::output_in_args(&root, ["rm", "--cached", rel_text.as_str()])?;
    let section = format!("submodule.{rel_text}");
    git::output_in_args(
        &root,
        ["config", "-f", ".gitmodules", "--remove-section", &section],
    )
    .ok();

    adopt(vec![path], id, true)?;
    git::output_in(&root, ["add", ".gnit"])?;
    if root.join(".gitmodules").exists() {
        git::output_in(&root, ["add", ".gitmodules"])?;
    }
    git::output_in(
        &root,
        ["commit", "-m", &format!("Import Gnit member {rel_text}")],
    )?;
    println!("imported submodule {rel_text}");
    Ok(())
}

pub fn find_gnit_workspace(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        if dir.join(ROSTER_PATH).exists() {
            return Some(dir.to_path_buf());
        }
    }
    None
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
        .with_context(|| format!("{} is outside workspace {}", path.display(), root.display()))
}

/// Reapply the roster's required member excludes and ignored paths to the root
/// repo's local `.git/info/exclude`. Returns the number of entries added; only
/// writes when something was missing, so it is a cheap no-op on a healthy tree.
pub(crate) fn repair_required_excludes(root: &Path, roster: &Roster) -> Result<usize> {
    let exclude = root.join(".git/info/exclude");
    if !exclude.exists() {
        return Ok(0);
    }

    let mut text = match fs::read_to_string(&exclude) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(err) => {
            return Err(err).with_context(|| format!("read git exclude {}", exclude.display()));
        }
    };
    let mut added = 0;
    added += append_exclude(&mut text, crate::lock::LOCK_EXCLUDE);
    added += append_exclude(&mut text, crate::cache::CACHE_EXCLUDE);
    for member in &roster.members {
        for entry in &member.required_excludes {
            added += append_exclude(&mut text, entry);
        }
    }
    for entry in &roster.ignored {
        added += append_exclude(&mut text, entry);
    }
    if added > 0 {
        fs::write(exclude, text).context("write git exclude")?;
    }
    Ok(added)
}

pub(crate) fn ensure_agent_guidance(root: &Path) -> Result<Vec<PathBuf>> {
    let targets = agent_guidance_targets(root);
    let mut changed = Vec::new();
    for rel in targets {
        let path = root.join(&rel);
        let mut text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => {
                return Err(err).with_context(|| format!("read agent guidance {}", path.display()));
            }
        };
        if text.contains(AGENT_GUIDANCE_START) {
            continue;
        }
        append_agent_guidance(&mut text);
        fs::write(&path, text)
            .with_context(|| format!("write agent guidance {}", path.display()))?;
        changed.push(rel);
    }
    Ok(changed)
}

fn agent_guidance_targets(root: &Path) -> Vec<PathBuf> {
    let agents = PathBuf::from("AGENTS.md");
    let claude = PathBuf::from("CLAUDE.md");
    let mut targets = Vec::new();
    if root.join(&agents).exists() {
        targets.push(agents.clone());
    }
    if root.join(&claude).exists() {
        targets.push(claude);
    }
    if targets.is_empty() {
        targets.push(agents);
    }
    targets
}

fn append_agent_guidance(text: &mut String) {
    if !text.is_empty() {
        if !text.ends_with('\n') {
            text.push('\n');
        }
        if !text.ends_with("\n\n") {
            text.push('\n');
        }
    }
    text.push_str(AGENT_GUIDANCE_BLOCK);
}

fn append_exclude(text: &mut String, entry: &str) -> usize {
    if text.lines().any(|line| line == entry) {
        return 0;
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(entry);
    text.push('\n');
    1
}

fn report_member_health(root: &Path, roster: &Roster) -> Result<()> {
    for member in &roster.members {
        let member_root = root.join(&member.path);
        if !member_root.exists() {
            println!("  member {}: missing", member.id);
            continue;
        }
        if !git::is_git_repo_root(&member_root) {
            println!("  member {}: not a git repo", member.id);
            continue;
        }
        if let Some(expected) = &member.remote {
            let actual = git::output_in(&member_root, ["remote", "get-url", "origin"])
                .unwrap_or_default()
                .trim()
                .to_string();
            if !actual.is_empty() && actual != *expected {
                println!("  member {}: remote drift", member.id);
            }
        }
    }
    Ok(())
}

fn report_pin_health(root: &Path) -> Result<()> {
    let pins_dir = root.join(crate::metadata::PINS_DIR);
    if !pins_dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(&pins_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("yaml") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let pin = crate::metadata::Pin::read(root, id)?;
        for member in &pin.members {
            let member_root = root.join(&member.path);
            if member_root.exists()
                && !git::status_in_args(
                    &member_root,
                    ["cat-file", "-e", &format!("{}^{{commit}}", member.commit)],
                )?
            {
                println!("  pin {}: dangling {}", pin.id, member.id);
            }
        }
    }
    Ok(())
}

pub(crate) fn commit_metadata(root: &Path, message: &str) -> Result<()> {
    commit_metadata_with_paths(root, message, &[])
}

fn commit_metadata_with_paths(root: &Path, message: &str, extra_paths: &[PathBuf]) -> Result<()> {
    if !git::is_git_repo(root) {
        return Ok(());
    }

    // Local excludes (.git/info/exclude) are intentionally local; never committed.
    let mut paths = vec![PathBuf::from(".gnit")];
    paths.extend(extra_paths.iter().cloned());

    stage_metadata_paths(root, extra_paths)?;

    let mut status_args = vec![
        OsString::from("status"),
        OsString::from("--porcelain"),
        OsString::from("--"),
    ];
    status_args.extend(paths.iter().map(|path| path.as_os_str().to_os_string()));
    status_args.push(OsString::from(":(exclude).gnit/lock"));
    status_args.push(OsString::from(":(exclude).gnit/cache"));
    let status = git::output_in_args(root, status_args)?;
    if !status.trim().is_empty() {
        // Pathspec-scope the commit so an unrelated staged change in the root
        // repo is never swept into the Gnit metadata commit.
        let mut commit_args = vec![
            OsString::from("commit"),
            OsString::from("-m"),
            OsString::from(message),
            OsString::from("--"),
        ];
        commit_args.extend(paths.iter().map(|path| path.as_os_str().to_os_string()));
        git::output_in_args(root, commit_args)?;
    }
    Ok(())
}

fn stage_metadata_paths(root: &Path, extra_paths: &[PathBuf]) -> Result<()> {
    let tracked_metadata = git::output_in(root, ["ls-files", ".gnit"])?;
    if !tracked_metadata.trim().is_empty() {
        git::output_in(root, ["add", "-u", "--", ".gnit"])?;
    }

    let mut add_paths = existing_metadata_files(root)?;
    add_paths.extend(extra_paths.iter().cloned());
    if add_paths.is_empty() {
        return Ok(());
    }

    let mut add_args = vec![OsString::from("add"), OsString::from("--")];
    add_args.extend(add_paths.iter().map(|path| path.as_os_str().to_os_string()));
    git::output_in_args(root, add_args)?;
    Ok(())
}

fn existing_metadata_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    collect_metadata_files(root, Path::new(".gnit"), &mut paths)?;
    Ok(paths)
}

fn collect_metadata_files(root: &Path, rel: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
    let dir = root.join(rel);
    if !dir.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(&dir)
        .with_context(|| format!("read metadata directory {}", dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("read metadata directory {}", dir.display()))?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let child_rel = rel.join(entry.file_name());
        if child_rel == Path::new(crate::lock::LOCK_EXCLUDE)
            || child_rel == Path::new(crate::cache::CACHE_DIR)
        {
            continue;
        }
        let file_type = entry
            .file_type()
            .with_context(|| format!("read metadata entry {}", entry.path().display()))?;
        if file_type.is_dir() {
            collect_metadata_files(root, &child_rel, paths)?;
        } else {
            paths.push(child_rel);
        }
    }
    Ok(())
}

fn build_commit() -> &'static str {
    match option_env!("GNIT_COMMIT") {
        Some(commit) => commit,
        None => "dev",
    }
}
