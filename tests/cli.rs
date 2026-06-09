use assert_cmd::Command;
use fs2::FileExt;
use predicates::prelude::*;
use serde_json::json;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
}

struct RemoteFixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    root_remote: PathBuf,
    sdk_remote: PathBuf,
}

struct ThreeMemberRemoteFixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    root_remote: PathBuf,
    sdk_remote: PathBuf,
    app_remote: PathBuf,
    docs_remote: PathBuf,
}

#[test]
fn help_describes_nit() {
    let mut cmd = Command::cargo_bin("gnit").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Git-native multi-repo workspaces"));
}

#[test]
fn doctor_runs() {
    let mut cmd = Command::cargo_bin("gnit").unwrap();
    cmd.arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("Gnit doctor"))
        .stdout(predicate::str::contains(
            "upkeep: automatic non-destructive upkeep enabled",
        ));
}

#[test]
fn status_outside_workspace_is_clear() {
    let temp = tempdir_without_gnit_ancestor();
    let mut cmd = Command::cargo_bin("gnit").unwrap();
    cmd.current_dir(temp.path())
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("No Gnit workspace found."));
}

#[test]
fn init_and_adopt_nested_repo_workflow_preserves_root_staging() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path();
    git_init(workspace);
    git(workspace, ["config", "user.email", "gnit-test@example.com"]);
    git(workspace, ["config", "user.name", "Gnit Test"]);

    std::fs::write(workspace.join("README.md"), "root\n").unwrap();
    git(workspace, ["add", "README.md"]);
    git(workspace, ["commit", "-m", "Initial root"]);

    std::fs::create_dir_all(workspace.join("vendor/sdk")).unwrap();
    git_init(&workspace.join("vendor/sdk"));
    git(
        &workspace.join("vendor/sdk"),
        ["config", "user.email", "gnit-test@example.com"],
    );
    git(
        &workspace.join("vendor/sdk"),
        ["config", "user.name", "Gnit Test"],
    );
    std::fs::write(workspace.join("vendor/sdk/lib.rs"), "pub fn sdk() {}\n").unwrap();
    git(&workspace.join("vendor/sdk"), ["add", "lib.rs"]);
    git(
        &workspace.join("vendor/sdk"),
        ["commit", "-m", "Initial sdk"],
    );

    std::fs::write(workspace.join("UNRELATED.txt"), "keep me staged\n").unwrap();
    git(workspace, ["add", "UNRELATED.txt"]);

    gnit(workspace, ["init"])
        .success()
        .stdout(predicate::str::contains("initialized Gnit workspace"));

    gnit(workspace, ["adopt", "vendor/sdk", "--id", "sdk"])
        .success()
        .stdout(predicate::str::contains("adopted sdk"));

    let roster = std::fs::read_to_string(workspace.join(".gnit/roster.yaml")).unwrap();
    assert!(roster.contains("id: sdk"));
    assert!(roster.contains("path: vendor/sdk"));
    assert!(roster.contains("required_excludes"));
    assert!(roster.contains("vendor/sdk"));

    let exclude = std::fs::read_to_string(workspace.join(".git/info/exclude")).unwrap();
    assert!(exclude.lines().any(|line| line == "vendor/sdk"));

    let root_status = git_out(workspace, ["status", "--porcelain"]);
    assert!(
        root_status.lines().any(|line| line == "A  UNRELATED.txt"),
        "unrelated staged change should remain staged: {root_status}"
    );
    assert!(
        !root_status.lines().any(|line| line.contains(".gnit/")),
        "Gnit metadata should have been committed: {root_status}"
    );
    assert!(
        !root_status.lines().any(|line| line.contains("AGENTS.md")),
        "Gnit guidance should have been committed: {root_status}"
    );

    let last_commit = git_out(workspace, ["log", "-1", "--pretty=%s"]);
    assert_eq!(last_commit.trim(), "Update Gnit roster");

    gnit(workspace, ["status"])
        .success()
        .stdout(predicate::str::contains("Repos"))
        .stdout(predicate::str::contains("sdk"))
        .stdout(predicate::str::contains("clean"));

    gnit(workspace, ["doctor"])
        .success()
        .stdout(predicate::str::contains("roster members: 1"));

    gnit(workspace, ["pin", "baseline"])
        .failure()
        .stderr(predicate::str::contains(
            "workspace root has uncommitted changes",
        ));

    git(workspace, ["commit", "-m", "Keep unrelated file"]);

    let sdk_head = git_out(&workspace.join("vendor/sdk"), ["rev-parse", "HEAD"]);
    gnit(workspace, ["pin", "baseline"])
        .success()
        .stdout(predicate::str::contains("created Pin PIN-"));

    let pin_paths = std::fs::read_dir(workspace.join(".gnit/pins"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(pin_paths.len(), 1);
    let pin = std::fs::read_to_string(&pin_paths[0]).unwrap();
    assert!(pin.contains("label: baseline"));
    assert!(pin.contains("id: sdk"));
    assert!(pin.contains("path: vendor/sdk"));
    assert!(pin.contains(sdk_head.trim()));

    let last_commit = git_out(workspace, ["log", "-1", "--pretty=%s"]);
    assert!(last_commit.starts_with("Create Gnit pin PIN-"));
}

#[test]
fn init_creates_and_commits_agent_guidance() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path();
    git_init(workspace);
    git(workspace, ["config", "user.email", "gnit-test@example.com"]);
    git(workspace, ["config", "user.name", "Gnit Test"]);
    fs::write(workspace.join("README.md"), "root\n").unwrap();
    git(workspace, ["add", "README.md"]);
    git(workspace, ["commit", "-m", "Initial root"]);

    gnit(workspace, ["init"])
        .success()
        .stdout(predicate::str::contains("initialized Gnit workspace"));

    let agents = fs::read_to_string(workspace.join("AGENTS.md")).unwrap();
    assert_gnit_guidance(&agents);
    assert!(
        !workspace.join("CLAUDE.md").exists(),
        "CLAUDE.md should not be created unless already present"
    );
    let status = git_out(
        workspace,
        [
            "status",
            "--porcelain",
            "--",
            ".gnit",
            "AGENTS.md",
            "CLAUDE.md",
        ],
    );
    assert!(
        status.trim().is_empty(),
        "init should commit Gnit metadata and guidance docs: {status}"
    );
    let last_commit = git_out(workspace, ["log", "-1", "--pretty=%s"]);
    assert_eq!(last_commit.trim(), "Initialize Gnit workspace");
}

#[test]
fn control_init_creates_repo_and_commits_agent_guidance() {
    let temp = tempdir_without_gnit_ancestor();
    let workspace = temp.path();

    gnit(workspace, ["init", "--control"])
        .success()
        .stdout(predicate::str::contains("initialized Gnit workspace"));

    assert!(workspace.join(".git").exists());
    let agents = fs::read_to_string(workspace.join("AGENTS.md")).unwrap();
    assert_gnit_guidance(&agents);
    let status = git_out(
        workspace,
        ["status", "--porcelain", "--", ".gnit", "AGENTS.md"],
    );
    assert!(
        status.trim().is_empty(),
        "control init should commit metadata and guidance docs: {status}"
    );
    let last_commit = git_out(workspace, ["log", "-1", "--pretty=%s"]);
    assert_eq!(last_commit.trim(), "Initialize Gnit workspace");
}

#[test]
fn init_updates_existing_agent_docs_without_rewriting_marked_block() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path();
    git_init(workspace);
    git(workspace, ["config", "user.email", "gnit-test@example.com"]);
    git(workspace, ["config", "user.name", "Gnit Test"]);
    let custom_agents =
        "<!-- gnit:workspace:start -->\nCustom Gnit note.\n<!-- gnit:workspace:end -->\n";
    fs::write(workspace.join("AGENTS.md"), custom_agents).unwrap();
    fs::write(workspace.join("CLAUDE.md"), "# Claude notes\n").unwrap();
    git(workspace, ["add", "AGENTS.md", "CLAUDE.md"]);
    git(workspace, ["commit", "-m", "Add agent docs"]);

    gnit(workspace, ["init"]).success();

    let agents = fs::read_to_string(workspace.join("AGENTS.md")).unwrap();
    assert_eq!(agents, custom_agents);
    let claude = fs::read_to_string(workspace.join("CLAUDE.md")).unwrap();
    assert!(claude.starts_with("# Claude notes\n\n"), "{claude}");
    assert_gnit_guidance(&claude);
    let status = git_out(
        workspace,
        [
            "status",
            "--porcelain",
            "--",
            ".gnit",
            "AGENTS.md",
            "CLAUDE.md",
        ],
    );
    assert!(
        status.trim().is_empty(),
        "init should commit changed Gnit guidance docs: {status}"
    );
}

#[test]
fn doctor_repairs_agent_guidance_without_duplicates() {
    let fixture = clean_workspace_with_sdk();
    let workspace = fixture.root.as_path();
    fs::write(workspace.join("AGENTS.md"), "Team notes\n").unwrap();

    gnit(workspace, ["doctor"])
        .success()
        .stdout(predicate::str::contains("agent guidance: added"));
    let agents = fs::read_to_string(workspace.join("AGENTS.md")).unwrap();
    assert!(agents.contains("Team notes"), "{agents}");
    assert_gnit_guidance(&agents);

    gnit(workspace, ["doctor"])
        .success()
        .stdout(predicate::str::contains("agent guidance: ok"));
    let agents = fs::read_to_string(workspace.join("AGENTS.md")).unwrap();
    assert_eq!(gnit_guidance_count(&agents), 1, "{agents}");
}

#[test]
fn local_init_writes_agent_guidance_without_commit() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path();
    git_init(workspace);
    git(workspace, ["config", "user.email", "gnit-test@example.com"]);
    git(workspace, ["config", "user.name", "Gnit Test"]);
    fs::write(workspace.join("README.md"), "root\n").unwrap();
    git(workspace, ["add", "README.md"]);
    git(workspace, ["commit", "-m", "Initial root"]);

    gnit(workspace, ["init", "--local"]).success();

    let agents = fs::read_to_string(workspace.join("AGENTS.md")).unwrap();
    assert_gnit_guidance(&agents);
    let status = git_out(workspace, ["status", "--porcelain"]);
    assert!(
        status.lines().any(|line| line == "?? .gnit/"),
        "local init should leave metadata uncommitted: {status}"
    );
    assert!(
        status.lines().any(|line| line == "?? AGENTS.md"),
        "local init should leave guidance uncommitted: {status}"
    );
    let last_commit = git_out(workspace, ["log", "-1", "--pretty=%s"]);
    assert_eq!(last_commit.trim(), "Initial root");
}

