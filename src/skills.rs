use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::cli::{SkillsInstallArgs, SkillsUninstallArgs};

const SKILL_NAME: &str = "nit";
const SKILL_FILE: &str = "SKILL.md";
const OWNERSHIP_MARKER: &str = ".nit-skill-managed";
const OWNERSHIP_MARKER_CONTENT: &str = "nit\n";
const BUNDLED_SKILL: &str = include_str!("../skills/nit/SKILL.md");

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Harness {
    ClaudeCode,
    Codex,
    OpenCode,
    Grok,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum InstallMode {
    Link,
    Copy,
}

#[derive(Debug)]
struct Paths {
    home: PathBuf,
    data_dir: PathBuf,
    grok_home: PathBuf,
}

#[derive(Debug)]
struct Target {
    harness: Harness,
    selected: bool,
    explicit: bool,
    base_dir: PathBuf,
    target_dir: PathBuf,
}

#[derive(Debug)]
struct ActionResult {
    harness: Harness,
    status: &'static str,
    message: String,
    ok: bool,
}

pub fn install(args: SkillsInstallArgs) -> Result<()> {
    let mode = install_mode(args.copy, args.link);
    let paths = Paths::resolve()?;
    let managed_source = managed_source_dir(&paths);
    let targets = select_targets(&args.harnesses, args.all, &paths)?;
    if args.print {
        print_install_plan(&targets, &managed_source, mode, args.force);
        return Ok(());
    }

    if targets.iter().any(|target| target.selected) {
        materialize_managed_source(&managed_source)?;
    }

    let results = targets
        .iter()
        .map(|target| {
            if !target.selected {
                return Ok(ActionResult {
                    harness: target.harness,
                    status: "skipped",
                    message: format!(
                        "harness config directory not found: {}",
                        target.base_dir.display()
                    ),
                    ok: true,
                });
            }
            install_target(target, &managed_source, mode, args.force)
        })
        .collect::<Result<Vec<_>>>()?;
    report_results(results, "skills install")
}

pub fn uninstall(args: SkillsUninstallArgs) -> Result<()> {
    let paths = Paths::resolve()?;
    let managed_source = managed_source_dir(&paths);
    let targets = select_targets(&args.harnesses, args.all, &paths)?;
    if args.print {
        print_uninstall_plan(&targets);
        return Ok(());
    }

    let results = targets
        .iter()
        .map(|target| {
            if !target.selected {
                return Ok(ActionResult {
                    harness: target.harness,
                    status: "skipped",
                    message: format!(
                        "harness config directory not found: {}",
                        target.base_dir.display()
                    ),
                    ok: true,
                });
            }
            uninstall_target(target, &managed_source)
        })
        .collect::<Result<Vec<_>>>()?;
    report_results(results, "skills uninstall")
}

pub fn list() -> Result<()> {
    let paths = Paths::resolve()?;
    let managed_source = managed_source_dir(&paths);
    println!("Nit skills");
    println!("  managed source: {}", managed_source.display());
    for harness in Harness::ALL {
        let base_dir = harness.base_dir(&paths);
        let target_dir = harness.target_dir(&paths);
        let state = target_state(&target_dir, &managed_source)?;
        let detected = if base_dir.exists() {
            "detected"
        } else {
            "not-detected"
        };
        println!(
            "  {:<11} {:<12} {:<12} {}",
            harness.id(),
            state,
            detected,
            target_dir.display()
        );
    }
    Ok(())
}

fn install_mode(copy: bool, _link: bool) -> InstallMode {
    if copy {
        InstallMode::Copy
    } else {
        InstallMode::Link
    }
}

fn select_targets(values: &[String], all: bool, paths: &Paths) -> Result<Vec<Target>> {
    if all && !values.is_empty() {
        bail!("use either explicit harnesses or --all, not both");
    }
    if !all && values.is_empty() {
        bail!(
            "specify at least one harness ({}) or pass --all",
            Harness::SUPPORTED.join(", ")
        );
    }

    let harnesses = if all {
        Harness::ALL.to_vec()
    } else {
        parse_harnesses(values)?
    };

    Ok(harnesses
        .into_iter()
        .map(|harness| {
            let base_dir = harness.base_dir(paths);
            let target_dir = harness.target_dir(paths);
            let detected = base_dir.exists();
            Target {
                harness,
                selected: !all || detected,
                explicit: !all,
                base_dir,
                target_dir,
            }
        })
        .collect())
}

fn parse_harnesses(values: &[String]) -> Result<Vec<Harness>> {
    let mut harnesses = Vec::new();
    for value in values {
        let harness = Harness::parse(value)?;
        if !harnesses.contains(&harness) {
            harnesses.push(harness);
        }
    }
    Ok(harnesses)
}

fn managed_source_dir(paths: &Paths) -> PathBuf {
    paths.data_dir.join("skills").join(SKILL_NAME)
}

fn materialize_managed_source(managed_source: &Path) -> Result<()> {
    prepare_dir(managed_source).with_context(|| {
        format!(
            "prepare managed Nit skill source {}",
            managed_source.display()
        )
    })?;
    write_if_changed(&managed_source.join(SKILL_FILE), BUNDLED_SKILL)?;
    write_if_changed(
        &managed_source.join(OWNERSHIP_MARKER),
        OWNERSHIP_MARKER_CONTENT,
    )?;
    Ok(())
}

fn install_target(
    target: &Target,
    managed_source: &Path,
    mode: InstallMode,
    force: bool,
) -> Result<ActionResult> {
    let before = inspect_target(&target.target_dir, managed_source)?;
    if before.is_foreign() && !force {
        return Ok(ActionResult {
            harness: target.harness,
            status: "failed",
            message: format!(
                "{} already exists and is not managed by Nit; rerun with --force to replace it",
                target.target_dir.display()
            ),
            ok: false,
        });
    }

    let created_base_dir = target.explicit && !target.base_dir.exists();
    if created_base_dir {
        fs::create_dir_all(&target.base_dir)
            .with_context(|| format!("create harness directory {}", target.base_dir.display()))?;
    }

    let mut result = match mode {
        InstallMode::Link => install_link(target, managed_source, before, force),
        InstallMode::Copy => install_copy(target, managed_source, before, force),
    }?;
    if created_base_dir {
        result.message.push_str(&format!(
            " (created harness directory {})",
            target.base_dir.display()
        ));
    }
    Ok(result)
}

fn install_link(
    target: &Target,
    managed_source: &Path,
    before: ExistingTarget,
    force: bool,
) -> Result<ActionResult> {
    if matches!(before, ExistingTarget::LinkedCurrent) {
        return Ok(ActionResult {
            harness: target.harness,
            status: "already-present",
            message: format!("linked to {}", managed_source.display()),
            ok: true,
        });
    }

    if before.exists() {
        remove_target(&target.target_dir, force)?;
    }
    fs::create_dir_all(
        target
            .target_dir
            .parent()
            .context("skill target has no parent directory")?,
    )
    .with_context(|| format!("create {}", target.target_dir.display()))?;
    symlink_dir(managed_source, &target.target_dir).with_context(|| {
        format!(
            "link {} -> {}",
            managed_source.display(),
            target.target_dir.display()
        )
    })?;
    Ok(ActionResult {
        harness: target.harness,
        status: if before.exists() { "updated" } else { "added" },
        message: format!("linked to {}", managed_source.display()),
        ok: true,
    })
}

fn install_copy(
    target: &Target,
    managed_source: &Path,
    before: ExistingTarget,
    force: bool,
) -> Result<ActionResult> {
    if matches!(before, ExistingTarget::CopiedCurrent) {
        return Ok(ActionResult {
            harness: target.harness,
            status: "already-present",
            message: format!("copied skill is current at {}", target.target_dir.display()),
            ok: true,
        });
    }

    if before.exists() {
        remove_target(&target.target_dir, force)?;
    }
    copy_skill_dir(managed_source, &target.target_dir)?;
    Ok(ActionResult {
        harness: target.harness,
        status: if before.exists() { "updated" } else { "added" },
        message: format!("copied to {}", target.target_dir.display()),
        ok: true,
    })
}

fn uninstall_target(target: &Target, managed_source: &Path) -> Result<ActionResult> {
    let before = inspect_target(&target.target_dir, managed_source)?;
    if matches!(before, ExistingTarget::Absent) {
        return Ok(ActionResult {
            harness: target.harness,
            status: "already-absent",
            message: format!("{} is not installed", target.target_dir.display()),
            ok: true,
        });
    }
    if before.is_foreign() {
        return Ok(ActionResult {
            harness: target.harness,
            status: "failed",
            message: format!(
                "{} exists but is not managed by Nit; leaving it untouched",
                target.target_dir.display()
            ),
            ok: false,
        });
    }

    remove_target(&target.target_dir, true)?;
    Ok(ActionResult {
        harness: target.harness,
        status: "removed",
        message: format!("removed {}", target.target_dir.display()),
        ok: true,
    })
}

fn print_install_plan(targets: &[Target], managed_source: &Path, mode: InstallMode, force: bool) {
    if targets.iter().any(|target| target.selected) {
        println!("refresh managed source: {}", managed_source.display());
    }
    for target in targets {
        if !target.selected {
            println!(
                "[{}] skipped: harness config directory not found: {}",
                target.harness.id(),
                target.base_dir.display()
            );
            continue;
        }
        let verb = match mode {
            InstallMode::Link => "link",
            InstallMode::Copy => "copy",
        };
        let force_suffix = if force { " (force)" } else { "" };
        if target.explicit && !target.base_dir.exists() {
            println!(
                "[{}] create harness directory: {}",
                target.harness.id(),
                target.base_dir.display()
            );
        }
        println!(
            "[{}] {verb} {} -> {}{}",
            target.harness.id(),
            managed_source.display(),
            target.target_dir.display(),
            force_suffix
        );
    }
}

fn print_uninstall_plan(targets: &[Target]) {
    for target in targets {
        if !target.selected {
            println!(
                "[{}] skipped: harness config directory not found: {}",
                target.harness.id(),
                target.base_dir.display()
            );
            continue;
        }
        println!(
            "[{}] remove {}",
            target.harness.id(),
            target.target_dir.display()
        );
    }
}

fn report_results(results: Vec<ActionResult>, action: &str) -> Result<()> {
    let mut failed = false;
    for result in results {
        println!(
            "[{}] {}: {}",
            result.harness.id(),
            result.status,
            result.message
        );
        if !result.ok {
            failed = true;
        }
    }
    if failed {
        bail!("{action} completed with failures");
    }
    Ok(())
}

fn prepare_dir(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            remove_target(path, true)?;
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).with_context(|| format!("inspect {}", path.display())),
    }
    fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))
}

fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    match fs::read_to_string(path) {
        Ok(existing) if existing == content => Ok(()),
        Ok(_) => fs::write(path, content).with_context(|| format!("write {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::write(path, content).with_context(|| format!("write {}", path.display()))
        }
        Err(error) => Err(error).with_context(|| format!("read {}", path.display())),
    }
}

fn copy_skill_dir(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target).with_context(|| format!("create {}", target.display()))?;
    fs::copy(source.join(SKILL_FILE), target.join(SKILL_FILE))
        .with_context(|| format!("copy skill to {}", target.display()))?;
    fs::copy(source.join(OWNERSHIP_MARKER), target.join(OWNERSHIP_MARKER))
        .with_context(|| format!("copy marker to {}", target.display()))?;
    Ok(())
}

fn remove_target(path: &Path, force: bool) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).with_context(|| format!("inspect {}", path.display())),
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path).with_context(|| format!("remove {}", path.display()))?;
        return Ok(());
    }
    if metadata.is_dir() {
        fs::remove_dir_all(path).with_context(|| format!("remove {}", path.display()))?;
        return Ok(());
    }
    if force {
        fs::remove_file(path).with_context(|| format!("remove {}", path.display()))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ExistingTarget {
    Absent,
    LinkedCurrent,
    ForeignLink,
    CopiedCurrent,
    CopiedStale,
    Foreign,
}

impl ExistingTarget {
    fn exists(self) -> bool {
        !matches!(self, Self::Absent)
    }

    fn is_foreign(self) -> bool {
        matches!(self, Self::Foreign | Self::ForeignLink)
    }
}

fn inspect_target(target: &Path, managed_source: &Path) -> Result<ExistingTarget> {
    let metadata = match fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ExistingTarget::Absent);
        }
        Err(error) => return Err(error).with_context(|| format!("inspect {}", target.display())),
    };

    if metadata.file_type().is_symlink() {
        let link =
            fs::read_link(target).with_context(|| format!("read link {}", target.display()))?;
        let resolved = if link.is_absolute() {
            link
        } else {
            target.parent().unwrap_or_else(|| Path::new(".")).join(link)
        };
        return if same_path(&resolved, managed_source) {
            Ok(ExistingTarget::LinkedCurrent)
        } else {
            Ok(ExistingTarget::ForeignLink)
        };
    }

    if metadata.is_dir() {
        let marker = target.join(OWNERSHIP_MARKER);
        let managed = fs::read_to_string(&marker)
            .map(|text| text == OWNERSHIP_MARKER_CONTENT)
            .unwrap_or(false);
        if managed {
            let current = fs::read_to_string(target.join(SKILL_FILE)).unwrap_or_default();
            return if current == BUNDLED_SKILL {
                Ok(ExistingTarget::CopiedCurrent)
            } else {
                Ok(ExistingTarget::CopiedStale)
            };
        }
    }

    Ok(ExistingTarget::Foreign)
}

