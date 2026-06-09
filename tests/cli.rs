use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
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
    let mut cmd = Command::cargo_bin("nit").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Git-native multi-repo workspaces"));
}

#[test]
fn doctor_runs() {
    let mut cmd = Command::cargo_bin("nit").unwrap();
    cmd.arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("Nit doctor"))
        .stdout(predicate::str::contains(
            "upkeep: automatic non-destructive upkeep enabled",
        ));
}

#[test]
fn status_outside_workspace_is_clear() {
    let temp = tempdir_without_nit_ancestor();
    let mut cmd = Command::cargo_bin("nit").unwrap();
    cmd.current_dir(temp.path())
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("No Nit workspace found."));
}

#[test]
fn init_and_adopt_nested_repo_workflow_preserves_root_staging() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path();
    git(workspace, ["init"]);
    git(workspace, ["config", "user.email", "nit-test@example.com"]);
    git(workspace, ["config", "user.name", "Nit Test"]);

    std::fs::write(workspace.join("README.md"), "root\n").unwrap();
    git(workspace, ["add", "README.md"]);
    git(workspace, ["commit", "-m", "Initial root"]);

    std::fs::create_dir_all(workspace.join("vendor/sdk")).unwrap();
    git(&workspace.join("vendor/sdk"), ["init"]);
    git(
        &workspace.join("vendor/sdk"),
        ["config", "user.email", "nit-test@example.com"],
    );
    git(
        &workspace.join("vendor/sdk"),
        ["config", "user.name", "Nit Test"],
    );
    std::fs::write(workspace.join("vendor/sdk/lib.rs"), "pub fn sdk() {}\n").unwrap();
    git(&workspace.join("vendor/sdk"), ["add", "lib.rs"]);
    git(
        &workspace.join("vendor/sdk"),
        ["commit", "-m", "Initial sdk"],
    );

    std::fs::write(workspace.join("UNRELATED.txt"), "keep me staged\n").unwrap();
    git(workspace, ["add", "UNRELATED.txt"]);

    Command::cargo_bin("nit")
        .unwrap()
        .arg("init")
        .current_dir(workspace)
        .assert()
        .success()
        .stdout(predicate::str::contains("initialized Nit workspace"));

    Command::cargo_bin("nit")
        .unwrap()
        .args(["adopt", "vendor/sdk", "--id", "sdk"])
        .current_dir(workspace)
        .assert()
        .success()
        .stdout(predicate::str::contains("adopted sdk"));

    let roster = std::fs::read_to_string(workspace.join(".nit/roster.yaml")).unwrap();
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
        !root_status.lines().any(|line| line.contains(".nit/")),
        "Nit metadata should have been committed: {root_status}"
    );

    let last_commit = git_out(workspace, ["log", "-1", "--pretty=%s"]);
    assert_eq!(last_commit.trim(), "Update Nit roster");

    Command::cargo_bin("nit")
        .unwrap()
        .arg("status")
        .current_dir(workspace)
        .assert()
        .success()
        .stdout(predicate::str::contains("Repos"))
        .stdout(predicate::str::contains("sdk"))
        .stdout(predicate::str::contains("clean"));

    Command::cargo_bin("nit")
        .unwrap()
        .arg("doctor")
        .current_dir(workspace)
        .assert()
        .success()
        .stdout(predicate::str::contains("roster members: 1"));

    Command::cargo_bin("nit")
        .unwrap()
        .args(["pin", "baseline"])
        .current_dir(workspace)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "workspace root has uncommitted changes",
        ));

    git(workspace, ["commit", "-m", "Keep unrelated file"]);

    let sdk_head = git_out(&workspace.join("vendor/sdk"), ["rev-parse", "HEAD"]);
    Command::cargo_bin("nit")
        .unwrap()
        .args(["pin", "baseline"])
        .current_dir(workspace)
        .assert()
        .success()
        .stdout(predicate::str::contains("created Pin PIN-"));

    let pin_paths = std::fs::read_dir(workspace.join(".nit/pins"))
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
    assert!(last_commit.starts_with("Create Nit pin PIN-"));
}