#[test]
fn update_dry_run_shows_installer() {
    let mut cmd = Command::cargo_bin("gnit").unwrap();
    cmd.args(["update", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mostlydev/gnit"))
        .stdout(predicate::str::contains("install.sh"));
}

#[cfg(unix)]
#[test]
fn skills_install_link_writes_symlink_to_managed_source() {
    let env = skill_env();
    fs::create_dir_all(env.home.join(".codex")).unwrap();

    env.command(["skills", "install", "codex"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[codex] added: linked"));

    let managed = env.managed_skill();
    let target = env.codex_skill();
    assert!(managed.join("SKILL.md").exists());
    assert!(fs::read_to_string(managed.join("SKILL.md"))
        .unwrap()
        .contains("# Driving Gnit"));
    assert!(fs::symlink_metadata(&target)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read_link(&target).unwrap(), managed);

    env.command(["skills", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("codex"))
        .stdout(predicate::str::contains("linked"));
}

#[test]
fn skills_install_copy_writes_real_skill_and_uninstall_keeps_managed_source() {
    let env = skill_env();
    fs::create_dir_all(env.home.join(".claude")).unwrap();

    env.command(["skills", "install", "claude", "--copy"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[claude-code] added: copied"));

    let managed = env.managed_skill();
    let target = env.claude_skill();
    assert!(managed.join("SKILL.md").exists());
    assert!(target.join("SKILL.md").exists());
    assert!(target.join(".gnit-skill-managed").exists());
    assert!(!fs::symlink_metadata(&target)
        .unwrap()
        .file_type()
        .is_symlink());

    env.command(["skills", "uninstall", "claude-code"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[claude-code] removed"));

    assert!(!target.exists());
    assert!(managed.join("SKILL.md").exists());
}

#[test]
fn skills_install_print_is_noop() {
    let env = skill_env();

    env.command(["skills", "install", "codex", "--print"])
        .assert()
        .success()
        .stdout(predicate::str::contains("refresh managed source"))
        .stdout(predicate::str::contains("create harness directory"))
        .stdout(predicate::str::contains("[codex] link"));

    assert!(!env.managed_skill().exists());
    assert!(!env.codex_skill().exists());
}

#[cfg(unix)]
#[test]
fn skills_install_all_targets_only_detected_harnesses() {
    let env = skill_env();
    fs::create_dir_all(env.home.join(".codex")).unwrap();
    fs::create_dir_all(&env.grok_home).unwrap();

    env.command(["skills", "install", "--all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[codex] added: linked"))
        .stdout(predicate::str::contains("[grok] added: linked"))
        .stdout(predicate::str::contains("[claude-code] skipped"))
        .stdout(predicate::str::contains("[opencode] skipped"));

    assert!(fs::symlink_metadata(env.codex_skill())
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(fs::symlink_metadata(env.grok_skill())
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(!env.claude_skill().exists());
    assert!(!env.opencode_skill().exists());
}

#[cfg(unix)]
#[test]
fn skills_install_explicit_missing_root_creates_and_notes_it() {
    let env = skill_env();

    env.command(["skills", "install", "opencode"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[opencode] added: linked"))
        .stdout(predicate::str::contains("created harness directory"));

    assert!(env.home.join(".opencode").exists());
    assert!(fs::symlink_metadata(env.opencode_skill())
        .unwrap()
        .file_type()
        .is_symlink());
}

#[test]
fn skills_install_copy_refreshes_stale_managed_copy() {
    let env = skill_env();
    fs::create_dir_all(env.home.join(".codex")).unwrap();

    env.command(["skills", "install", "codex", "--copy"])
        .assert()
        .success();
    fs::write(env.codex_skill().join("SKILL.md"), "stale\n").unwrap();

    env.command(["skills", "install", "codex", "--copy"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[codex] updated: copied"));

    assert!(fs::read_to_string(env.codex_skill().join("SKILL.md"))
        .unwrap()
        .contains("# Driving Gnit"));
}

#[cfg(unix)]
#[test]
fn skills_install_link_refreshes_managed_source_on_reinstall() {
    let env = skill_env();
    fs::create_dir_all(env.home.join(".codex")).unwrap();

    env.command(["skills", "install", "codex"])
        .assert()
        .success();
    fs::write(env.managed_skill().join("SKILL.md"), "stale\n").unwrap();

    env.command(["skills", "install", "codex"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[codex] already-present: linked"));

    assert!(fs::read_to_string(env.managed_skill().join("SKILL.md"))
        .unwrap()
        .contains("# Driving Gnit"));
}

#[test]
fn skills_install_propagates_managed_source_read_errors() {
    let env = skill_env();
    fs::create_dir_all(env.home.join(".codex")).unwrap();
    fs::create_dir_all(env.managed_skill().join("SKILL.md")).unwrap();

    env.command(["skills", "install", "codex", "--copy"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("read"))
        .stderr(predicate::str::contains("SKILL.md"));

    assert!(env.managed_skill().join("SKILL.md").is_dir());
    assert!(!env.codex_skill().exists());
}

#[test]
fn skills_install_copy_rejects_unreadable_managed_target_skill() {
    let env = skill_env();
    fs::create_dir_all(env.home.join(".codex")).unwrap();

    env.command(["skills", "install", "codex", "--copy"])
        .assert()
        .success();
    let invalid = b"not utf8: \xff\n";
    fs::write(env.codex_skill().join("SKILL.md"), invalid).unwrap();

    env.command(["skills", "install", "codex", "--copy"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("read"))
        .stderr(predicate::str::contains("SKILL.md"));

    assert_eq!(
        fs::read(env.codex_skill().join("SKILL.md")).unwrap(),
        invalid
    );
    assert!(env.codex_skill().join(".gnit-skill-managed").exists());
}

#[test]
fn skills_install_force_rejects_unreadable_managed_target_marker() {
    let env = skill_env();
    fs::create_dir_all(env.home.join(".codex")).unwrap();

    env.command(["skills", "install", "codex", "--copy"])
        .assert()
        .success();
    let invalid = b"not utf8: \xff\n";
    fs::write(env.codex_skill().join(".gnit-skill-managed"), invalid).unwrap();

    env.command(["skills", "install", "codex", "--copy", "--force"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("read"))
        .stderr(predicate::str::contains(".gnit-skill-managed"));

    assert_eq!(
        fs::read(env.codex_skill().join(".gnit-skill-managed")).unwrap(),
        invalid
    );
    assert!(env.codex_skill().join("SKILL.md").exists());
}

#[test]
fn skills_install_copy_self_heals_missing_managed_target_skill() {
    let env = skill_env();
    fs::create_dir_all(env.home.join(".codex")).unwrap();

    env.command(["skills", "install", "codex", "--copy"])
        .assert()
        .success();
    fs::remove_file(env.codex_skill().join("SKILL.md")).unwrap();

    env.command(["skills", "install", "codex", "--copy"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[codex] updated: copied"));

    assert!(fs::read_to_string(env.codex_skill().join("SKILL.md"))
        .unwrap()
        .contains("# Driving Gnit"));
}

#[cfg(unix)]
#[test]
fn skills_install_blocks_foreign_target_without_force() {
    let env = skill_env();
    let target = env.codex_skill();
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("SKILL.md"), "custom\n").unwrap();

    let assert = env
        .command(["skills", "install", "codex"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "skills install completed with failures",
        ));
    assert!(String::from_utf8_lossy(&assert.get_output().stdout).contains("[codex] failed"));
    assert_eq!(
        fs::read_to_string(target.join("SKILL.md")).unwrap(),
        "custom\n"
    );

    env.command(["skills", "install", "codex", "--force"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[codex] updated: linked"));
    assert!(fs::symlink_metadata(env.codex_skill())
        .unwrap()
        .file_type()
        .is_symlink());
}

#[cfg(unix)]
#[test]
fn skills_install_aliases_dedupe_harnesses() {
    let env = skill_env();
    fs::create_dir_all(env.home.join(".claude")).unwrap();

    let assert = env
        .command(["skills", "install", "claude", "claude-code"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert_eq!(stdout.matches("[claude-code]").count(), 1, "{stdout}");
    assert!(fs::symlink_metadata(env.claude_skill())
        .unwrap()
        .file_type()
        .is_symlink());
}

#[test]
fn update_check_caches_latest_release_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let latest = temp.path().join("latest.json");
    let cache = temp.path().join("update-cache");
    std::fs::write(
        &latest,
        r#"{"tag_name":"v9.9.9","html_url":"https://example.test/releases/v9.9.9"}"#,
    )
    .unwrap();

    Command::cargo_bin("gnit")
        .unwrap()
        .args(["update", "--check"])
        .env(
            "GNIT_UPDATE_CHECK_URL",
            format!("file://{}", latest.display()),
        )
        .env("GNIT_UPDATE_CACHE_PATH", &cache)
        .assert()
        .success()
        .stdout(predicate::str::contains("gnit 9.9.9 is available"));

    let text = std::fs::read_to_string(&cache).unwrap();
    assert!(text.contains("latest_tag=v9.9.9"), "{text}");
    assert!(text.contains("latest_version=9.9.9"), "{text}");
    assert!(text.contains("checked_at="), "{text}");
}

#[test]
fn update_check_failure_is_best_effort() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("update-cache");
    let missing = temp.path().join("missing.json");

    Command::cargo_bin("gnit")
        .unwrap()
        .args(["update", "--check"])
        .env(
            "GNIT_UPDATE_CHECK_URL",
            format!("file://{}", missing.display()),
        )
        .env("GNIT_UPDATE_CHECK_TIMEOUT_SECS", "1")
        .env("GNIT_UPDATE_CACHE_PATH", &cache)
        .assert()
        .success()
        .stderr(predicate::str::contains("gnit update check unavailable"));

    assert!(!cache.exists(), "failed check should not write cache");
}

#[test]
fn commit_change_and_land_workflow_records_shared_history() {
    let fixture = clean_workspace_with_sdk();
    let workspace = fixture.root.as_path();

    std::fs::write(workspace.join("README.md"), "root v2\n").unwrap();
    std::fs::write(
        workspace.join("vendor/sdk/lib.rs"),
        "pub fn sdk() -> &'static str { \"v2\" }\n",
    )
    .unwrap();

    gnit(workspace, ["add", "README.md", "vendor/sdk/lib.rs"]);
    let commit = gnit(workspace, ["commit", "-m", "Update root and sdk"]);
    let change_id = parse_created_change(&commit);

    let root_commit = git_out(workspace, ["log", "-1", "--pretty=%B"]);
    let sdk_commit = git_out(&workspace.join("vendor/sdk"), ["log", "-1", "--pretty=%B"]);
    assert!(root_commit.contains(&format!("Gnit-Change-Id: {change_id}")));
    assert!(sdk_commit.contains(&format!("Gnit-Change-Id: {change_id}")));

    gnit(workspace, ["change", "status", &change_id])
        .success()
        .stdout(predicate::str::contains("root:"))
        .stdout(predicate::str::contains("sdk:"));
    gnit(workspace, ["change", "show", &change_id])
        .success()
        .stdout(predicate::str::contains("Update root and sdk"));
    gnit(workspace, ["change", "log"])
        .success()
        .stdout(predicate::str::contains(&change_id));

    std::fs::write(
        workspace.join("vendor/sdk/lib.rs"),
        "pub fn sdk() -> &'static str { \"ambiguous\" }\n",
    )
    .unwrap();
    git(&workspace.join("vendor/sdk"), ["add", "lib.rs"]);
    let duplicate_change_message = format!("Manual follow-up\n\nGnit-Change-Id: {change_id}");
    git(
        &workspace.join("vendor/sdk"),
        ["commit", "-m", &duplicate_change_message],
    );
    gnit(workspace, ["change", "status", &change_id])
        .success()
        .stdout(predicate::str::contains("sdk: ambiguous (2 commits)"));

    std::fs::write(
        workspace.join("vendor/sdk/lib.rs"),
        "pub fn sdk() -> &'static str { \"landed\" }\n",
    )
    .unwrap();
    gnit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    let land = gnit(workspace, ["land", "release", "-m", "Land sdk update"]);
    let landed_change = parse_created_change(&land);

    gnit(workspace, ["review", &landed_change])
        .success()
        .stdout(predicate::str::contains("Change"))
        .stdout(predicate::str::contains("Land sdk update"));
    gnit(workspace, ["review", "release"])
        .success()
        .stdout(predicate::str::contains("Review Pin"))
        .stdout(predicate::str::contains("Changes:"))
        .stdout(predicate::str::contains(&landed_change));

    let pin_paths = std::fs::read_dir(workspace.join(".gnit/pins"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(pin_paths.len(), 1);
    let pin = std::fs::read_to_string(&pin_paths[0]).unwrap();
    assert!(pin.contains("label: release"));
    assert!(pin.contains(&landed_change));

    gnit(
        workspace,
        ["pin", "release-copy", "--change", &landed_change],
    )
    .success()
    .stdout(predicate::str::contains("created Pin PIN-"));
}

#[test]
fn review_pin_missing_local_commit_prints_actionable_remediation_without_fetching() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();
    let sdk = workspace.join("vendor/sdk");

    advance_remote(
        fixture._temp.path(),
        &fixture.sdk_remote,
        "sdk-review-remote",
        "lib.rs",
        "pub fn sdk() -> &'static str { \"remote only\" }\n",
    );
    let remote_commit = git_dir_out(&fixture.sdk_remote, ["rev-parse", "master"])
        .trim()
        .to_string();
    assert_commit_absent(&sdk, &remote_commit);

    let pin_id = "PIN-review-remote-only";
    fs::create_dir_all(workspace.join(".gnit/pins")).unwrap();
    fs::write(
        workspace.join(format!(".gnit/pins/{pin_id}.yaml")),
        format!(
            "version: 1\nid: {pin_id}\nlabel: review-remote-only\nmembers:\n- id: sdk\n  path: vendor/sdk\n  commit: {remote_commit}\n  branch_hint: master\nprovenance:\n  changes: []\n"
        ),
    )
    .unwrap();

    gnit(workspace, ["review", "review-remote-only"])
        .success()
        .stdout(predicate::str::contains("== sdk (vendor/sdk) =="))
        .stdout(predicate::str::contains(format!("commit {remote_commit}")))
        .stdout(predicate::str::contains("commit not available locally"))
        .stdout(predicate::str::contains(
            "gnit checkout PIN-review-remote-only",
        ))
        .stdout(predicate::str::contains("git -C "))
        .stdout(predicate::str::contains("vendor/sdk fetch origin"));

    assert_commit_absent(&sdk, &remote_commit);
}

#[test]
fn review_fetch_retrieves_missing_member_commit_before_rendering() {
    let fixture = workspace_with_three_member_remotes();
    let workspace = fixture.root.as_path();
    let sdk = workspace.join("sdk");
    let app = workspace.join("app");

    advance_remote(
        fixture._temp.path(),
        &fixture.sdk_remote,
        "sdk-review-fetch",
        "sdk.txt",
        "sdk v2\n",
    );
    let sdk_remote_commit = git_dir_out(&fixture.sdk_remote, ["rev-parse", "master"])
        .trim()
        .to_string();
    let app_commit = git_out(&app, ["rev-parse", "HEAD"]).trim().to_string();
    assert_commit_absent(&sdk, &sdk_remote_commit);

    git(
        &app,
        [
            "remote",
            "set-url",
            "origin",
            "/definitely/missing/review-fetch-app.git",
        ],
    );

    let pin_id = "PIN-review-fetch";
    fs::create_dir_all(workspace.join(".gnit/pins")).unwrap();
    fs::write(
        workspace.join(format!(".gnit/pins/{pin_id}.yaml")),
        format!(
            "version: 1\nid: {pin_id}\nlabel: review-fetch\nmembers:\n- id: sdk\n  path: sdk\n  commit: {sdk_remote_commit}\n  branch_hint: master\n- id: app\n  path: app\n  commit: {app_commit}\n  branch_hint: master\nprovenance:\n  changes: []\n"
        ),
    )
    .unwrap();

    gnit(workspace, ["review", "--fetch", "review-fetch"])
        .success()
        .stdout(predicate::str::contains("== sdk (sdk) =="))
        .stdout(predicate::str::contains(format!(
            "commit {sdk_remote_commit}"
        )))
        .stdout(predicate::str::contains("Advance remote"))
        .stdout(predicate::str::contains("== app (app) =="))
        .stdout(predicate::str::contains(format!("commit {app_commit}")))
        .stdout(predicate::str::contains("commit not available locally").not());

    assert_commit_present(&sdk, &sdk_remote_commit);
}

#[test]
fn review_fetch_keeps_clear_remediation_when_commit_stays_unavailable() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();
    let sdk = workspace.join("vendor/sdk");
    let missing_commit = "0123456789012345678901234567890123456789";
    assert_commit_absent(&sdk, missing_commit);

    let pin_id = "PIN-review-fetch-missing";
    fs::create_dir_all(workspace.join(".gnit/pins")).unwrap();
    fs::write(
        workspace.join(format!(".gnit/pins/{pin_id}.yaml")),
        format!(
            "version: 1\nid: {pin_id}\nlabel: review-fetch-missing\nmembers:\n- id: sdk\n  path: vendor/sdk\n  commit: {missing_commit}\n  branch_hint: master\nprovenance:\n  changes: []\n"
        ),
    )
    .unwrap();

    gnit(workspace, ["review", "--fetch", "review-fetch-missing"])
        .success()
        .stdout(predicate::str::contains("== sdk (vendor/sdk) =="))
        .stdout(predicate::str::contains(format!("commit {missing_commit}")))
        .stdout(predicate::str::contains("commit not available locally"))
        .stdout(predicate::str::contains("git -C "))
        .stdout(predicate::str::contains("vendor/sdk fetch origin"));

    assert_commit_absent(&sdk, missing_commit);
}

#[test]
fn review_fetch_is_rejected_for_change_targets() {
    let fixture = clean_workspace_with_sdk();
    let workspace = fixture.root.as_path();

    gnit(workspace, ["review", "--fetch", "GCH-1760000000000-72e5"])
        .failure()
        .stderr(predicate::str::contains(
            "--fetch is only supported when reviewing a Pin",
        ));
}

#[test]
fn change_discovery_uses_and_invalidates_ref_keyed_cache() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    let sdk = ws.join("vendor/sdk");

    fs::write(sdk.join("lib.rs"), "pub fn sdk() { /* cached */ }\n").unwrap();
    gnit(ws, ["add", "vendor/sdk/lib.rs"]);
    let commit = gnit(ws, ["commit", "-m", "Cached change"]).success();
    let change_id = parse_created_change(&commit);

    let warm = gnit_output(ws, ["change", "log"]);
    assert!(warm.contains(&change_id), "{warm}");
    let cache_path = ws.join(".gnit/cache/changes-sdk.json");
    assert!(
        cache_path.exists(),
        "discovery should persist a per-member cache"
    );

    // Tampering with the cached id while the ref-state key still matches
    // proves the warm path reads the cache instead of rescanning.
    let bogus_id = "GCH-1760000000000-cafe";
    let tampered = fs::read_to_string(&cache_path)
        .unwrap()
        .replace(&change_id, bogus_id);
    fs::write(&cache_path, tampered).unwrap();
    let cached = gnit_output(ws, ["change", "log"]);
    assert!(cached.contains(bogus_id), "{cached}");
    assert!(!cached.contains(&change_id), "{cached}");

    // Moving a ref invalidates the key; discovery rescans and the tampered
    // entry disappears.
    fs::write(sdk.join("lib.rs"), "pub fn sdk() { /* moved */ }\n").unwrap();
    gnit(ws, ["add", "vendor/sdk/lib.rs"]);
    let second = gnit(ws, ["commit", "-m", "Move refs"]).success();
    let second_id = parse_created_change(&second);
    let rescanned = gnit_output(ws, ["change", "log"]);
    assert!(rescanned.contains(&change_id), "{rescanned}");
    assert!(rescanned.contains(&second_id), "{rescanned}");
    assert!(!rescanned.contains(bogus_id), "{rescanned}");
}

#[test]
fn corrupt_discovery_cache_falls_back_to_scan() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    let sdk = ws.join("vendor/sdk");

    fs::write(sdk.join("lib.rs"), "pub fn sdk() { /* corrupt */ }\n").unwrap();
    gnit(ws, ["add", "vendor/sdk/lib.rs"]);
    let commit = gnit(ws, ["commit", "-m", "Survives corruption"]).success();
    let change_id = parse_created_change(&commit);

    let cache_path = ws.join(".gnit/cache/changes-sdk.json");
    gnit_output(ws, ["change", "log"]);
    fs::write(&cache_path, "{ not json").unwrap();

    let output = gnit_output(ws, ["change", "log"]);
    assert!(output.contains(&change_id), "{output}");
}

#[test]
fn discovery_cache_stays_local() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    gnit_output(ws, ["log"]);

    assert!(ws.join(".gnit/cache").exists());
    let tracked = git_out(ws, ["ls-files", ".gnit/cache"]);
    assert!(
        tracked.trim().is_empty(),
        ".gnit/cache should stay untracked: {tracked}"
    );
    let exclude = fs::read_to_string(ws.join(".git/info/exclude")).unwrap();
    assert!(
        exclude.lines().any(|line| line == ".gnit/cache/"),
        ".gnit/cache/ should be hidden by local excludes: {exclude}"
    );
    let status = gnit_output(ws, ["status"]);
    let root_line = status
        .lines()
        .find(|line| line.trim_start().starts_with("root"))
        .unwrap();
    assert!(
        root_line.contains("clean"),
        "cache must not dirty root status: {status}"
    );
}

#[test]
fn commit_respects_index_and_leaves_unstaged_tracked_changes() {
    let fixture = clean_workspace_with_sdk();
    let workspace = fixture.root.as_path();
    let sdk = workspace.join("vendor/sdk");

    fs::write(workspace.join("root-staged.txt"), "root staged v1\n").unwrap();
    fs::write(workspace.join("root-unstaged.txt"), "root unstaged v1\n").unwrap();
    git(workspace, ["add", "root-staged.txt", "root-unstaged.txt"]);
    git(workspace, ["commit", "-m", "Add root tracked files"]);

    fs::write(sdk.join("sdk-staged.txt"), "sdk staged v1\n").unwrap();
    fs::write(sdk.join("sdk-unstaged.txt"), "sdk unstaged v1\n").unwrap();
    git(&sdk, ["add", "sdk-staged.txt", "sdk-unstaged.txt"]);
    git(&sdk, ["commit", "-m", "Add sdk tracked files"]);

    fs::write(workspace.join("root-staged.txt"), "root staged v2\n").unwrap();
    fs::write(workspace.join("root-unstaged.txt"), "root unstaged v2\n").unwrap();
    fs::write(sdk.join("sdk-staged.txt"), "sdk staged v2\n").unwrap();
    fs::write(sdk.join("sdk-unstaged.txt"), "sdk unstaged v2\n").unwrap();

    gnit(
        workspace,
        ["add", "root-staged.txt", "vendor/sdk/sdk-staged.txt"],
    );
    gnit(workspace, ["commit", "-m", "Commit staged only"]).success();

    let root_files = git_out(workspace, ["show", "--name-only", "--format=", "HEAD"]);
    assert!(root_files.lines().any(|line| line == "root-staged.txt"));
    assert!(
        !root_files.lines().any(|line| line == "root-unstaged.txt"),
        "root commit swept unstaged tracked file:\n{root_files}"
    );

    let sdk_files = git_out(&sdk, ["show", "--name-only", "--format=", "HEAD"]);
    assert!(sdk_files.lines().any(|line| line == "sdk-staged.txt"));
    assert!(
        !sdk_files.lines().any(|line| line == "sdk-unstaged.txt"),
        "member commit swept unstaged tracked file:\n{sdk_files}"
    );

    let root_status = git_out(workspace, ["status", "--porcelain"]);
    assert!(
        root_status
            .lines()
            .any(|line| line == " M root-unstaged.txt"),
        "unstaged root change should remain dirty:\n{root_status}"
    );
    assert!(
        !root_status
            .lines()
            .any(|line| line.ends_with("root-staged.txt")),
        "staged root change should have been committed:\n{root_status}"
    );

    let sdk_status = git_out(&sdk, ["status", "--porcelain"]);
    assert!(
        sdk_status.lines().any(|line| line == " M sdk-unstaged.txt"),
        "unstaged member change should remain dirty:\n{sdk_status}"
    );
    assert!(
        !sdk_status
            .lines()
            .any(|line| line.ends_with("sdk-staged.txt")),
        "staged member change should have been committed:\n{sdk_status}"
    );
}

#[test]
fn commit_rejects_staged_workspace_metadata() {
    let fixture = clean_workspace_with_sdk();
    let workspace = fixture.root.as_path();
    let roster_path = workspace.join(".gnit/roster.yaml");
    let roster = fs::read_to_string(&roster_path).unwrap();
    fs::write(&roster_path, format!("{roster}\n# staged metadata\n")).unwrap();
    git(workspace, ["add", ".gnit/roster.yaml"]);

    gnit(workspace, ["commit", "-m", "Metadata should stay metadata"])
        .failure()
        .stderr(predicate::str::contains("workspace metadata is staged"));
}

#[test]
fn adopt_rejects_plain_subdirectory() {
    let fixture = clean_workspace_with_sdk();
    let workspace = fixture.root.as_path();
    // A plain subdirectory of the root repo is not its own repository; adopting
    // it must fail rather than register a non-repo path as a member.
    std::fs::create_dir_all(workspace.join("plainsub")).unwrap();
    gnit(workspace, ["adopt", "plainsub"])
        .failure()
        .stderr(predicate::str::contains("not a repository root"));
}

#[test]
fn ignore_and_doctor_repair_root_excludes() {
    let fixture = clean_workspace_with_sdk();
    let workspace = fixture.root.as_path();

    gnit(workspace, ["ignore", "scratch"])
        .success()
        .stdout(predicate::str::contains("updated ignored paths"));
    let roster = std::fs::read_to_string(workspace.join(".gnit/roster.yaml")).unwrap();
    assert!(roster.contains("ignored:"));
    assert!(roster.contains("scratch"));

    std::fs::write(workspace.join(".git/info/exclude"), "# reset by clone\n").unwrap();
    gnit(workspace, ["doctor"])
        .success()
        .stdout(predicate::str::contains("exclude repair: ok"));

    let exclude = std::fs::read_to_string(workspace.join(".git/info/exclude")).unwrap();
    assert!(exclude.lines().any(|line| line == "vendor/sdk"));
    assert!(exclude.lines().any(|line| line == "scratch"));
}

#[test]
fn invalid_utf8_root_exclude_is_not_overwritten_by_upkeep_or_explicit_repair() {
    let fixture = clean_workspace_with_sdk();
    let workspace = fixture.root.as_path();
    let exclude = workspace.join(".git/info/exclude");
    let original = b"# user excludes\n*.log\n\xff\n";
    fs::write(&exclude, original).unwrap();

    gnit(workspace, ["--verbose", "status"])
        .success()
        .stderr(predicate::str::contains(
            "gnit upkeep: skipped exclude repair",
        ));
    assert_eq!(fs::read(&exclude).unwrap(), original);

    gnit(workspace, ["doctor"])
        .failure()
        .stderr(predicate::str::contains("read git exclude"));
    assert_eq!(fs::read(&exclude).unwrap(), original);
}

#[test]
fn mutating_command_fails_when_workspace_lock_is_held() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    let lock = hold_workspace_lock(ws);

    gnit_command(ws, ["ignore", "scratch"])
        .env("GNIT_LOCK_TIMEOUT_MS", "1")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "another gnit process holds the workspace lock",
        ));

    drop(lock);
}

#[test]
fn read_only_command_skips_upkeep_when_workspace_lock_is_held() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    let exclude = ws.join(".git/info/exclude");
    let stripped = fs::read_to_string(&exclude)
        .unwrap()
        .lines()
        .filter(|line| *line != "vendor/sdk")
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&exclude, format!("{stripped}\n")).unwrap();
    let lock = hold_workspace_lock(ws);

    gnit_command(ws, ["--verbose", "status"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "gnit upkeep: skipped local maintenance",
        ));

    let exclude_text = fs::read_to_string(&exclude).unwrap();
    assert!(
        !exclude_text.lines().any(|line| line == "vendor/sdk"),
        "read-only command should not repair excludes while lock is held: {exclude_text}"
    );
    drop(lock);
}

#[test]
fn workspace_lock_file_stays_local() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    assert!(ws.join(".gnit/lock").exists());
    let tracked = git_out(ws, ["ls-files", ".gnit/lock"]);
    assert!(
        tracked.trim().is_empty(),
        ".gnit/lock should stay untracked: {tracked}"
    );
    let exclude = fs::read_to_string(ws.join(".git/info/exclude")).unwrap();
    assert!(
        exclude.lines().any(|line| line == ".gnit/lock"),
        ".gnit/lock should be hidden by local excludes: {exclude}"
    );
}

#[test]
fn push_and_checkout_workflow_reconstructs_missing_member() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();

    std::fs::write(
        workspace.join("vendor/sdk/lib.rs"),
        "pub fn sdk() -> &'static str { \"pushed\" }\n",
    )
    .unwrap();
    gnit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    gnit(workspace, ["land", "baseline", "-m", "Publish sdk update"]).success();
    gnit(workspace, ["push"]).success();
    gnit(workspace, ["push", "--resume"])
        .success()
        .stdout(predicate::str::contains("member sdk"))
        .stdout(predicate::str::contains("workspace root"))
        .stdout(predicate::str::contains("already landed"));

    let sdk_head = git_out(&workspace.join("vendor/sdk"), ["rev-parse", "HEAD"]);
    let root_head = git_out(workspace, ["rev-parse", "HEAD"]);
    assert_eq!(
        git_dir_out(&fixture.sdk_remote, ["rev-parse", "master"]).trim(),
        sdk_head.trim()
    );
    assert_eq!(
        git_dir_out(&fixture.root_remote, ["rev-parse", "master"]).trim(),
        root_head.trim()
    );

    let root_remote = fixture.root_remote.to_str().unwrap();

    let hydrated = fixture._temp.path().join("hydrated");
    let hydrated_path = hydrated.to_str().unwrap();
    gnit(fixture._temp.path(), ["clone", root_remote, hydrated_path])
        .success()
        .stdout(predicate::str::contains("cloned sdk"))
        .stdout(predicate::str::contains("cloned Gnit workspace"));
    assert_eq!(
        std::fs::read_to_string(hydrated.join("vendor/sdk/lib.rs")).unwrap(),
        "pub fn sdk() -> &'static str { \"pushed\" }\n"
    );

    let restored = fixture._temp.path().join("restored");
    let restored_path = restored.to_str().unwrap();
    gnit(
        fixture._temp.path(),
        ["clone", root_remote, restored_path, "--pin", "baseline"],
    )
    .success()
    .stdout(predicate::str::contains("cloned sdk"))
    .stdout(predicate::str::contains("checked out Pin"));
    assert_eq!(
        std::fs::read_to_string(restored.join("vendor/sdk/lib.rs")).unwrap(),
        "pub fn sdk() -> &'static str { \"pushed\" }\n"
    );
    assert_eq!(
        git_out(
            &restored.join("vendor/sdk"),
            ["rev-parse", "--abbrev-ref", "HEAD"]
        )
        .trim(),
        "master"
    );

    std::fs::write(restored.join("vendor/sdk/lib.rs"), "dirty\n").unwrap();
    gnit(&restored, ["checkout", "baseline"])
        .failure()
        .stderr(predicate::str::contains("use --exact to reset it"));
    gnit(&restored, ["checkout", "baseline", "--exact"]).success();
    assert_eq!(
        std::fs::read_to_string(restored.join("vendor/sdk/lib.rs")).unwrap(),
        "pub fn sdk() -> &'static str { \"pushed\" }\n"
    );
}

#[test]
fn push_reports_partial_landing_and_resume_retries_in_order() {
    let fixture = workspace_with_three_member_remotes();
    let workspace = fixture.root.as_path();

    std::fs::write(workspace.join("sdk/sdk.txt"), "sdk v2\n").unwrap();
    std::fs::write(workspace.join("app/app.txt"), "app v2\n").unwrap();
    std::fs::write(workspace.join("docs/docs.txt"), "docs v2\n").unwrap();
    gnit(workspace, ["add", "-A"]);
    gnit(workspace, ["land", "release", "-m", "Publish v2"]).success();

    advance_remote(
        fixture._temp.path(),
        &fixture.app_remote,
        "app-remote-advance",
        "remote.txt",
        "remote app change\n",
    );

    gnit(workspace, ["push"])
        .failure()
        .stdout(predicate::str::contains("Push report:"))
        .stdout(predicate::str::contains("member sdk"))
        .stdout(predicate::str::contains("pushed"))
        .stdout(predicate::str::contains("member app"))
        .stdout(predicate::str::contains("failed: rejected"))
        .stdout(predicate::str::contains("member docs"))
        .stdout(predicate::str::contains("not attempted"))
        .stdout(predicate::str::contains("workspace root"))
        .stdout(predicate::str::contains("held back"))
        .stderr(predicate::str::contains("push incomplete"));

    assert_eq!(
        git_dir_out(&fixture.sdk_remote, ["rev-parse", "master"]),
        git_out(&workspace.join("sdk"), ["rev-parse", "HEAD"])
    );
    assert_ne!(
        git_dir_out(&fixture.docs_remote, ["rev-parse", "master"]),
        git_out(&workspace.join("docs"), ["rev-parse", "HEAD"])
    );
    assert_ne!(
        git_dir_out(&fixture.root_remote, ["rev-parse", "master"]),
        git_out(workspace, ["rev-parse", "HEAD"])
    );

    let app = workspace.join("app");
    git(&app, ["fetch", "origin", "master"]);
    git(&app, ["merge", "--no-edit", "FETCH_HEAD"]);

    gnit(workspace, ["push", "--resume"])
        .success()
        .stdout(predicate::str::contains(
            "resuming ordered push from remote state",
        ))
        .stdout(predicate::str::contains("member sdk"))
        .stdout(predicate::str::contains("already landed"))
        .stdout(predicate::str::contains("member app"))
        .stdout(predicate::str::contains("pushed"))
        .stdout(predicate::str::contains("member docs"))
        .stdout(predicate::str::contains("pushed"))
        .stdout(predicate::str::contains("workspace root"))
        .stdout(predicate::str::contains("pushed"))
        .stdout(predicate::str::contains("push complete"));

    assert_eq!(
        git_dir_out(&fixture.app_remote, ["rev-parse", "master"]),
        git_out(&workspace.join("app"), ["rev-parse", "HEAD"])
    );
    assert_eq!(
        git_dir_out(&fixture.docs_remote, ["rev-parse", "master"]),
        git_out(&workspace.join("docs"), ["rev-parse", "HEAD"])
    );
    assert_eq!(
        git_dir_out(&fixture.root_remote, ["rev-parse", "master"]),
        git_out(workspace, ["rev-parse", "HEAD"])
    );
}

#[test]
fn push_holds_root_when_pin_commit_was_rewritten_away() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();
    let sdk = workspace.join("vendor/sdk");

    std::fs::write(
        sdk.join("lib.rs"),
        "pub fn sdk() -> &'static str { \"pinned\" }\n",
    )
    .unwrap();
    gnit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    gnit(workspace, ["land", "baseline", "-m", "Publish sdk update"]).success();
    let pinned = git_out(&sdk, ["rev-parse", "HEAD"]);

    git(&sdk, ["reset", "--hard", "HEAD~1"]);
    std::fs::write(
        sdk.join("lib.rs"),
        "pub fn sdk() -> &'static str { \"rewritten\" }\n",
    )
    .unwrap();
    git(&sdk, ["add", "lib.rs"]);
    git(&sdk, ["commit", "-m", "Rewrite sdk update"]);

    gnit(workspace, ["push"])
        .failure()
        .stdout(predicate::str::contains("member sdk"))
        .stdout(predicate::str::contains("pushed"))
        .stdout(predicate::str::contains("workspace root"))
        .stdout(predicate::str::contains("held back"))
        .stdout(predicate::str::contains(
            "pin baseline references member sdk",
        ))
        .stdout(predicate::str::contains(&pinned.trim()[..12]))
        .stderr(predicate::str::contains("push incomplete"));

    assert_ne!(
        git_dir_out(&fixture.root_remote, ["rev-parse", "master"]),
        git_out(workspace, ["rev-parse", "HEAD"])
    );

    remove_pins_with_label(workspace, "baseline");
    git(workspace, ["add", "-A", ".gnit/pins"]);
    git(workspace, ["commit", "-m", "Remove orphaned pin"]);
    gnit(workspace, ["pin", "recovered"]).success();

    gnit(workspace, ["push", "--resume"])
        .success()
        .stdout(predicate::str::contains("member sdk"))
        .stdout(predicate::str::contains("already landed"))
        .stdout(predicate::str::contains("workspace root"))
        .stdout(predicate::str::contains("pushed"))
        .stdout(predicate::str::contains("push complete"));

    assert_eq!(
        git_dir_out(&fixture.root_remote, ["rev-parse", "master"]),
        git_out(workspace, ["rev-parse", "HEAD"])
    );
}

#[test]
fn push_allows_historical_pin_reachable_from_origin_side_branch() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();
    let sdk = workspace.join("vendor/sdk");

    git(&sdk, ["checkout", "-b", "feature/historical-pin"]);
    std::fs::write(
        sdk.join("lib.rs"),
        "pub fn sdk() -> &'static str { \"historical\" }\n",
    )
    .unwrap();
    gnit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    gnit(
        workspace,
        [
            "land",
            "historical-side-branch",
            "-m",
            "Publish historical sdk update",
        ],
    )
    .success();
    let historical = git_out(&sdk, ["rev-parse", "HEAD"]);
    git(&sdk, ["push", "-u", "origin", "HEAD"]);

    git(&sdk, ["checkout", "master"]);
    gnit(workspace, ["pin", "current-master"]).success();

    gnit(workspace, ["push"])
        .success()
        .stdout(predicate::str::contains("member sdk"))
        .stdout(predicate::str::contains("already landed"))
        .stdout(predicate::str::contains("workspace root"))
        .stdout(predicate::str::contains("pushed"))
        .stdout(predicate::str::contains("push complete"));

    assert_eq!(
        git_dir_out(&fixture.root_remote, ["rev-parse", "master"]),
        git_out(workspace, ["rev-parse", "HEAD"])
    );

    let restored = fixture._temp.path().join("restored-side-branch-pin");
    gnit(
        fixture._temp.path(),
        [
            "clone",
            fixture.root_remote.to_str().unwrap(),
            restored.to_str().unwrap(),
            "--pin",
            "historical-side-branch",
        ],
    )
    .success()
    .stdout(predicate::str::contains("checked out Pin"));
    assert_eq!(
        git_out(&restored.join("vendor/sdk"), ["rev-parse", "HEAD"]).trim(),
        historical.trim()
    );
    assert_eq!(
        std::fs::read_to_string(restored.join("vendor/sdk/lib.rs")).unwrap(),
        "pub fn sdk() -> &'static str { \"historical\" }\n"
    );
}

#[test]
fn push_holds_root_for_pin_reachable_only_from_local_branch() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();
    let sdk = workspace.join("vendor/sdk");

    git(&sdk, ["checkout", "-b", "local/historical-pin"]);
    std::fs::write(
        sdk.join("lib.rs"),
        "pub fn sdk() -> &'static str { \"local-only\" }\n",
    )
    .unwrap();
    gnit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    gnit(
        workspace,
        [
            "land",
            "local-only-side-branch",
            "-m",
            "Publish local-only sdk update",
        ],
    )
    .success();
    let pinned = git_out(&sdk, ["rev-parse", "HEAD"]);

    git(&sdk, ["checkout", "master"]);
    gnit(workspace, ["pin", "current-master"]).success();

    gnit(workspace, ["push"])
        .failure()
        .stdout(predicate::str::contains("member sdk"))
        .stdout(predicate::str::contains("already landed"))
        .stdout(predicate::str::contains("workspace root"))
        .stdout(predicate::str::contains("held back"))
        .stdout(predicate::str::contains(
            "pin local-only-side-branch references member sdk",
        ))
        .stdout(predicate::str::contains(
            "reachable only from local branch local/historical-pin",
        ))
        .stdout(predicate::str::contains(
            "git -C vendor/sdk push origin local/historical-pin",
        ))
        .stdout(predicate::str::contains(&pinned.trim()[..12]))
        .stderr(predicate::str::contains("push incomplete"));

    assert_ne!(
        git_dir_out(&fixture.root_remote, ["rev-parse", "master"]),
        git_out(workspace, ["rev-parse", "HEAD"])
    );
}

#[cfg(unix)]
#[test]
fn pr_open_creates_linked_draft_prs_and_rerun_does_not_duplicate() {
    let fixture = workspace_with_three_member_remotes();
    let workspace = fixture.root.as_path();
    prepare_pr_base_refs(workspace, ["sdk", "app", "docs"]);

    git(workspace, ["checkout", "-b", "feature/pr-flow"]);
    git(
        &workspace.join("sdk"),
        ["checkout", "-b", "feature/pr-flow"],
    );
    git(
        &workspace.join("app"),
        ["checkout", "-b", "feature/pr-flow"],
    );

    fs::write(workspace.join("README.md"), "root pr flow\n").unwrap();
    fs::write(workspace.join("sdk/sdk.txt"), "sdk pr flow\n").unwrap();
    fs::write(workspace.join("app/app.txt"), "app pr flow\n").unwrap();
    gnit(
        workspace,
        ["add", "README.md", "sdk/sdk.txt", "app/app.txt"],
    );
    let land = gnit(
        workspace,
        ["land", "review-pin", "-m", "Add linked PR flow"],
    );
    let change_id = parse_created_change(&land);
    gnit(workspace, ["push"]).success();

    let gh = fake_gh();
    gh.command(workspace, ["pr", "open"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Opening PRs for Change"))
        .stdout(predicate::str::contains("created"));

    let state = gh.state();
    let prs = state["prs"].as_array().unwrap();
    assert_eq!(prs.len(), 3, "{state}");
    assert!(prs.iter().any(|pr| pr["repo"] == "acme/root"));
    assert!(prs.iter().any(|pr| pr["repo"] == "acme/sdk"));
    assert!(prs.iter().any(|pr| pr["repo"] == "acme/app"));
    assert!(prs.iter().all(|pr| pr["draft"] == true));
    for pr in prs {
        let body = pr["body"].as_str().unwrap();
        assert!(body.contains(&format!("Gnit-Change-Id: {change_id}")));
        assert!(body.contains("acme/root#"));
        assert!(body.contains("acme/sdk#"));
        assert!(body.contains("acme/app#"));
    }

    gh.command(workspace, ["pr", "open"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already open"));
    assert_eq!(gh.state()["prs"].as_array().unwrap().len(), 3);

    gh.command(workspace, ["pr"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Workspace change"))
        .stdout(predicate::str::contains("#1"))
        .stdout(predicate::str::contains("open"));
}

#[cfg(unix)]
#[test]
fn pr_open_member_only_change_creates_only_member_pr() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();
    prepare_pr_base_refs(workspace, ["vendor/sdk"]);

    let sdk = workspace.join("vendor/sdk");
    git(&sdk, ["checkout", "-b", "feature/sdk-api"]);
    fs::write(
        sdk.join("lib.rs"),
        "pub fn sdk() -> &'static str { \"pr\" }\n",
    )
    .unwrap();
    gnit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    let commit = gnit(workspace, ["commit", "-m", "Update SDK API"]);
    let change_id = parse_created_change(&commit);
    gnit(workspace, ["push"]).success();

    let gh = fake_gh();
    gh.command(workspace, ["pr", "open"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&change_id))
        .stdout(predicate::str::contains("created"));

    let state = gh.state();
    let prs = state["prs"].as_array().unwrap();
    assert_eq!(prs.len(), 1, "{state}");
    assert_eq!(prs[0]["repo"], "acme/sdk");
    assert_eq!(prs[0]["head"], "feature/sdk-api");
}

#[cfg(unix)]
#[test]
fn pr_open_adopts_manual_pr_and_preserves_body_text() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();
    prepare_pr_base_refs(workspace, ["vendor/sdk"]);

    let sdk = workspace.join("vendor/sdk");
    git(&sdk, ["checkout", "-b", "feature/manual-pr"]);
    fs::write(
        sdk.join("lib.rs"),
        "pub fn sdk() -> &'static str { \"manual\" }\n",
    )
    .unwrap();
    gnit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    let commit = gnit(workspace, ["commit", "-m", "Update manual PR"]);
    let change_id = parse_created_change(&commit);
    gnit(workspace, ["push"]).success();

    let gh = fake_gh();
    gh.write_state(json!({
        "next": 2,
        "prs": [{
            "repo": "acme/sdk",
            "number": 1,
            "state": "OPEN",
            "url": "https://github.com/acme/sdk/pull/1",
            "title": "Manual PR",
            "head": "feature/manual-pr",
            "body": "Manual text stays.",
            "checks": []
        }]
    }));

    gh.command(workspace, ["pr", "open", "--change", &change_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("adopted"));

    let state = gh.state();
    let prs = state["prs"].as_array().unwrap();
    assert_eq!(prs.len(), 1, "{state}");
    let body = prs[0]["body"].as_str().unwrap();
    assert!(body.contains("Manual text stays."));
    assert!(body.contains(&format!("Gnit-Change-Id: {change_id}")));
}

#[cfg(unix)]
#[test]
fn pr_open_blocks_when_remote_branch_is_missing() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();
    prepare_pr_base_refs(workspace, ["vendor/sdk"]);

    let sdk = workspace.join("vendor/sdk");
    git(&sdk, ["checkout", "-b", "feature/unpushed"]);
    fs::write(
        sdk.join("lib.rs"),
        "pub fn sdk() -> &'static str { \"unpushed\" }\n",
    )
    .unwrap();
    gnit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    let commit = gnit(workspace, ["commit", "-m", "Update unpushed SDK"]);
    let change_id = parse_created_change(&commit);

    fake_gh()
        .command(workspace, ["pr", "open", "--change", &change_id])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "pr open blocked before creating PRs",
        ))
        .stderr(predicate::str::contains("gnit push"));
}

#[cfg(unix)]
#[test]
fn pr_open_blocks_ambiguous_duplicate_markers() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();
    prepare_pr_base_refs(workspace, ["vendor/sdk"]);

    let sdk = workspace.join("vendor/sdk");
    git(&sdk, ["checkout", "-b", "feature/duplicate-pr"]);
    fs::write(
        sdk.join("lib.rs"),
        "pub fn sdk() -> &'static str { \"duplicate\" }\n",
    )
    .unwrap();
    gnit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    let commit = gnit(workspace, ["commit", "-m", "Update duplicate PR"]);
    let change_id = parse_created_change(&commit);
    gnit(workspace, ["push"]).success();

    let marker = format!(
        "<!-- gnit-pr-sync:start -->\nGnit-Change-Id: {change_id}\n<!-- gnit-pr-sync:end -->"
    );
    let gh = fake_gh();
    gh.write_state(json!({
        "next": 3,
        "prs": [
            {
                "repo": "acme/sdk",
                "number": 1,
                "state": "OPEN",
                "url": "https://github.com/acme/sdk/pull/1",
                "title": "One",
                "head": "feature/duplicate-pr",
                "body": marker,
                "checks": []
            },
            {
                "repo": "acme/sdk",
                "number": 2,
                "state": "CLOSED",
                "url": "https://github.com/acme/sdk/pull/2",
                "title": "Two",
                "head": "feature/duplicate-pr",
                "body": marker,
                "checks": []
            }
        ]
    }));

    gh.command(workspace, ["pr", "open", "--change", &change_id])
        .assert()
        .failure()
        .stderr(predicate::str::contains("multiple PRs"));
}

#[cfg(unix)]
#[test]
fn pr_status_degrades_when_gh_is_offline() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();
    prepare_pr_base_refs(workspace, ["vendor/sdk"]);

    let sdk = workspace.join("vendor/sdk");
    git(&sdk, ["checkout", "-b", "feature/offline"]);
    fs::write(
        sdk.join("lib.rs"),
        "pub fn sdk() -> &'static str { \"offline\" }\n",
    )
    .unwrap();
    gnit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    let commit = gnit(workspace, ["commit", "-m", "Update offline SDK"]);
    let change_id = parse_created_change(&commit);

    let mut cmd = gnit_command(workspace, ["pr", "--change", &change_id]);
    cmd.env("GNIT_GH_BIN", workspace.join("missing-gh"))
        .assert()
        .success()
        .stdout(predicate::str::contains("Workspace change"))
        .stdout(predicate::str::contains("unknown"))
        .stdout(predicate::str::contains("offline"));
}

#[cfg(unix)]
#[test]
fn pr_open_resumes_after_partial_create_failure() {
    let (fixture, change_id) = three_repo_pr_change();
    let workspace = fixture.root.as_path();
    let gh = fake_gh();

    let mut first = gh.command(workspace, ["pr", "open", "--change", &change_id]);
    first
        .env("GNIT_FAKE_GH_FAIL_CREATE_REPO", "acme/app")
        .assert()
        .failure()
        .stderr(predicate::str::contains("pr open incomplete"));
    assert_eq!(gh.state()["prs"].as_array().unwrap().len(), 2);

    gh.command(workspace, ["pr", "open", "--change", &change_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("already open"))
        .stdout(predicate::str::contains("created"));

    let state = gh.state();
    let prs = state["prs"].as_array().unwrap();
    assert_eq!(prs.len(), 3, "{state}");
    for pr in prs {
        let body = pr["body"].as_str().unwrap();
        assert!(body.contains("acme/root#"));
        assert!(body.contains("acme/sdk#"));
        assert!(body.contains("acme/app#"));
    }
}

#[cfg(unix)]
#[test]
fn pr_status_shows_mixed_states_and_checks() {
    let (fixture, change_id) = three_repo_pr_change();
    let workspace = fixture.root.as_path();
    let marker = format!(
        "<!-- gnit-pr-sync:start -->\nGnit-Change-Id: {change_id}\n<!-- gnit-pr-sync:end -->"
    );
    let gh = fake_gh();
    gh.write_state(json!({
        "next": 4,
        "prs": [
            {
                "repo": "acme/root",
                "number": 1,
                "state": "OPEN",
                "url": "https://github.com/acme/root/pull/1",
                "title": "Root",
                "head": "feature/pr-flow",
                "body": marker,
                "checks": [{"status": "IN_PROGRESS"}]
            },
            {
                "repo": "acme/sdk",
                "number": 2,
                "state": "MERGED",
                "url": "https://github.com/acme/sdk/pull/2",
                "title": "SDK",
                "head": "feature/pr-flow",
                "body": marker,
                "checks": [{"status": "COMPLETED", "conclusion": "SUCCESS"}]
            },
            {
                "repo": "acme/app",
                "number": 3,
                "state": "CLOSED",
                "url": "https://github.com/acme/app/pull/3",
                "title": "App",
                "head": "feature/pr-flow",
                "body": marker,
                "checks": [{"status": "COMPLETED", "conclusion": "FAILURE"}]
            }
        ]
    }));

    gh.command(workspace, ["pr", "--change", &change_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("open"))
        .stdout(predicate::str::contains("pending"))
        .stdout(predicate::str::contains("merged"))
        .stdout(predicate::str::contains("pass"))
        .stdout(predicate::str::contains("closed"))
        .stdout(predicate::str::contains("fail"));
}

#[cfg(unix)]
#[test]
fn pr_open_preflight_failure_does_not_create_any_prs() {
    let fixture = workspace_with_three_member_remotes();
    let workspace = fixture.root.as_path();
    prepare_pr_base_refs(workspace, ["sdk", "app", "docs"]);

    git(workspace, ["checkout", "-b", "feature/not-pushed"]);
    git(
        &workspace.join("sdk"),
        ["checkout", "-b", "feature/not-pushed"],
    );
    git(
        &workspace.join("app"),
        ["checkout", "-b", "feature/not-pushed"],
    );

    fs::write(workspace.join("README.md"), "root not pushed\n").unwrap();
    fs::write(workspace.join("sdk/sdk.txt"), "sdk not pushed\n").unwrap();
    fs::write(workspace.join("app/app.txt"), "app not pushed\n").unwrap();
    gnit(
        workspace,
        ["add", "README.md", "sdk/sdk.txt", "app/app.txt"],
    );
    let land = gnit(workspace, ["land", "review-pin", "-m", "Add unpushed flow"]);
    let change_id = parse_created_change(&land);

    let gh = fake_gh();
    gh.command(workspace, ["pr", "open", "--change", &change_id])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "pr open blocked before creating PRs",
        ))
        .stderr(predicate::str::contains("run `gnit push`"));

    let state = gh.state();
    assert_eq!(state["prs"].as_array().unwrap().len(), 0, "{state}");
    assert!(
        !gh.calls().iter().any(|call| call_is(call, "pr", "create")),
        "preflight failure must not create PRs: {:?}",
        gh.calls()
    );
    assert!(
        !gh.calls().iter().any(|call| call_is(call, "pr", "edit")),
        "preflight failure must not edit PR bodies: {:?}",
        gh.calls()
    );
}

#[cfg(unix)]
#[test]
fn pr_open_resumes_after_body_edit_failure() {
    let (fixture, change_id) = three_repo_pr_change();
    let workspace = fixture.root.as_path();
    let gh = fake_gh();

    let mut first = gh.command(workspace, ["pr", "open", "--change", &change_id]);
    first
        .env("GNIT_FAKE_GH_FAIL_EDIT_REPO", "acme/app")
        .assert()
        .failure()
        .stderr(predicate::str::contains("pr body update incomplete"))
        .stderr(predicate::str::contains("forced edit failure for acme/app"));

    let state = gh.state();
    let prs = state["prs"].as_array().unwrap();
    assert_eq!(prs.len(), 3, "{state}");
    let app_body = prs.iter().find(|pr| pr["repo"] == "acme/app").unwrap()["body"]
        .as_str()
        .unwrap();
    assert!(
        !app_body.contains("acme/root#"),
        "failed body edit should leave app with its provisional marker: {app_body}"
    );

    gh.command(workspace, ["pr", "open", "--change", &change_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("already open"))
        .stdout(predicate::str::contains("PRs synchronized."));

    let state = gh.state();
    for pr in state["prs"].as_array().unwrap() {
        let body = pr["body"].as_str().unwrap();
        assert!(body.contains("acme/root#"), "{body}");
        assert!(body.contains("acme/sdk#"), "{body}");
        assert!(body.contains("acme/app#"), "{body}");
    }
}

#[cfg(unix)]
#[test]
fn pr_open_replaces_marker_once_and_preserves_author_text() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();
    prepare_pr_base_refs(workspace, ["vendor/sdk"]);

    let sdk = workspace.join("vendor/sdk");
    git(&sdk, ["checkout", "-b", "feature/replace-marker"]);
    fs::write(
        sdk.join("lib.rs"),
        "pub fn sdk() -> &'static str { \"replace\" }\n",
    )
    .unwrap();
    gnit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    let commit = gnit(workspace, ["commit", "-m", "Update marker body"]);
    let change_id = parse_created_change(&commit);
    gnit(workspace, ["push"]).success();

    let old_body = format!(
        "Intro stays.\n\n<!-- gnit-pr-sync:start -->\nGnit-Change-Id: {change_id}\nOld: remove me\n<!-- gnit-pr-sync:end -->\n\nFooter stays."
    );
    let gh = fake_gh();
    gh.write_state(json!({
        "next": 2,
        "prs": [{
            "repo": "acme/sdk",
            "number": 1,
            "state": "OPEN",
            "url": "https://github.com/acme/sdk/pull/1",
            "title": "Manual PR",
            "head": "feature/replace-marker",
            "body": old_body,
            "checks": []
        }]
    }));

    gh.command(workspace, ["pr", "open", "--change", &change_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("already open"));

    let state = gh.state();
    let body = state["prs"][0]["body"].as_str().unwrap();
    assert!(body.contains("Intro stays."), "{body}");
    assert!(body.contains("Footer stays."), "{body}");
    assert!(!body.contains("Old: remove me"), "{body}");
    assert_eq!(
        body.matches("<!-- gnit-pr-sync:start -->").count(),
        1,
        "{body}"
    );
    assert_eq!(
        body.matches("<!-- gnit-pr-sync:end -->").count(),
        1,
        "{body}"
    );
    assert!(body.contains("Member PRs:\n- acme/sdk#1 @"), "{body}");
}

#[cfg(unix)]
#[test]
fn pr_open_ready_creates_non_draft_prs() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();
    prepare_pr_base_refs(workspace, ["vendor/sdk"]);

    let sdk = workspace.join("vendor/sdk");
    git(&sdk, ["checkout", "-b", "feature/ready-pr"]);
    fs::write(
        sdk.join("lib.rs"),
        "pub fn sdk() -> &'static str { \"ready\" }\n",
    )
    .unwrap();
    gnit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    gnit(workspace, ["commit", "-m", "Update ready PR"]);
    gnit(workspace, ["push"]).success();

    let gh = fake_gh();
    gh.command(workspace, ["pr", "open", "--ready"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Mode: ready"))
        .stdout(predicate::str::contains("created"));

    let state = gh.state();
    let prs = state["prs"].as_array().unwrap();
    assert_eq!(prs.len(), 1, "{state}");
    assert_eq!(prs[0]["draft"], false, "{state}");
}

#[cfg(unix)]
#[test]
fn pr_pin_alias_resolves_single_provenance_change() {
    let (fixture, change_id) = three_repo_pr_change();
    let workspace = fixture.root.as_path();

    fake_gh()
        .command(workspace, ["pr", "--pin", "review-pin"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Pin: review-pin"))
        .stdout(predicate::str::contains(change_id));
}

#[test]
fn push_ignores_retained_pins_for_retired_missing_members() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();
    let sdk = workspace.join("vendor/sdk");

    gnit(workspace, ["pin", "baseline"]).success();
    std::fs::write(
        workspace.join(".gnit/roster.yaml"),
        "version: 1\nmode: shared\nmembers: []\n",
    )
    .unwrap();
    git(workspace, ["add", ".gnit/roster.yaml"]);
    git(workspace, ["commit", "-m", "Retire sdk"]);
    std::fs::remove_dir_all(&sdk).unwrap();

    gnit(workspace, ["push"])
        .success()
        .stdout(predicate::str::contains("workspace root"))
        .stdout(predicate::str::contains("pushed"))
        .stdout(predicate::str::contains("pin baseline").not());

    assert_eq!(
        git_dir_out(&fixture.root_remote, ["rev-parse", "master"]),
        git_out(workspace, ["rev-parse", "HEAD"])
    );
}

#[test]
fn checkout_recreates_local_branch_from_remote_pin_hint() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();

    std::fs::write(
        workspace.join("vendor/sdk/lib.rs"),
        "pub fn sdk() -> &'static str { \"branch-aware\" }\n",
    )
    .unwrap();
    gnit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    gnit(workspace, ["land", "baseline", "-m", "Publish sdk update"]).success();
    gnit(workspace, ["push"]).success();

    let root_remote = fixture.root_remote.to_str().unwrap();
    let restored = fixture._temp.path().join("branch-restored");
    let restored_path = restored.to_str().unwrap();
    gnit(
        fixture._temp.path(),
        ["clone", root_remote, restored_path, "--pin", "baseline"],
    )
    .success();

    let sdk = restored.join("vendor/sdk");
    git(&sdk, ["checkout", "--detach", "HEAD"]);
    git(&sdk, ["branch", "-D", "master"]);

    gnit(&restored, ["checkout", "baseline"])
        .success()
        .stdout(predicate::str::contains("on master"));
    assert_eq!(
        git_out(&sdk, ["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
        "master"
    );
}

#[test]
fn checkout_prefers_hinted_remote_branch_over_other_local_branch_at_commit() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();

    std::fs::write(
        workspace.join("vendor/sdk/lib.rs"),
        "pub fn sdk() -> &'static str { \"hinted\" }\n",
    )
    .unwrap();
    gnit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    gnit(workspace, ["land", "baseline", "-m", "Publish sdk update"]).success();
    gnit(workspace, ["push"]).success();

    let root_remote = fixture.root_remote.to_str().unwrap();
    let restored = fixture._temp.path().join("hint-restored");
    let restored_path = restored.to_str().unwrap();
    gnit(
        fixture._temp.path(),
        ["clone", root_remote, restored_path, "--pin", "baseline"],
    )
    .success();

    let sdk = restored.join("vendor/sdk");
    git(&sdk, ["checkout", "-b", "topic"]);
    git(&sdk, ["branch", "-D", "master"]);

    gnit(&restored, ["checkout", "baseline"])
        .success()
        .stdout(predicate::str::contains("on master"));
    assert_eq!(
        git_out(&sdk, ["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
        "master"
    );
}

#[test]
fn checkout_fast_forwards_existing_local_branch_to_remote_pin_hint() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();

    std::fs::write(
        workspace.join("vendor/sdk/lib.rs"),
        "pub fn sdk() -> &'static str { \"fast-forward\" }\n",
    )
    .unwrap();
    gnit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    gnit(workspace, ["land", "baseline", "-m", "Publish sdk update"]).success();
    gnit(workspace, ["push"]).success();

    let root_remote = fixture.root_remote.to_str().unwrap();
    let restored = fixture._temp.path().join("ff-restored");
    let restored_path = restored.to_str().unwrap();
    gnit(fixture._temp.path(), ["clone", root_remote, restored_path]).success();

    let sdk = restored.join("vendor/sdk");
    let remote_head = git_out(&sdk, ["rev-parse", "origin/master"]);
    git(&sdk, ["reset", "--hard", "HEAD~1"]);

    gnit(&restored, ["checkout", "baseline"])
        .success()
        .stdout(predicate::str::contains("on master"));
    assert_eq!(git_out(&sdk, ["rev-parse", "master"]), remote_head);
    assert_eq!(
        git_out(&sdk, ["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
        "master"
    );
}

#[test]
fn checkout_detaches_without_repointing_ahead_branch_by_default() {
    let fixture = clean_workspace_with_sdk();
    let workspace = fixture.root.as_path();
    let sdk = workspace.join("vendor/sdk");

    let baseline_head = git_out(&sdk, ["rev-parse", "HEAD"]);
    gnit(workspace, ["pin", "baseline"]).success();

    std::fs::write(
        sdk.join("lib.rs"),
        "pub fn sdk() -> &'static str { \"ahead\" }\n",
    )
    .unwrap();
    git(&sdk, ["add", "lib.rs"]);
    git(&sdk, ["commit", "-m", "Ahead sdk"]);
    let branch_head = git_out(&sdk, ["rev-parse", "master"]);

    gnit(workspace, ["checkout", "baseline"])
        .success()
        .stderr(predicate::str::contains("detached"));

    assert_eq!(
        git_out(&sdk, ["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
        "HEAD"
    );
    assert_eq!(git_out(&sdk, ["rev-parse", "HEAD"]), baseline_head);
    assert_eq!(git_out(&sdk, ["rev-parse", "master"]), branch_head);
}

#[test]
fn checkout_detaches_when_hinted_remote_cannot_fast_forward_local_branch() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();

    std::fs::write(
        workspace.join("vendor/sdk/lib.rs"),
        "pub fn sdk() -> &'static str { \"remote\" }\n",
    )
    .unwrap();
    gnit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    gnit(workspace, ["land", "baseline", "-m", "Publish sdk update"]).success();
    gnit(workspace, ["push"]).success();

    let root_remote = fixture.root_remote.to_str().unwrap();
    let restored = fixture._temp.path().join("diverged-restored");
    let restored_path = restored.to_str().unwrap();
    gnit(fixture._temp.path(), ["clone", root_remote, restored_path]).success();

    let sdk = restored.join("vendor/sdk");
    let pinned_head = git_out(&sdk, ["rev-parse", "origin/master"]);
    git(&sdk, ["reset", "--hard", "HEAD~1"]);
    std::fs::write(
        sdk.join("lib.rs"),
        "pub fn sdk() -> &'static str { \"diverged\" }\n",
    )
    .unwrap();
    git(&sdk, ["add", "lib.rs"]);
    git(&sdk, ["commit", "-m", "Diverged sdk"]);
    let branch_head = git_out(&sdk, ["rev-parse", "master"]);

    gnit(&restored, ["checkout", "baseline"])
        .success()
        .stderr(predicate::str::contains("cannot fast-forward"));

    assert_eq!(
        git_out(&sdk, ["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
        "HEAD"
    );
    assert_eq!(git_out(&sdk, ["rev-parse", "HEAD"]), pinned_head);
    assert_eq!(git_out(&sdk, ["rev-parse", "master"]), branch_head);
}

#[test]
fn exact_checkout_detaches_without_repointing_current_branch() {
    let fixture = clean_workspace_with_sdk();
    let workspace = fixture.root.as_path();
    let sdk = workspace.join("vendor/sdk");

    let baseline_head = git_out(&sdk, ["rev-parse", "HEAD"]);
    gnit(workspace, ["pin", "baseline"]).success();

    std::fs::write(
        sdk.join("lib.rs"),
        "pub fn sdk() -> &'static str { \"later\" }\n",
    )
    .unwrap();
    git(&sdk, ["add", "lib.rs"]);
    git(&sdk, ["commit", "-m", "Later sdk"]);
    let branch_head = git_out(&sdk, ["rev-parse", "master"]);

    gnit(workspace, ["checkout", "baseline", "--exact"])
        .success()
        .stderr(predicate::str::contains("detached"));

    assert_eq!(
        git_out(&sdk, ["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
        "HEAD"
    );
    assert_eq!(git_out(&sdk, ["rev-parse", "HEAD"]), baseline_head);
    assert_eq!(git_out(&sdk, ["rev-parse", "master"]), branch_head);
}

#[test]
fn import_submodule_workflow_converts_gitlink_to_member() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    git_init(&workspace);
    git(
        &workspace,
        ["config", "user.email", "gnit-test@example.com"],
    );
    git(&workspace, ["config", "user.name", "Gnit Test"]);
    std::fs::write(workspace.join("README.md"), "root\n").unwrap();
    git(&workspace, ["add", "README.md"]);
    git(&workspace, ["commit", "-m", "Initial root"]);

    let sub_source = temp.path().join("sub-source");
    std::fs::create_dir(&sub_source).unwrap();
    git_init(&sub_source);
    git(
        &sub_source,
        ["config", "user.email", "gnit-test@example.com"],
    );
    git(&sub_source, ["config", "user.name", "Gnit Test"]);
    std::fs::write(sub_source.join("lib.rs"), "pub fn sub() {}\n").unwrap();
    git(&sub_source, ["add", "lib.rs"]);
    git(&sub_source, ["commit", "-m", "Initial sub"]);

    git_args(
        &workspace,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            sub_source.to_str().unwrap(),
            "vendor/sub",
        ],
    );
    git(&workspace, ["commit", "-m", "Add submodule"]);
    assert!(
        git_out(&workspace, ["ls-files", "--stage", "vendor/sub"]).starts_with("160000 "),
        "fixture should start with a gitlink"
    );

    gnit(&workspace, ["init"]).success();
    gnit(
        &workspace,
        ["import-submodule", "vendor/sub", "--id", "sub"],
    )
    .success()
    .stdout(predicate::str::contains("imported submodule vendor/sub"));

    let roster = std::fs::read_to_string(workspace.join(".gnit/roster.yaml")).unwrap();
    assert!(roster.contains("id: sub"));
    assert!(roster.contains("path: vendor/sub"));
    assert!(roster.contains(sub_source.to_str().unwrap()));

    let index_entry = git_out(&workspace, ["ls-files", "--stage", "vendor/sub"]);
    assert!(
        index_entry.trim().is_empty(),
        "gitlink should be removed from root index: {index_entry}"
    );
    let modules = std::fs::read_to_string(workspace.join(".gitmodules")).unwrap_or_default();
    assert!(!modules.contains("vendor/sub"));
    let last_commit = git_out(&workspace, ["log", "-1", "--pretty=%s"]);
    assert_eq!(last_commit.trim(), "Import Gnit member vendor/sub");
}

#[test]
fn status_reports_member_state_and_discovered() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    std::fs::write(ws.join("vendor/sdk/new.txt"), "x\n").unwrap();
    std::fs::create_dir_all(ws.join("scratch")).unwrap();
    git_init(&ws.join("scratch"));

    gnit(ws, ["status"])
        .success()
        .stdout(predicate::str::contains("sdk"))
        .stdout(predicate::str::contains("untracked"))
        .stdout(predicate::str::contains("Discovered (not adopted)"))
        .stdout(predicate::str::contains("scratch"));
}

#[test]
fn log_interleaves_changes_and_pins() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    std::fs::write(ws.join("vendor/sdk/lib.rs"), "pub fn v() {}\n").unwrap();
    gnit(ws, ["add", "vendor/sdk/lib.rs"]);
    gnit(ws, ["land", "release", "-m", "Ship it"]).success();

    gnit(ws, ["log"])
        .success()
        .stdout(predicate::str::contains("change GCH-"))
        .stdout(predicate::str::contains("pin    release"));
}

#[test]
fn log_orders_equal_timestamp_entries_by_stable_secondary_key() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    let sdk = ws.join("vendor/sdk");
    let date = "2026-01-01T00:00:00Z";
    let changes = [
        (
            "GCH-1760000000000-c",
            "Zulu change",
            "pub fn sdk() { /* z */ }\n",
        ),
        (
            "GCH-1760000000000-a",
            "Alpha change",
            "pub fn sdk() { /* a */ }\n",
        ),
        (
            "GCH-1760000000000-b",
            "Middle change",
            "pub fn sdk() { /* m */ }\n",
        ),
    ];

    for (change_id, subject, contents) in changes {
        fs::write(sdk.join("lib.rs"), contents).unwrap();
        git(&sdk, ["add", "lib.rs"]);
        git_commit_at(
            &sdk,
            &format!("{subject}\n\nGnit-Change-Id: {change_id}"),
            date,
        );
    }

    let output = gnit_output(ws, ["log"]);
    let change_lines = output
        .lines()
        .filter(|line| line.contains("  change GCH-"))
        .collect::<Vec<_>>();
    assert_eq!(change_lines.len(), 3, "{output}");
    assert!(change_lines[0].contains("GCH-1760000000000-a"), "{output}");
    assert!(change_lines[1].contains("GCH-1760000000000-b"), "{output}");
    assert!(change_lines[2].contains("GCH-1760000000000-c"), "{output}");
}

#[test]
fn change_discovery_parses_real_trailers_not_substrings() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    let sdk = ws.join("vendor/sdk");

    // A trailer-shaped line in a middle paragraph is prose, not membership.
    fs::write(sdk.join("lib.rs"), "pub fn sdk() { /* prose */ }\n").unwrap();
    git(&sdk, ["add", "lib.rs"]);
    git(
        &sdk,
        [
            "commit",
            "-m",
            "Mention a change\n\nGnit-Change-Id: GCH-1760000000000-dead\n\nDiscussed above, not a trailer block.",
        ],
    );

    // The no-space trailer spelling is a valid git trailer and must count.
    fs::write(sdk.join("lib.rs"), "pub fn sdk() { /* nospace */ }\n").unwrap();
    git(&sdk, ["add", "lib.rs"]);
    git(
        &sdk,
        [
            "commit",
            "-m",
            "Tight trailer\n\nGnit-Change-Id:GCH-1760000000001-beef",
        ],
    );

    let output = gnit_output(ws, ["change", "log"]);
    assert!(
        !output.contains("GCH-1760000000000-dead"),
        "mid-paragraph mention must not register as membership:\n{output}"
    );
    assert!(
        output.contains("GCH-1760000000001-beef"),
        "no-space trailer spelling must register:\n{output}"
    );
}

#[cfg(unix)]
#[test]
fn commit_partial_failure_is_resumable_with_same_change_id() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    let sdk = ws.join("vendor/sdk");

    // A failing pre-commit hook in the member interrupts the workspace commit
    // after the root has already committed with the freshly minted id.
    let hook = sdk.join(".git/hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    let mut perms = fs::metadata(&hook).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&hook, perms).unwrap();

    fs::write(ws.join("README.md"), "root v2\n").unwrap();
    fs::write(sdk.join("lib.rs"), "pub fn sdk() { /* v2 */ }\n").unwrap();
    gnit(ws, ["add", "README.md", "vendor/sdk/lib.rs"]);

    let assert = gnit(ws, ["commit", "-m", "Cross-repo update"]).failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    let change_id = stderr
        .split("gnit commit --change ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_else(|| panic!("partial-failure error must name the resume command:\n{stderr}"))
        .to_string();

    let root_body = git_out(ws, ["log", "-1", "--pretty=%B"]);
    assert!(
        root_body.contains(&format!("Gnit-Change-Id: {change_id}")),
        "root should already carry the partial change:\n{root_body}"
    );

    // Re-running plain `gnit commit` would mint a second id; resuming with
    // --change reunifies the change under the original one.
    fs::remove_file(&hook).unwrap();
    gnit(
        ws,
        ["commit", "--change", &change_id, "-m", "Cross-repo update"],
    )
    .success();

    let sdk_body = git_out(&sdk, ["log", "-1", "--pretty=%B"]);
    assert!(
        sdk_body.contains(&format!("Gnit-Change-Id: {change_id}")),
        "member resume must reuse the original change id:\n{sdk_body}"
    );
    gnit(ws, ["change", "status", &change_id])
        .success()
        .stdout(predicate::str::contains("root:"))
        .stdout(predicate::str::contains("sdk:"));
}

#[test]
fn commit_change_resume_rejects_unknown_or_malformed_ids() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();

    fs::write(ws.join("README.md"), "root v2\n").unwrap();
    gnit(ws, ["add", "README.md"]);

    gnit(
        ws,
        ["commit", "--change", "GCH-1760000000000-72e5", "-m", "x"],
    )
    .failure()
    .stderr(predicate::str::contains("not found"));
    gnit(ws, ["commit", "--change", "not-a-change-id", "-m", "x"])
        .failure()
        .stderr(predicate::str::contains("not a valid change id"));
}

#[test]
fn upkeep_restores_missing_local_exclude() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    let exclude = ws.join(".git/info/exclude");
    let text = std::fs::read_to_string(&exclude).unwrap();
    let stripped: String = text
        .lines()
        .filter(|line| *line != "vendor/sdk")
        .map(|line| format!("{line}\n"))
        .collect();
    std::fs::write(&exclude, stripped).unwrap();

    // Any command runs transparent upkeep, which restores the local exclude.
    gnit(ws, ["status"]).success();
    let restored = std::fs::read_to_string(&exclude).unwrap();
    assert!(
        restored.lines().any(|line| line == "vendor/sdk"),
        "upkeep should restore the local exclude: {restored}"
    );
}

#[test]
fn status_includes_dirty_root_repo() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    // Only the workspace ROOT is dirty (staged + untracked); members are clean.
    std::fs::write(ws.join("root_change.txt"), "x\n").unwrap();
    git(ws, ["add", "root_change.txt"]);
    std::fs::write(ws.join("root_untracked.txt"), "y\n").unwrap();

    let output = gnit_output(ws, ["status"]);
    let root_line = output
        .lines()
        .find(|line| line.trim_start().starts_with("root"))
        .unwrap_or_else(|| panic!("missing root repo status line:\n{output}"));
    assert!(
        root_line.contains("1 staged") && root_line.contains("1 untracked"),
        "root line should report staged and untracked changes:\n{output}"
    );
    let sdk_line = output
        .lines()
        .find(|line| line.trim_start().starts_with("sdk"))
        .unwrap_or_else(|| panic!("missing sdk status line:\n{output}"));
    assert!(
        sdk_line.contains("clean"),
        "member should stay clean while only root is dirty:\n{output}"
    );
}

#[test]
fn status_hides_pure_root_gnit_metadata_changes() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    fs::write(ws.join(".gnit/local-noise.txt"), "metadata only\n").unwrap();

    let output = gnit_output(ws, ["status"]);
    let root_line = output
        .lines()
        .find(|line| line.trim_start().starts_with("root"))
        .unwrap_or_else(|| panic!("missing root repo status line:\n{output}"));
    assert!(
        root_line.contains("clean"),
        "pure .gnit metadata noise should be hidden from root status:\n{output}"
    );
}

#[test]
fn status_counts_renames_across_gnit_metadata_boundary() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();

    fs::write(ws.join(".gnit/meta-fixture.txt"), "metadata tracked\n").unwrap();
    fs::write(ws.join("root-fixture.txt"), "root tracked\n").unwrap();
    git(ws, ["add", ".gnit/meta-fixture.txt", "root-fixture.txt"]);
    git(ws, ["commit", "-m", "Add rename fixtures"]);

    git(ws, ["mv", ".gnit/meta-fixture.txt", "visible-meta.txt"]);
    git(ws, ["mv", "root-fixture.txt", ".gnit/root-fixture.txt"]);
    fs::write(ws.join(".gnit/local-noise.txt"), "metadata only\n").unwrap();

    let output = gnit_output(ws, ["status"]);
    let root_line = output
        .lines()
        .find(|line| line.trim_start().starts_with("root"))
        .unwrap_or_else(|| panic!("missing root repo status line:\n{output}"));
    assert!(
        root_line.contains("2 staged"),
        "renames into and out of .gnit should count, while pure .gnit noise stays hidden:\n{output}"
    );
    assert!(
        !root_line.contains("untracked"),
        "untracked pure .gnit metadata should stay hidden:\n{output}"
    );
}

// ---- Error-contract sweep ---------------------------------------------------
// Deterministic, no-network coverage of the CLI's `bail!` surface: argument
// validation, "outside a workspace", and unknown-id handling. These guard the
// promises Gnit makes in its error messages, which agents and scripts depend on.

#[test]
fn add_rejects_all_combined_with_paths() {
    let fixture = clean_workspace_with_sdk();
    gnit(fixture.root.as_path(), ["add", "-A", "README.md"])
        .failure()
        .stderr(predicate::str::contains(
            "use either `gnit add -A` or explicit paths, not both",
        ));
}

#[test]
fn add_requires_paths_or_all() {
    let fixture = clean_workspace_with_sdk();
    gnit(fixture.root.as_path(), ["add"])
        .failure()
        .stderr(predicate::str::contains("nothing specified"));
}

#[test]
fn add_outside_workspace_is_rejected() {
    let temp = tempdir_without_gnit_ancestor();
    gnit(temp.path(), ["add", "README.md"])
        .failure()
        .stderr(predicate::str::contains("not in a Gnit workspace"));
}

#[test]
fn commit_without_staged_changes_is_rejected() {
    let fixture = clean_workspace_with_sdk();
    gnit(fixture.root.as_path(), ["commit", "-m", "nothing here"])
        .failure()
        .stderr(predicate::str::contains("no staged changes to commit"));
}

#[test]
fn init_in_existing_workspace_is_rejected() {
    let fixture = clean_workspace_with_sdk();
    gnit(fixture.root.as_path(), ["init"])
        .failure()
        .stderr(predicate::str::contains("Gnit workspace already exists"));
}

#[test]
fn adopt_duplicate_id_is_rejected() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    let other = ws.join("vendor/other");
    std::fs::create_dir_all(&other).unwrap();
    git_init(&other);
    std::fs::write(other.join("f.txt"), "x\n").unwrap();
    git(&other, ["add", "f.txt"]);
    git(&other, ["commit", "-m", "Initial other"]);

    gnit(ws, ["adopt", "vendor/other", "--id", "sdk"])
        .failure()
        .stderr(predicate::str::contains("member id sdk already exists"));
}

#[test]
fn adopt_id_with_multiple_paths_is_rejected() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    for name in ["a", "b"] {
        let member = ws.join(format!("vendor/{name}"));
        std::fs::create_dir_all(&member).unwrap();
        git_init(&member);
        std::fs::write(member.join("f.txt"), "x\n").unwrap();
        git(&member, ["add", "f.txt"]);
        git(&member, ["commit", "-m", "Initial member"]);
    }
    gnit(ws, ["adopt", "vendor/a", "vendor/b", "--id", "x"])
        .failure()
        .stderr(predicate::str::contains(
            "--id can only be used when adopting one path",
        ));
}

#[test]
fn clone_into_existing_target_is_rejected() {
    let fixture = workspace_with_remotes();
    let root_remote = fixture.root_remote.to_str().unwrap();
    let target = fixture._temp.path().join("occupied");
    std::fs::create_dir_all(&target).unwrap();
    gnit(
        fixture._temp.path(),
        ["clone", root_remote, target.to_str().unwrap()],
    )
    .failure()
    .stderr(predicate::str::contains("already exists"));
}

#[test]
fn checkout_unknown_pin_is_rejected() {
    let fixture = clean_workspace_with_sdk();
    gnit(fixture.root.as_path(), ["checkout", "nonesuch"])
        .failure()
        .stderr(predicate::str::contains("pin nonesuch not found"));
}

#[test]
fn pin_with_no_members_is_rejected() {
    let fixture = empty_workspace();
    gnit(fixture.root.as_path(), ["pin", "baseline"])
        .failure()
        .stderr(predicate::str::contains(
            "cannot pin a workspace with no members",
        ));
}

#[test]
fn change_show_unknown_is_rejected() {
    let fixture = clean_workspace_with_sdk();
    gnit(
        fixture.root.as_path(),
        ["change", "show", "GCH-does-not-exist"],
    )
    .failure()
    .stderr(predicate::str::contains("not found"));
}

#[test]
fn change_diff_unknown_is_rejected() {
    let fixture = clean_workspace_with_sdk();
    gnit(
        fixture.root.as_path(),
        ["change", "diff", "GCH-does-not-exist"],
    )
    .failure()
    .stderr(predicate::str::contains("not found"));
}

#[test]
fn push_rejects_detached_member() {
    let fixture = workspace_with_remotes();
    let ws = fixture.root.as_path();
    git(&ws.join("vendor/sdk"), ["checkout", "--detach", "HEAD"]);
    gnit(ws, ["push"])
        .failure()
        .stdout(predicate::str::contains("member sdk"))
        .stdout(predicate::str::contains(
            "is detached; checkout a branch before pushing",
        ))
        .stderr(predicate::str::contains(
            "push preflight failed; no repos were pushed",
        ));
}

#[test]
fn skills_install_rejects_explicit_with_all() {
    // Sandboxed HOME so the real harness skill directories are never touched.
    let env = skill_env();
    env.command(["skills", "install", "codex", "--all"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "use either explicit harnesses or --all, not both",
        ));
}

// ---- change diff happy path -------------------------------------------------

#[test]
fn change_diff_shows_per_repo_diffs() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    std::fs::write(ws.join("vendor/sdk/lib.rs"), "pub fn sdk() -> u8 { 7 }\n").unwrap();
    gnit(ws, ["add", "vendor/sdk/lib.rs"]);
    let commit = gnit(ws, ["commit", "-m", "Tweak sdk"]);
    let change_id = parse_created_change(&commit);

    gnit(ws, ["change", "diff", &change_id])
        .success()
        .stdout(predicate::str::contains(format!("Change {change_id}")))
        .stdout(predicate::str::contains("== sdk"))
        .stdout(predicate::str::contains("Tweak sdk"))
        .stdout(predicate::str::contains("lib.rs"));
}

// ---- doctor failure modes ---------------------------------------------------

#[test]
fn doctor_reports_missing_member() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    std::fs::remove_dir_all(ws.join("vendor/sdk")).unwrap();
    gnit(ws, ["doctor"])
        .success()
        .stdout(predicate::str::contains("member sdk: missing"));
}

#[test]
fn doctor_reports_remote_drift() {
    let fixture = workspace_with_remotes();
    let ws = fixture.root.as_path();
    git(
        &ws.join("vendor/sdk"),
        [
            "remote",
            "set-url",
            "origin",
            "https://example.invalid/moved.git",
        ],
    );
    gnit(ws, ["doctor"])
        .success()
        .stdout(predicate::str::contains("member sdk: remote drift"));
}

fn git<const N: usize>(dir: &Path, args: [&str; N]) {
    let status = git_command(dir).args(args).status().unwrap();
    assert!(
        status.success(),
        "git {:?} failed in {}",
        args,
        dir.display()
    );
}

fn git_init(dir: &Path) {
    git(dir, ["init", "-b", "master"]);
}

fn git_commit_at(dir: &Path, message: &str, date: &str) {
    let status = git_command(dir)
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_DATE", date)
        .args(["commit", "-m", message])
        .status()
        .unwrap();
    assert!(status.success(), "git commit failed in {}", dir.display());
}

fn git_args(dir: &Path, args: &[&str]) {
    let status = git_command(dir).args(args).status().unwrap();
    assert!(
        status.success(),
        "git {:?} failed in {}",
        args,
        dir.display()
    );
}

fn git_out<const N: usize>(dir: &Path, args: [&str; N]) -> String {
    let output = git_command(dir).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed in {}: {}",
        args,
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn git_dir_out<const N: usize>(git_dir: &Path, args: [&str; N]) -> String {
    let output = git_base_command()
        .arg("--git-dir")
        .arg(git_dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git --git-dir {} {:?} failed: {}",
        git_dir.display(),
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn assert_commit_absent(repo: &Path, commit: &str) {
    let output = git_command(repo)
        .args(["cat-file", "-e", &format!("{commit}^{{commit}}")])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "commit {commit} should not be present in {}",
        repo.display()
    );
}

fn assert_commit_present(repo: &Path, commit: &str) {
    let output = git_command(repo)
        .args(["cat-file", "-e", &format!("{commit}^{{commit}}")])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "commit {commit} should be present in {}: {}",
        repo.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_command(dir: &Path) -> std::process::Command {
    let mut command = git_base_command();
    command.current_dir(dir);
    command
}

fn git_base_command() -> std::process::Command {
    let mut command = std::process::Command::new("git");
    command
        .args([
            "-c",
            "init.defaultBranch=master",
            "-c",
            "advice.defaultBranchName=false",
        ])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Gnit Test")
        .env("GIT_AUTHOR_EMAIL", "gnit-test@example.com")
        .env("GIT_COMMITTER_NAME", "Gnit Test")
        .env("GIT_COMMITTER_EMAIL", "gnit-test@example.com");
    command
}

fn hold_workspace_lock(root: &Path) -> std::fs::File {
    let lock_path = root.join(".gnit/lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .unwrap();
    file.lock_exclusive().unwrap();
    file
}

fn tempdir_without_gnit_ancestor() -> tempfile::TempDir {
    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(parent) = cwd.parent() {
            candidates.push(parent.to_path_buf());
        }
    }
    candidates.push(std::env::temp_dir());
    candidates.push(PathBuf::from("/var/tmp"));
    candidates.push(PathBuf::from("/tmp"));

    for base in candidates {
        if !base.is_dir() || has_gnit_ancestor(&base) {
            continue;
        }
        for _ in 0..8 {
            if let Ok(temp) = tempfile::Builder::new()
                .prefix("gnit-outside-workspace-")
                .tempdir_in(&base)
            {
                if !has_gnit_ancestor(temp.path()) {
                    return temp;
                }
            }
        }
    }

    panic!("could not create a tempdir without a .gnit ancestor");
}

fn has_gnit_ancestor(path: &Path) -> bool {
    path.ancestors()
        .any(|ancestor| ancestor.join(".gnit").exists())
}

fn gnit_output<const N: usize>(dir: &Path, args: [&str; N]) -> String {
    let output = gnit_command(dir, args).output().unwrap();
    assert!(
        output.status.success(),
        "gnit {:?} failed in {}: {}",
        args,
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn clean_workspace_with_sdk() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    git_init(&root);
    git(&root, ["config", "user.email", "gnit-test@example.com"]);
    git(&root, ["config", "user.name", "Gnit Test"]);
    std::fs::write(root.join("README.md"), "root\n").unwrap();
    git(&root, ["add", "README.md"]);
    git(&root, ["commit", "-m", "Initial root"]);

    std::fs::create_dir_all(root.join("vendor/sdk")).unwrap();
    git_init(&root.join("vendor/sdk"));
    git(
        &root.join("vendor/sdk"),
        ["config", "user.email", "gnit-test@example.com"],
    );
    git(
        &root.join("vendor/sdk"),
        ["config", "user.name", "Gnit Test"],
    );
    std::fs::write(root.join("vendor/sdk/lib.rs"), "pub fn sdk() {}\n").unwrap();
    git(&root.join("vendor/sdk"), ["add", "lib.rs"]);
    git(&root.join("vendor/sdk"), ["commit", "-m", "Initial sdk"]);

    gnit(&root, ["init"]);
    gnit(&root, ["adopt", "vendor/sdk", "--id", "sdk"]);

    Fixture { _temp: temp, root }
}

fn empty_workspace() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    git_init(&root);
    git(&root, ["config", "user.email", "gnit-test@example.com"]);
    git(&root, ["config", "user.name", "Gnit Test"]);
    std::fs::write(root.join("README.md"), "root\n").unwrap();
    git(&root, ["add", "README.md"]);
    git(&root, ["commit", "-m", "Initial root"]);

    gnit(&root, ["init"]);

    Fixture { _temp: temp, root }
}

fn workspace_with_remotes() -> RemoteFixture {
    let temp = tempfile::tempdir().unwrap();
    let remotes = temp.path().join("remotes");
    std::fs::create_dir_all(&remotes).unwrap();
    let root_remote = remotes.join("root.git");
    let sdk_remote = remotes.join("sdk.git");
    git(
        temp.path(),
        ["init", "--bare", root_remote.to_str().unwrap()],
    );
    git(
        temp.path(),
        ["init", "--bare", sdk_remote.to_str().unwrap()],
    );

    let root = temp.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    git_init(&root);
    git(&root, ["config", "user.email", "gnit-test@example.com"]);
    git(&root, ["config", "user.name", "Gnit Test"]);
    git(
        &root,
        ["remote", "add", "origin", root_remote.to_str().unwrap()],
    );
    std::fs::write(root.join("README.md"), "root\n").unwrap();
    git(&root, ["add", "README.md"]);
    git(&root, ["commit", "-m", "Initial root"]);
    git(&root, ["push", "origin", "HEAD"]);

    std::fs::create_dir_all(root.join("vendor/sdk")).unwrap();
    git_init(&root.join("vendor/sdk"));
    git(
        &root.join("vendor/sdk"),
        ["config", "user.email", "gnit-test@example.com"],
    );
    git(
        &root.join("vendor/sdk"),
        ["config", "user.name", "Gnit Test"],
    );
    git(
        &root.join("vendor/sdk"),
        ["remote", "add", "origin", sdk_remote.to_str().unwrap()],
    );
    std::fs::write(root.join("vendor/sdk/lib.rs"), "pub fn sdk() {}\n").unwrap();
    git(&root.join("vendor/sdk"), ["add", "lib.rs"]);
    git(&root.join("vendor/sdk"), ["commit", "-m", "Initial sdk"]);
    git(&root.join("vendor/sdk"), ["push", "origin", "HEAD"]);

    gnit(&root, ["init"]);
    gnit(&root, ["adopt", "vendor/sdk", "--id", "sdk"]);

    RemoteFixture {
        _temp: temp,
        root,
        root_remote,
        sdk_remote,
    }
}

fn workspace_with_three_member_remotes() -> ThreeMemberRemoteFixture {
    let temp = tempfile::tempdir().unwrap();
    let remotes = temp.path().join("remotes");
    std::fs::create_dir_all(&remotes).unwrap();
    let root_remote = remotes.join("root.git");
    let sdk_remote = remotes.join("sdk.git");
    let app_remote = remotes.join("app.git");
    let docs_remote = remotes.join("docs.git");
    for remote in [&root_remote, &sdk_remote, &app_remote, &docs_remote] {
        git(temp.path(), ["init", "--bare", remote.to_str().unwrap()]);
    }

    let root = temp.path().join("workspace");
    std::fs::create_dir(&root).unwrap();
    git_init(&root);
    git(&root, ["config", "user.email", "gnit-test@example.com"]);
    git(&root, ["config", "user.name", "Gnit Test"]);
    git(
        &root,
        ["remote", "add", "origin", root_remote.to_str().unwrap()],
    );
    std::fs::write(root.join("README.md"), "root\n").unwrap();
    git(&root, ["add", "README.md"]);
    git(&root, ["commit", "-m", "Initial root"]);
    git(&root, ["push", "origin", "HEAD"]);

    create_member_repo(&root, "sdk", &sdk_remote, "sdk.txt", "sdk v1\n");
    create_member_repo(&root, "app", &app_remote, "app.txt", "app v1\n");
    create_member_repo(&root, "docs", &docs_remote, "docs.txt", "docs v1\n");

    gnit(&root, ["init"]);
    gnit(&root, ["adopt", "sdk", "--id", "sdk"]);
    gnit(&root, ["adopt", "app", "--id", "app"]);
    gnit(&root, ["adopt", "docs", "--id", "docs"]);

    ThreeMemberRemoteFixture {
        _temp: temp,
        root,
        root_remote,
        sdk_remote,
        app_remote,
        docs_remote,
    }
}

fn create_member_repo(root: &Path, path: &str, remote: &Path, file: &str, content: &str) {
    let member = root.join(path);
    std::fs::create_dir_all(&member).unwrap();
    git_init(&member);
    git(&member, ["config", "user.email", "gnit-test@example.com"]);
    git(&member, ["config", "user.name", "Gnit Test"]);
    git(
        &member,
        ["remote", "add", "origin", remote.to_str().unwrap()],
    );
    std::fs::write(member.join(file), content).unwrap();
    git(&member, ["add", file]);
    git(&member, ["commit", "-m", "Initial member"]);
    git(&member, ["push", "origin", "HEAD"]);
}

fn advance_remote(base: &Path, remote: &Path, dirname: &str, file: &str, content: &str) {
    let clone = base.join(dirname);
    git_args(
        base,
        &["clone", remote.to_str().unwrap(), clone.to_str().unwrap()],
    );
    git(&clone, ["config", "user.email", "gnit-test@example.com"]);
    git(&clone, ["config", "user.name", "Gnit Test"]);
    std::fs::write(clone.join(file), content).unwrap();
    git(&clone, ["add", file]);
    git(&clone, ["commit", "-m", "Advance remote"]);
    git(&clone, ["push", "origin", "HEAD"]);
}

fn remove_pins_with_label(root: &Path, label: &str) {
    let pins_dir = root.join(".gnit/pins");
    for entry in std::fs::read_dir(&pins_dir).unwrap() {
        let path = entry.unwrap().path();
        let text = std::fs::read_to_string(&path).unwrap();
        if text.lines().any(|line| line == format!("label: {label}")) {
            std::fs::remove_file(path).unwrap();
        }
    }
}

struct SkillEnv {
    _temp: tempfile::TempDir,
    home: PathBuf,
    data: PathBuf,
    grok_home: PathBuf,
}

impl SkillEnv {
    fn command<const N: usize>(&self, args: [&str; N]) -> Command {
        let mut command = Command::cargo_bin("gnit").unwrap();
        command
            .args(args)
            .current_dir(&self.home)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("GNIT_DATA_DIR", &self.data)
            .env("XDG_DATA_HOME", self.home.join(".xdg-data"))
            .env("GROK_HOME", &self.grok_home)
            .env("GNIT_NO_UPKEEP", "true");
        command
    }

    fn managed_skill(&self) -> PathBuf {
        self.data.join("skills/gnit")
    }

    fn claude_skill(&self) -> PathBuf {
        self.home.join(".claude/skills/gnit")
    }

    fn codex_skill(&self) -> PathBuf {
        self.home.join(".codex/skills/gnit")
    }

    fn opencode_skill(&self) -> PathBuf {
        self.home.join(".opencode/skills/gnit")
    }

    fn grok_skill(&self) -> PathBuf {
        self.grok_home.join("skills/gnit")
    }
}

fn skill_env() -> SkillEnv {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let home = root.join("home");
    let data = root.join("data");
    let grok_home = root.join("grok-home");
    fs::create_dir_all(&home).unwrap();
    SkillEnv {
        _temp: temp,
        home,
        data,
        grok_home,
    }
}

#[cfg(unix)]
struct FakeGh {
    _temp: tempfile::TempDir,
    bin: PathBuf,
    state: PathBuf,
}

#[cfg(unix)]
impl FakeGh {
    fn command<const N: usize>(&self, dir: &Path, args: [&str; N]) -> Command {
        let mut command = Command::cargo_bin("gnit").unwrap();
        command
            .args(args)
            .current_dir(dir)
            .env("GNIT_GH_BIN", &self.bin)
            .env("GNIT_FAKE_GH_STATE", &self.state)
            .env("GNIT_NO_UPKEEP", "true");
        hermetic_git_env(&mut command);
        command
    }

    fn state(&self) -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(&self.state).unwrap()).unwrap()
    }

    fn calls(&self) -> Vec<Vec<String>> {
        let state = self.state();
        state
            .get("calls")
            .and_then(|calls| calls.as_array())
            .into_iter()
            .flatten()
            .map(|call| {
                call.as_array()
                    .unwrap()
                    .iter()
                    .map(|arg| arg.as_str().unwrap().to_string())
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn write_state(&self, value: serde_json::Value) {
        fs::write(&self.state, serde_json::to_string_pretty(&value).unwrap()).unwrap();
    }
}

#[cfg(unix)]
fn fake_gh() -> FakeGh {
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("gh");
    let state = temp.path().join("state.json");
    fs::write(&state, r#"{"next":1,"prs":[]}"#).unwrap();
    fs::write(
        &bin,
        r#"#!/usr/bin/env python3
import json
import os
import sys

state_path = os.environ.get("GNIT_FAKE_GH_STATE")

def normalize(data):
    data.setdefault("next", 1)
    data.setdefault("prs", [])
    data.setdefault("calls", [])
    return data

def load():
    if not state_path or not os.path.exists(state_path):
        return normalize({"next": 1, "prs": []})
    with open(state_path) as f:
        return normalize(json.load(f))

def save(data):
    with open(state_path, "w") as f:
        json.dump(data, f)

def record(args, data=None):
    data = data or load()
    data.setdefault("calls", []).append(args)
    save(data)
    return data

def arg_after(args, name):
    if name in args:
        i = args.index(name)
        if i + 1 < len(args):
            return args[i + 1]
    return None

def repo_for_cwd():
    name = os.path.basename(os.getcwd())
    if name == "workspace":
        name = "root"
    if name == "sdk":
        name = "sdk"
    return f"acme/{name}"

args = sys.argv[1:]
if args == ["--version"]:
    record(args)
    print("gh version 2.93.0 (fake)")
    sys.exit(0)
if args[:2] == ["auth", "status"]:
    record(args)
    print("Logged in to github.com as gnit-test")
    sys.exit(0)
if args[:2] == ["repo", "view"]:
    record(args)
    print(json.dumps({"nameWithOwner": repo_for_cwd()}))
    sys.exit(0)
if args[:2] == ["pr", "list"]:
    data = record(args)
    repo = arg_after(args, "-R")
    head = arg_after(args, "--head")
    search = arg_after(args, "--search")
    needle = None
    if search:
        needle = search.strip('"')
    out = []
    for pr in data["prs"]:
        if repo and pr.get("repo") != repo:
            continue
        if head and pr.get("head") != head:
            continue
        if needle and needle not in pr.get("body", ""):
            continue
        out.append({
            "number": pr["number"],
            "state": pr.get("state", "OPEN"),
            "url": pr.get("url", ""),
            "title": pr.get("title", ""),
            "headRefName": pr.get("head", ""),
            "body": pr.get("body", ""),
            "statusCheckRollup": pr.get("checks", []),
        })
    print(json.dumps(out))
    sys.exit(0)
if args[:2] == ["pr", "create"]:
    data = record(args)
    repo = arg_after(args, "-R")
    if os.environ.get("GNIT_FAKE_GH_FAIL_CREATE_REPO") == repo:
        print(f"forced create failure for {repo}", file=sys.stderr)
        sys.exit(1)
    number = int(data.get("next", 1))
    data["next"] = number + 1
    head = arg_after(args, "--head")
    title = arg_after(args, "--title") or ""
    body = arg_after(args, "--body") or ""
    pr = {
        "repo": repo,
        "number": number,
        "state": "OPEN",
        "url": f"https://github.com/{repo}/pull/{number}",
        "title": title,
        "head": head,
        "body": body,
        "draft": "--draft" in args,
        "checks": [],
    }
    data["prs"].append(pr)
    save(data)
    print(pr["url"])
    sys.exit(0)
if args[:2] == ["pr", "edit"]:
    data = record(args)
    number = int(args[2])
    repo = arg_after(args, "-R")
    body = arg_after(args, "--body")
    if os.environ.get("GNIT_FAKE_GH_FAIL_EDIT_REPO") == repo:
        print(f"forced edit failure for {repo}", file=sys.stderr)
        sys.exit(1)
    for pr in data["prs"]:
        if pr.get("repo") == repo and int(pr.get("number")) == number:
            pr["body"] = body
            save(data)
            print(pr.get("url", ""))
            sys.exit(0)
    print(f"PR {repo}#{number} not found", file=sys.stderr)
    sys.exit(1)
print("unsupported fake gh args: " + " ".join(args), file=sys.stderr)
sys.exit(1)
"#,
    )
    .unwrap();
    let mut perms = fs::metadata(&bin).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&bin, perms).unwrap();
    FakeGh {
        _temp: temp,
        bin,
        state,
    }
}

#[cfg(unix)]
fn prepare_pr_base_refs<const N: usize>(workspace: &Path, members: [&str; N]) {
    prepare_one_pr_base_ref(workspace);
    for member in members {
        prepare_one_pr_base_ref(&workspace.join(member));
    }
}

#[cfg(unix)]
fn prepare_one_pr_base_ref(repo: &Path) {
    git(repo, ["fetch", "origin", "master"]);
    git(
        repo,
        [
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/master",
        ],
    );
}

#[cfg(unix)]
fn three_repo_pr_change() -> (ThreeMemberRemoteFixture, String) {
    let fixture = workspace_with_three_member_remotes();
    let workspace = fixture.root.as_path();
    prepare_pr_base_refs(workspace, ["sdk", "app", "docs"]);

    git(workspace, ["checkout", "-b", "feature/pr-flow"]);
    git(
        &workspace.join("sdk"),
        ["checkout", "-b", "feature/pr-flow"],
    );
    git(
        &workspace.join("app"),
        ["checkout", "-b", "feature/pr-flow"],
    );

    fs::write(workspace.join("README.md"), "root pr flow\n").unwrap();
    fs::write(workspace.join("sdk/sdk.txt"), "sdk pr flow\n").unwrap();
    fs::write(workspace.join("app/app.txt"), "app pr flow\n").unwrap();
    gnit(
        workspace,
        ["add", "README.md", "sdk/sdk.txt", "app/app.txt"],
    );
    let land = gnit(
        workspace,
        ["land", "review-pin", "-m", "Add linked PR flow"],
    );
    let change_id = parse_created_change(&land);
    gnit(workspace, ["push"]).success();

    (fixture, change_id)
}

fn gnit<const N: usize>(dir: &Path, args: [&str; N]) -> assert_cmd::assert::Assert {
    gnit_command(dir, args).assert()
}

/// Build a `gnit` invocation whose child Git processes are hermetic: the
/// developer's global/system Git config (gpgsign, hooksPath, autocrlf, ...) is
/// neutralized and a deterministic identity is supplied, so Gnit's own internal
/// commits behave identically on every machine. Mirrors `git_base_command` for
/// the fixtures' direct Git calls.
fn gnit_command<const N: usize>(dir: &Path, args: [&str; N]) -> Command {
    let mut command = Command::cargo_bin("gnit").unwrap();
    command.args(args).current_dir(dir);
    hermetic_git_env(&mut command);
    command
}

fn hermetic_git_env(command: &mut Command) {
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Gnit Test")
        .env("GIT_AUTHOR_EMAIL", "gnit-test@example.com")
        .env("GIT_COMMITTER_NAME", "Gnit Test")
        .env("GIT_COMMITTER_EMAIL", "gnit-test@example.com");
}

fn parse_created_change(assert: &assert_cmd::assert::Assert) -> String {
    let output = assert.get_output();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("created Change "))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| panic!("missing created Change line in {stdout}"))
}

fn call_is(call: &[String], first: &str, second: &str) -> bool {
    call.first().is_some_and(|arg| arg == first) && call.get(1).is_some_and(|arg| arg == second)
}

fn assert_gnit_guidance(text: &str) {
    assert!(text.contains("<!-- gnit:workspace:start -->"), "{text}");
    assert!(text.contains("Gnit workspace"), "{text}");
    assert!(text.contains("gnit --help"), "{text}");
    assert!(text.contains("<!-- gnit:workspace:end -->"), "{text}");
    assert_eq!(gnit_guidance_count(text), 1, "{text}");
}

fn gnit_guidance_count(text: &str) -> usize {
    text.matches("<!-- gnit:workspace:start -->").count()
}

#[test]
fn migrate_converts_legacy_nit_workspace() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();

    // Forge a pre-rename workspace: .nit/ metadata and the old guidance block.
    git(ws, ["mv", ".gnit", ".nit"]);
    let legacy_agents = "# Workspace\n\n<!-- nit:workspace:start -->\n> **Nit workspace** — drive cross-repo work with the `nit` CLI.\n<!-- nit:workspace:end -->\n";
    std::fs::write(ws.join("AGENTS.md"), legacy_agents).unwrap();
    git(ws, ["add", "AGENTS.md"]);
    git(ws, ["commit", "-m", "Forge legacy nit workspace"]);

    gnit(ws, ["doctor"])
        .success()
        .stdout(predicate::str::contains("legacy nit metadata"))
        .stdout(predicate::str::contains("gnit migrate"));

    gnit(ws, ["migrate"])
        .success()
        .stdout(predicate::str::contains(".nit -> .gnit"))
        .stdout(predicate::str::contains(
            "agent guidance: refreshed (AGENTS.md)",
        ));

    assert!(ws.join(".gnit/roster.yaml").exists());
    assert!(!ws.join(".nit").exists());
    let agents = std::fs::read_to_string(ws.join("AGENTS.md")).unwrap();
    assert!(!agents.contains("<!-- nit:workspace:start -->"), "{agents}");
    assert_gnit_guidance(&agents);

    // The migration lands as one committed metadata change; the tree is clean.
    let status = git_out(ws, ["status", "--porcelain"]);
    assert_eq!(status.trim(), "", "{status}");
    let subject = git_out(ws, ["log", "-1", "--pretty=%s"]);
    assert_eq!(subject.trim(), "Migrate workspace metadata to gnit");

    // Re-running is a no-op.
    gnit(ws, ["migrate"])
        .success()
        .stdout(predicate::str::contains("nothing to migrate"));
}
