use assert_cmd::Command;
use predicates::prelude::*;
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
    let temp = tempfile::tempdir().unwrap();
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
        .stdout(predicate::str::contains("member sdk already pushed"))
        .stdout(predicate::str::contains("workspace root already pushed"));

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