#[test]
fn update_dry_run_shows_installer() {
    let mut cmd = Command::cargo_bin("nit").unwrap();
    cmd.args(["update", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mostlydev/nit"))
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
        .contains("# Driving Nit"));
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
    assert!(target.join(".nit-skill-managed").exists());
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
        .contains("# Driving Nit"));
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
        .contains("# Driving Nit"));
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

    Command::cargo_bin("nit")
        .unwrap()
        .args(["update", "--check"])
        .env(
            "NIT_UPDATE_CHECK_URL",
            format!("file://{}", latest.display()),
        )
        .env("NIT_UPDATE_CACHE_PATH", &cache)
        .assert()
        .success()
        .stdout(predicate::str::contains("nit 9.9.9 is available"));

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

    Command::cargo_bin("nit")
        .unwrap()
        .args(["update", "--check"])
        .env(
            "NIT_UPDATE_CHECK_URL",
            format!("file://{}", missing.display()),
        )
        .env("NIT_UPDATE_CHECK_TIMEOUT_SECS", "1")
        .env("NIT_UPDATE_CACHE_PATH", &cache)
        .assert()
        .success()
        .stderr(predicate::str::contains("nit update check unavailable"));

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

    nit(workspace, ["add", "README.md", "vendor/sdk/lib.rs"]);
    let commit = nit(workspace, ["commit", "-m", "Update root and sdk"]);
    let change_id = parse_created_change(&commit);

    let root_commit = git_out(workspace, ["log", "-1", "--pretty=%B"]);
    let sdk_commit = git_out(&workspace.join("vendor/sdk"), ["log", "-1", "--pretty=%B"]);
    assert!(root_commit.contains(&format!("Nit-Change-Id: {change_id}")));
    assert!(sdk_commit.contains(&format!("Nit-Change-Id: {change_id}")));

    nit(workspace, ["change", "status", &change_id])
        .success()
        .stdout(predicate::str::contains("root:"))
        .stdout(predicate::str::contains("sdk:"));
    nit(workspace, ["change", "show", &change_id])
        .success()
        .stdout(predicate::str::contains("Update root and sdk"));
    nit(workspace, ["change", "log"])
        .success()
        .stdout(predicate::str::contains(&change_id));

    std::fs::write(
        workspace.join("vendor/sdk/lib.rs"),
        "pub fn sdk() -> &'static str { \"ambiguous\" }\n",
    )
    .unwrap();
    git(&workspace.join("vendor/sdk"), ["add", "lib.rs"]);
    let duplicate_change_message = format!("Manual follow-up\n\nNit-Change-Id: {change_id}");
    git(
        &workspace.join("vendor/sdk"),
        ["commit", "-m", &duplicate_change_message],
    );
    nit(workspace, ["change", "status", &change_id])
        .success()
        .stdout(predicate::str::contains("sdk: ambiguous (2 commits)"));

    std::fs::write(
        workspace.join("vendor/sdk/lib.rs"),
        "pub fn sdk() -> &'static str { \"landed\" }\n",
    )
    .unwrap();
    nit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    let land = nit(workspace, ["land", "release", "-m", "Land sdk update"]);
    let landed_change = parse_created_change(&land);

    nit(workspace, ["review", &landed_change])
        .success()
        .stdout(predicate::str::contains("Change"))
        .stdout(predicate::str::contains("Land sdk update"));
    nit(workspace, ["review", "release"])
        .success()
        .stdout(predicate::str::contains("Review Pin"))
        .stdout(predicate::str::contains("Changes:"))
        .stdout(predicate::str::contains(&landed_change));

    let pin_paths = std::fs::read_dir(workspace.join(".nit/pins"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(pin_paths.len(), 1);
    let pin = std::fs::read_to_string(&pin_paths[0]).unwrap();
    assert!(pin.contains("label: release"));
    assert!(pin.contains(&landed_change));

    nit(
        workspace,
        ["pin", "release-copy", "--change", &landed_change],
    )
    .success()
    .stdout(predicate::str::contains("created Pin PIN-"));
}

#[test]
fn adopt_rejects_plain_subdirectory() {
    let fixture = clean_workspace_with_sdk();
    let workspace = fixture.root.as_path();
    // A plain subdirectory of the root repo is not its own repository; adopting
    // it must fail rather than register a non-repo path as a member.
    std::fs::create_dir_all(workspace.join("plainsub")).unwrap();
    nit(workspace, ["adopt", "plainsub"])
        .failure()
        .stderr(predicate::str::contains("not a repository root"));
}

#[test]
fn ignore_and_doctor_repair_root_excludes() {
    let fixture = clean_workspace_with_sdk();
    let workspace = fixture.root.as_path();

    nit(workspace, ["ignore", "scratch"])
        .success()
        .stdout(predicate::str::contains("updated ignored paths"));
    let roster = std::fs::read_to_string(workspace.join(".nit/roster.yaml")).unwrap();
    assert!(roster.contains("ignored:"));
    assert!(roster.contains("scratch"));

    std::fs::write(workspace.join(".git/info/exclude"), "# reset by clone\n").unwrap();
    nit(workspace, ["doctor"])
        .success()
        .stdout(predicate::str::contains("exclude repair: ok"));

    let exclude = std::fs::read_to_string(workspace.join(".git/info/exclude")).unwrap();
    assert!(exclude.lines().any(|line| line == "vendor/sdk"));
    assert!(exclude.lines().any(|line| line == "scratch"));
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
    nit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    nit(workspace, ["land", "baseline", "-m", "Publish sdk update"]).success();
    nit(workspace, ["push"]).success();
    nit(workspace, ["push", "--resume"])
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
    nit(fixture._temp.path(), ["clone", root_remote, hydrated_path])
        .success()
        .stdout(predicate::str::contains("cloned sdk"))
        .stdout(predicate::str::contains("cloned Nit workspace"));
    assert_eq!(
        std::fs::read_to_string(hydrated.join("vendor/sdk/lib.rs")).unwrap(),
        "pub fn sdk() -> &'static str { \"pushed\" }\n"
    );

    let restored = fixture._temp.path().join("restored");
    let restored_path = restored.to_str().unwrap();
    nit(
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
    nit(&restored, ["checkout", "baseline"])
        .failure()
        .stderr(predicate::str::contains("use --exact to reset it"));
    nit(&restored, ["checkout", "baseline", "--exact"]).success();
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
    nit(workspace, ["add", "-A"]);
    nit(workspace, ["land", "release", "-m", "Publish v2"]).success();

    advance_remote(
        fixture._temp.path(),
        &fixture.app_remote,
        "app-remote-advance",
        "remote.txt",
        "remote app change\n",
    );

    nit(workspace, ["push"])
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

    nit(workspace, ["push", "--resume"])
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
    nit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    nit(workspace, ["land", "baseline", "-m", "Publish sdk update"]).success();
    let pinned = git_out(&sdk, ["rev-parse", "HEAD"]);

    git(&sdk, ["reset", "--hard", "HEAD~1"]);
    std::fs::write(
        sdk.join("lib.rs"),
        "pub fn sdk() -> &'static str { \"rewritten\" }\n",
    )
    .unwrap();
    git(&sdk, ["add", "lib.rs"]);
    git(&sdk, ["commit", "-m", "Rewrite sdk update"]);

    nit(workspace, ["push"])
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
    git(workspace, ["add", "-A", ".nit/pins"]);
    git(workspace, ["commit", "-m", "Remove orphaned pin"]);
    nit(workspace, ["pin", "recovered"]).success();

    nit(workspace, ["push", "--resume"])
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
fn push_ignores_retained_pins_for_retired_missing_members() {
    let fixture = workspace_with_remotes();
    let workspace = fixture.root.as_path();
    let sdk = workspace.join("vendor/sdk");

    nit(workspace, ["pin", "baseline"]).success();
    std::fs::write(
        workspace.join(".nit/roster.yaml"),
        "version: 1\nmode: shared\nmembers: []\n",
    )
    .unwrap();
    git(workspace, ["add", ".nit/roster.yaml"]);
    git(workspace, ["commit", "-m", "Retire sdk"]);
    std::fs::remove_dir_all(&sdk).unwrap();

    nit(workspace, ["push"])
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
    nit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    nit(workspace, ["land", "baseline", "-m", "Publish sdk update"]).success();
    nit(workspace, ["push"]).success();

    let root_remote = fixture.root_remote.to_str().unwrap();
    let restored = fixture._temp.path().join("branch-restored");
    let restored_path = restored.to_str().unwrap();
    nit(
        fixture._temp.path(),
        ["clone", root_remote, restored_path, "--pin", "baseline"],
    )
    .success();

    let sdk = restored.join("vendor/sdk");
    git(&sdk, ["checkout", "--detach", "HEAD"]);
    git(&sdk, ["branch", "-D", "master"]);

    nit(&restored, ["checkout", "baseline"])
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
    nit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    nit(workspace, ["land", "baseline", "-m", "Publish sdk update"]).success();
    nit(workspace, ["push"]).success();

    let root_remote = fixture.root_remote.to_str().unwrap();
    let restored = fixture._temp.path().join("hint-restored");
    let restored_path = restored.to_str().unwrap();
    nit(
        fixture._temp.path(),
        ["clone", root_remote, restored_path, "--pin", "baseline"],
    )
    .success();

    let sdk = restored.join("vendor/sdk");
    git(&sdk, ["checkout", "-b", "topic"]);
    git(&sdk, ["branch", "-D", "master"]);

    nit(&restored, ["checkout", "baseline"])
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
    nit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    nit(workspace, ["land", "baseline", "-m", "Publish sdk update"]).success();
    nit(workspace, ["push"]).success();

    let root_remote = fixture.root_remote.to_str().unwrap();
    let restored = fixture._temp.path().join("ff-restored");
    let restored_path = restored.to_str().unwrap();
    nit(fixture._temp.path(), ["clone", root_remote, restored_path]).success();

    let sdk = restored.join("vendor/sdk");
    let remote_head = git_out(&sdk, ["rev-parse", "origin/master"]);
    git(&sdk, ["reset", "--hard", "HEAD~1"]);

    nit(&restored, ["checkout", "baseline"])
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
    nit(workspace, ["pin", "baseline"]).success();

    std::fs::write(
        sdk.join("lib.rs"),
        "pub fn sdk() -> &'static str { \"ahead\" }\n",
    )
    .unwrap();
    git(&sdk, ["add", "lib.rs"]);
    git(&sdk, ["commit", "-m", "Ahead sdk"]);
    let branch_head = git_out(&sdk, ["rev-parse", "master"]);

    nit(workspace, ["checkout", "baseline"])
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
    nit(workspace, ["add", "--repo", "sdk", "lib.rs"]);
    nit(workspace, ["land", "baseline", "-m", "Publish sdk update"]).success();
    nit(workspace, ["push"]).success();

    let root_remote = fixture.root_remote.to_str().unwrap();
    let restored = fixture._temp.path().join("diverged-restored");
    let restored_path = restored.to_str().unwrap();
    nit(fixture._temp.path(), ["clone", root_remote, restored_path]).success();

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

    nit(&restored, ["checkout", "baseline"])
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
    nit(workspace, ["pin", "baseline"]).success();

    std::fs::write(
        sdk.join("lib.rs"),
        "pub fn sdk() -> &'static str { \"later\" }\n",
    )
    .unwrap();
    git(&sdk, ["add", "lib.rs"]);
    git(&sdk, ["commit", "-m", "Later sdk"]);
    let branch_head = git_out(&sdk, ["rev-parse", "master"]);

    nit(workspace, ["checkout", "baseline", "--exact"])
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
    git(&workspace, ["init"]);
    git(&workspace, ["config", "user.email", "nit-test@example.com"]);
    git(&workspace, ["config", "user.name", "Nit Test"]);
    std::fs::write(workspace.join("README.md"), "root\n").unwrap();
    git(&workspace, ["add", "README.md"]);
    git(&workspace, ["commit", "-m", "Initial root"]);

    let sub_source = temp.path().join("sub-source");
    std::fs::create_dir(&sub_source).unwrap();
    git(&sub_source, ["init"]);
    git(
        &sub_source,
        ["config", "user.email", "nit-test@example.com"],
    );
    git(&sub_source, ["config", "user.name", "Nit Test"]);
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

    nit(&workspace, ["init"]).success();
    nit(
        &workspace,
        ["import-submodule", "vendor/sub", "--id", "sub"],
    )
    .success()
    .stdout(predicate::str::contains("imported submodule vendor/sub"));

    let roster = std::fs::read_to_string(workspace.join(".nit/roster.yaml")).unwrap();
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
    assert_eq!(last_commit.trim(), "Import Nit member vendor/sub");
}

#[test]
fn status_reports_member_state_and_discovered() {
    let fixture = clean_workspace_with_sdk();
    let ws = fixture.root.as_path();
    std::fs::write(ws.join("vendor/sdk/new.txt"), "x\n").unwrap();
    std::fs::create_dir_all(ws.join("scratch")).unwrap();
    git(&ws.join("scratch"), ["init"]);

    nit(ws, ["status"])
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
    nit(ws, ["add", "vendor/sdk/lib.rs"]);
    nit(ws, ["land", "release", "-m", "Ship it"]).success();

    nit(ws, ["log"])
        .success()
        .stdout(predicate::str::contains("change NCH-"))
        .stdout(predicate::str::contains("pin    release"));
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
    nit(ws, ["status"]).success();
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

    let output = nit_output(ws, ["status"]);
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

fn git<const N: usize>(dir: &Path, args: [&str; N]) {
    let status = std::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .unwrap();
    assert!(
        status.success(),
        "git {:?} failed in {}",
        args,
        dir.display()
    );
}

fn git_args(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .unwrap();
    assert!(
        status.success(),
        "git {:?} failed in {}",
        args,
        dir.display()
    );
}

fn git_out<const N: usize>(dir: &Path, args: [&str; N]) -> String {
    let output = std::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
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
    let output = std::process::Command::new("git")
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

fn tempdir_without_nit_ancestor() -> tempfile::TempDir {
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
        if !base.is_dir() || has_nit_ancestor(&base) {
            continue;
        }
        for _ in 0..8 {
            if let Ok(temp) = tempfile::Builder::new()
                .prefix("nit-outside-workspace-")
                .tempdir_in(&base)
            {
                if !has_nit_ancestor(temp.path()) {
                    return temp;
                }
            }
        }
    }

    panic!("could not create a tempdir without a .nit ancestor");
}

fn has_nit_ancestor(path: &Path) -> bool {
    path.ancestors()
        .any(|ancestor| ancestor.join(".nit").exists())
}

fn nit_output<const N: usize>(dir: &Path, args: [&str; N]) -> String {
    let output = Command::cargo_bin("nit")
        .unwrap()
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "nit {:?} failed in {}: {}",
        args,
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn clean_workspace_with_sdk() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    git(&root, ["init"]);
    git(&root, ["config", "user.email", "nit-test@example.com"]);
    git(&root, ["config", "user.name", "Nit Test"]);
    std::fs::write(root.join("README.md"), "root\n").unwrap();
    git(&root, ["add", "README.md"]);
    git(&root, ["commit", "-m", "Initial root"]);

    std::fs::create_dir_all(root.join("vendor/sdk")).unwrap();
    git(&root.join("vendor/sdk"), ["init"]);
    git(
        &root.join("vendor/sdk"),
        ["config", "user.email", "nit-test@example.com"],
    );
    git(
        &root.join("vendor/sdk"),
        ["config", "user.name", "Nit Test"],
    );
    std::fs::write(root.join("vendor/sdk/lib.rs"), "pub fn sdk() {}\n").unwrap();
    git(&root.join("vendor/sdk"), ["add", "lib.rs"]);
    git(&root.join("vendor/sdk"), ["commit", "-m", "Initial sdk"]);

    nit(&root, ["init"]);
    nit(&root, ["adopt", "vendor/sdk", "--id", "sdk"]);

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
    git(&root, ["init"]);
    git(&root, ["config", "user.email", "nit-test@example.com"]);
    git(&root, ["config", "user.name", "Nit Test"]);
    git(
        &root,
        ["remote", "add", "origin", root_remote.to_str().unwrap()],
    );
    std::fs::write(root.join("README.md"), "root\n").unwrap();
    git(&root, ["add", "README.md"]);
    git(&root, ["commit", "-m", "Initial root"]);
    git(&root, ["push", "origin", "HEAD"]);

    std::fs::create_dir_all(root.join("vendor/sdk")).unwrap();
    git(&root.join("vendor/sdk"), ["init"]);
    git(
        &root.join("vendor/sdk"),
        ["config", "user.email", "nit-test@example.com"],
    );
    git(
        &root.join("vendor/sdk"),
        ["config", "user.name", "Nit Test"],
    );
    git(
        &root.join("vendor/sdk"),
        ["remote", "add", "origin", sdk_remote.to_str().unwrap()],
    );
    std::fs::write(root.join("vendor/sdk/lib.rs"), "pub fn sdk() {}\n").unwrap();
    git(&root.join("vendor/sdk"), ["add", "lib.rs"]);
    git(&root.join("vendor/sdk"), ["commit", "-m", "Initial sdk"]);
    git(&root.join("vendor/sdk"), ["push", "origin", "HEAD"]);

    nit(&root, ["init"]);
    nit(&root, ["adopt", "vendor/sdk", "--id", "sdk"]);

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
    git(&root, ["init"]);
    git(&root, ["config", "user.email", "nit-test@example.com"]);
    git(&root, ["config", "user.name", "Nit Test"]);
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

    nit(&root, ["init"]);
    nit(&root, ["adopt", "sdk", "--id", "sdk"]);
    nit(&root, ["adopt", "app", "--id", "app"]);
    nit(&root, ["adopt", "docs", "--id", "docs"]);

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
    git(&member, ["init"]);
    git(&member, ["config", "user.email", "nit-test@example.com"]);
    git(&member, ["config", "user.name", "Nit Test"]);
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
    git(&clone, ["config", "user.email", "nit-test@example.com"]);
    git(&clone, ["config", "user.name", "Nit Test"]);
    std::fs::write(clone.join(file), content).unwrap();
    git(&clone, ["add", file]);
    git(&clone, ["commit", "-m", "Advance remote"]);
    git(&clone, ["push", "origin", "HEAD"]);
}

fn remove_pins_with_label(root: &Path, label: &str) {
    let pins_dir = root.join(".nit/pins");
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
        let mut command = Command::cargo_bin("nit").unwrap();
        command
            .args(args)
            .current_dir(&self.home)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("NIT_DATA_DIR", &self.data)
            .env("XDG_DATA_HOME", self.home.join(".xdg-data"))
            .env("GROK_HOME", &self.grok_home)
            .env("NIT_NO_UPKEEP", "true");
        command
    }

    fn managed_skill(&self) -> PathBuf {
        self.data.join("skills/nit")
    }

    fn claude_skill(&self) -> PathBuf {
        self.home.join(".claude/skills/nit")
    }

    fn codex_skill(&self) -> PathBuf {
        self.home.join(".codex/skills/nit")
    }

    fn opencode_skill(&self) -> PathBuf {
        self.home.join(".opencode/skills/nit")
    }

    fn grok_skill(&self) -> PathBuf {
        self.grok_home.join("skills/nit")
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

fn nit<const N: usize>(dir: &Path, args: [&str; N]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("nit")
        .unwrap()
        .args(args)
        .current_dir(dir)
        .assert()
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
