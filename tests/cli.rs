use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;

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
        .stdout(predicate::str::contains("Members:"))
        .stdout(predicate::str::contains("sdk  vendor/sdk"));

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
