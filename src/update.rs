use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

const REPO: &str = "mostlydev/nit";
const INSTALLER_URL: &str = "https://raw.githubusercontent.com/mostlydev/nit/master/install.sh";
const BUILD_COMMIT: &str = match option_env!("NIT_COMMIT") {
    Some(commit) => commit,
    None => "dev",
};

pub fn run(dry_run: bool, force: bool) -> Result<()> {
    println!("Nit updates install from GitHub Releases for {REPO}.");
    println!("Installer: {INSTALLER_URL}");
    println!("Current build commit: {BUILD_COMMIT}");

    if dry_run {
        println!("dry run: would run `curl -sSL {INSTALLER_URL} | sh`");
        return Ok(());
    }

    if BUILD_COMMIT == "dev" && !force {
        bail!(
            "refusing to replace a dev build; rerun with `nit update --force` to use the release installer"
        );
    }

    let sh = find_command("sh").context("find sh")?;
    let mut curl = Command::new("curl")
        .args(["-sSL", INSTALLER_URL])
        .stdout(Stdio::piped())
        .spawn()
        .context("start curl")?;

    let curl_stdout = curl.stdout.take().context("capture curl stdout")?;
    let mut installer = Command::new(sh)
        .stdin(Stdio::from(curl_stdout))
        .spawn()
        .context("start installer")?;

    let curl_status = curl.wait().context("wait for curl")?;
    if !curl_status.success() {
        bail!("curl failed with status {curl_status}");
    }

    let installer_status = installer.wait().context("wait for installer")?;
    if !installer_status.success() {
        bail!("installer failed with status {installer_status}");
    }

    Ok(())
}

fn find_command(name: &str) -> Result<PathBuf> {
    let path = env::var_os("PATH").context("PATH is not set")?;
    for dir in env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    bail!("{name} not found in PATH")
}
