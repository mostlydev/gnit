use assert_cmd::Command;
use predicates::prelude::*;

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
fn init_and_adopt_existing_repo() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path();
    std::fs::create_dir(workspace.join("app")).unwrap();

    std::process::Command::new("git")
        .args(["init"])
        .current_dir(workspace.join("app"))
        .status()
        .unwrap();

    Command::cargo_bin("nit")
        .unwrap()
        .arg("init")
        .current_dir(workspace)
        .assert()
        .success()
        .stdout(predicate::str::contains("initialized Nit workspace"));

    Command::cargo_bin("nit")
        .unwrap()
        .args(["adopt", "app"])
        .current_dir(workspace)
        .assert()
        .success()
        .stdout(predicate::str::contains("adopted app"));

    Command::cargo_bin("nit")
        .unwrap()
        .arg("status")
        .current_dir(workspace)
        .assert()
        .success()
        .stdout(predicate::str::contains("Members:"))
        .stdout(predicate::str::contains("app  app"));
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