fn target_state(target: &Path, managed_source: &Path) -> Result<&'static str> {
    Ok(match inspect_target(target, managed_source)? {
        ExistingTarget::Absent => "absent",
        ExistingTarget::LinkedCurrent => "linked",
        ExistingTarget::ForeignLink => "foreign-link",
        ExistingTarget::CopiedCurrent => "copied",
        ExistingTarget::CopiedStale => "stale",
        ExistingTarget::Foreign => "foreign",
    })
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

#[cfg(unix)]
fn symlink_dir(source: &Path, target: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}

#[cfg(windows)]
fn symlink_dir(source: &Path, target: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(source, target)
}

impl Paths {
    fn resolve() -> Result<Self> {
        let home = home_dir()?;
        let data_dir = data_dir(&home);
        let grok_home = env_path("GROK_HOME").unwrap_or_else(|| home.join(".grok"));
        Ok(Self {
            home,
            data_dir,
            grok_home,
        })
    }
}

fn home_dir() -> Result<PathBuf> {
    env_path("HOME")
        .or_else(|| env_path("USERPROFILE"))
        .context("HOME is not set")
}

fn data_dir(home: &Path) -> PathBuf {
    if let Some(path) = env_path("NIT_DATA_DIR") {
        return path;
    }
    if let Some(path) = env_path("XDG_DATA_HOME") {
        return path.join(SKILL_NAME);
    }
    home.join(".local").join("share").join(SKILL_NAME)
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

impl Harness {
    const ALL: [Harness; 4] = [
        Harness::ClaudeCode,
        Harness::Codex,
        Harness::OpenCode,
        Harness::Grok,
    ];
    const SUPPORTED: [&'static str; 6] = [
        "claude",
        "claude-code",
        "codex",
        "opencode",
        "grok",
        "grok-build",
    ];

    fn parse(value: &str) -> Result<Self> {
        match value {
            "claude" | "claude-code" => Ok(Self::ClaudeCode),
            "codex" => Ok(Self::Codex),
            "opencode" => Ok(Self::OpenCode),
            "grok" | "grok-build" => Ok(Self::Grok),
            other => bail!(
                "unknown harness {other}; supported: {}",
                Self::SUPPORTED.join(", ")
            ),
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
            Self::Grok => "grok",
        }
    }

    fn base_dir(self, paths: &Paths) -> PathBuf {
        match self {
            Self::ClaudeCode => paths.home.join(".claude"),
            Self::Codex => paths.home.join(".codex"),
            Self::OpenCode => paths.home.join(".opencode"),
            Self::Grok => paths.grok_home.clone(),
        }
    }

    fn target_dir(self, paths: &Paths) -> PathBuf {
        self.base_dir(paths).join("skills").join(SKILL_NAME)
    }
}
