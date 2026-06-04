use std::env;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};

const REPO: &str = "mostlydev/nit";
const INSTALLER_URL: &str = "https://raw.githubusercontent.com/mostlydev/nit/master/install.sh";
const UPDATE_CHECK_URL: &str = "https://api.github.com/repos/mostlydev/nit/releases/latest";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const NOTICE_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
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

pub fn maybe_print_update_notice(verbose: bool) {
    let Some(path) = cache_path() else {
        return;
    };
    let now = now_secs();
    let cache = read_cache(&path).ok();
    let plan = notice_plan(
        cache.as_ref(),
        env!("CARGO_PKG_VERSION"),
        now,
        NoticeEnvironment {
            official_build: BUILD_COMMIT != "dev",
            stdout_tty: std::io::stdout().is_terminal(),
            ci: env_flag("CI"),
            upkeep_disabled: env_flag("NIT_NO_UPKEEP"),
            notice_disabled: env_flag("NIT_NO_UPDATE_NOTICE"),
        },
    );

    if plan.print_notice {
        if let Some(cache) = cache.as_ref() {
            if let Some(latest) = cache.latest_version.as_deref() {
                eprintln!(
                    "nit {latest} is available (current {}); run `nit update`.",
                    env!("CARGO_PKG_VERSION")
                );
                let mut updated = cache.clone();
                updated.last_notified_at = Some(now);
                let _ = write_cache(&path, &updated);
            }
        }
    }

    if plan.refresh {
        if let Err(err) = spawn_background_check() {
            if verbose {
                eprintln!("nit update notice: {err}");
            }
        } else if verbose {
            eprintln!("nit update notice: scheduled background refresh");
        }
    }
}

pub fn check() -> Result<()> {
    match refresh_notice_cache() {
        Ok(cache) => {
            print_check_result(&cache);
            Ok(())
        }
        Err(err) => {
            eprintln!("nit update check unavailable: {err}");
            Ok(())
        }
    }
}

fn refresh_notice_cache() -> Result<UpdateCache> {
    let url = env::var("NIT_UPDATE_CHECK_URL").unwrap_or_else(|_| UPDATE_CHECK_URL.to_string());
    let timeout = env::var("NIT_UPDATE_CHECK_TIMEOUT_SECS").unwrap_or_else(|_| "2".to_string());
    let curl = find_command("curl").context("find curl")?;
    let output = Command::new(curl)
        .args(["-fsSL", "--max-time", timeout.as_str(), url.as_str()])
        .output()
        .context("fetch latest release metadata")?;
    if !output.status.success() {
        bail!("curl failed with status {}", output.status);
    }
    let body = String::from_utf8(output.stdout).context("release metadata was not utf-8")?;
    let tag = extract_tag_name(&body).context("release metadata did not contain tag_name")?;
    let version = version_from_tag(&tag).context("release tag did not contain a version")?;

    let path = cache_path().context("update cache path unavailable")?;
    let previous = read_cache(&path).ok();
    let last_notified_at = previous
        .as_ref()
        .filter(|cache| cache.latest_version.as_deref() == Some(version.as_str()))
        .and_then(|cache| cache.last_notified_at);
    let cache = UpdateCache {
        checked_at: Some(now_secs()),
        latest_tag: Some(tag),
        latest_version: Some(version),
        last_notified_at,
    };
    write_cache(&path, &cache)?;
    Ok(cache)
}

fn print_check_result(cache: &UpdateCache) {
    let current = env!("CARGO_PKG_VERSION");
    let latest = cache.latest_version.as_deref().unwrap_or(current);
    if is_newer_version(latest, current) {
        println!("nit {latest} is available (current {current}); run `nit update`.");
    } else {
        println!("nit is up to date (current {current}; latest {latest}).");
    }
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

#[derive(Clone, Default)]
struct UpdateCache {
    checked_at: Option<u64>,
    latest_tag: Option<String>,
    latest_version: Option<String>,
    last_notified_at: Option<u64>,
}

#[derive(Clone, Copy)]
struct NoticeEnvironment {
    official_build: bool,
    stdout_tty: bool,
    ci: bool,
    upkeep_disabled: bool,
    notice_disabled: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct NoticePlan {
    print_notice: bool,
    refresh: bool,
}

impl NoticePlan {
    fn quiet() -> Self {
        Self {
            print_notice: false,
            refresh: false,
        }
    }
}

fn notice_plan(
    cache: Option<&UpdateCache>,
    current_version: &str,
    now: u64,
    env: NoticeEnvironment,
) -> NoticePlan {
    if !env.official_build
        || !env.stdout_tty
        || env.ci
        || env.upkeep_disabled
        || env.notice_disabled
    {
        return NoticePlan::quiet();
    }

    let print_notice = cache.is_some_and(|cache| {
        cache
            .latest_version
            .as_deref()
            .is_some_and(|latest| is_newer_version(latest, current_version))
            && notice_due(cache.last_notified_at, now)
    });
    let refresh = cache.is_none_or(|cache| check_due(cache.checked_at, now));

    NoticePlan {
        print_notice,
        refresh,
    }
}

fn cache_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("NIT_UPDATE_CACHE_PATH") {
        return Some(PathBuf::from(path));
    }
    if let Some(base) = env::var_os("XDG_CACHE_HOME").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(base).join("nit/update-check"));
    }
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache/nit/update-check"))
}

fn read_cache(path: &Path) -> Result<UpdateCache> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut cache = UpdateCache::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "checked_at" => cache.checked_at = optional_timestamp(value),
            "latest_tag" => cache.latest_tag = optional_string(value),
            "latest_version" => cache.latest_version = optional_string(value),
            "last_notified_at" => cache.last_notified_at = optional_timestamp(value),
            _ => {}
        }
    }
    Ok(cache)
}

fn write_cache(path: &Path, cache: &UpdateCache) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let text = format!(
        "checked_at={}\nlatest_tag={}\nlatest_version={}\nlast_notified_at={}\n",
        cache.checked_at.unwrap_or(0),
        cache.latest_tag.as_deref().unwrap_or(""),
        cache.latest_version.as_deref().unwrap_or(""),
        cache.last_notified_at.unwrap_or(0)
    );
    fs::write(path, text).with_context(|| format!("write {}", path.display()))
}

fn spawn_background_check() -> Result<()> {
    let exe = env::current_exe().context("find current executable")?;
    Command::new(exe)
        .args(["--no-upkeep", "update", "--check"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("spawn update check")?;
    Ok(())
}

fn check_due(checked_at: Option<u64>, now: u64) -> bool {
    checked_at
        .and_then(|checked_at| now.checked_sub(checked_at))
        .is_none_or(|age| age >= CHECK_INTERVAL.as_secs())
}

fn notice_due(last_notified_at: Option<u64>, now: u64) -> bool {
    last_notified_at
        .and_then(|notified_at| now.checked_sub(notified_at))
        .is_none_or(|age| age >= NOTICE_INTERVAL.as_secs())
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .map(|value| {
            let value = value.to_ascii_lowercase();
            matches!(value.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

fn extract_tag_name(json: &str) -> Option<String> {
    let value = serde_yaml::from_str::<serde_yaml::Value>(json).ok()?;
    let key = serde_yaml::Value::String("tag_name".to_string());
    value
        .as_mapping()?
        .get(&key)?
        .as_str()
        .and_then(optional_string)
}

fn optional_timestamp(value: &str) -> Option<u64> {
    value
        .trim()
        .parse()
        .ok()
        .filter(|timestamp| *timestamp != 0)
}

fn optional_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn version_from_tag(tag: &str) -> Option<String> {
    let version = tag.strip_prefix('v').unwrap_or(tag);
    (!version.is_empty()).then(|| version.to_string())
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    let latest = version_parts(latest);
    let current = version_parts(current);
    for i in 0..latest.len().max(current.len()) {
        let left = latest.get(i).copied().unwrap_or(0);
        let right = current.get(i).copied().unwrap_or(0);
        if left != right {
            return left > right;
        }
    }
    false
}

fn version_parts(version: &str) -> Vec<u64> {
    version
        .strip_prefix('v')
        .unwrap_or(version)
        .split('.')
        .map(|part| {
            let digits = part
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>();
            digits.parse().unwrap_or(0)
        })
        .collect()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_latest_release_tag() {
        let json = r#"{"html_url":"https://example.test","tag_name":"v0.2.10"}"#;
        assert_eq!(extract_tag_name(json).as_deref(), Some("v0.2.10"));
        assert_eq!(version_from_tag("v0.2.10").as_deref(), Some("0.2.10"));
    }

    #[test]
    fn parses_top_level_release_tag_only() {
        let json = r#"{"body":"old payload with \"tag_name\":\"v9.9.9\"","tag_name":"v0.3.1"}"#;
        assert_eq!(extract_tag_name(json).as_deref(), Some("v0.3.1"));
    }

    #[test]
    fn cache_read_drops_empty_strings_and_zero_timestamps() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("update-cache");
        std::fs::write(
            &path,
            "checked_at=0\nlatest_tag=\nlatest_version=\nlast_notified_at=0\n",
        )
        .unwrap();

        let cache = read_cache(&path).unwrap();
        assert_eq!(cache.checked_at, None);
        assert_eq!(cache.latest_tag, None);
        assert_eq!(cache.latest_version, None);
        assert_eq!(cache.last_notified_at, None);
    }

    #[test]
    fn compares_dotted_versions_numerically() {
        assert!(is_newer_version("0.2.10", "0.2.2"));
        assert!(is_newer_version("1.0.0", "0.9.9"));
        assert!(!is_newer_version("0.2.2", "0.2.10"));
        assert!(!is_newer_version("0.2.2", "0.2.2"));
    }

    #[test]
    fn notice_plan_prints_cached_update_and_refreshes_stale_cache() {
        let cache = test_cache("9.9.9", Some(1), None);
        assert_eq!(
            notice_plan(
                Some(&cache),
                "0.3.0",
                CHECK_INTERVAL.as_secs() + 1,
                test_env()
            ),
            NoticePlan {
                print_notice: true,
                refresh: true,
            }
        );
    }

    #[test]
    fn notice_plan_is_silent_for_fresh_current_cache() {
        let cache = test_cache("0.3.0", Some(1), None);
        assert_eq!(
            notice_plan(Some(&cache), "0.3.0", 2, test_env()),
            NoticePlan::quiet()
        );
    }

    #[test]
    fn notice_plan_suppresses_risky_or_non_interactive_contexts() {
        let cache = test_cache("9.9.9", Some(1), None);
        let now = CHECK_INTERVAL.as_secs() + 1;

        for env in [
            NoticeEnvironment {
                official_build: false,
                ..test_env()
            },
            NoticeEnvironment {
                stdout_tty: false,
                ..test_env()
            },
            NoticeEnvironment {
                ci: true,
                ..test_env()
            },
            NoticeEnvironment {
                upkeep_disabled: true,
                ..test_env()
            },
            NoticeEnvironment {
                notice_disabled: true,
                ..test_env()
            },
        ] {
            assert_eq!(
                notice_plan(Some(&cache), "0.3.0", now, env),
                NoticePlan::quiet()
            );
        }
    }

    fn test_env() -> NoticeEnvironment {
        NoticeEnvironment {
            official_build: true,
            stdout_tty: true,
            ci: false,
            upkeep_disabled: false,
            notice_disabled: false,
        }
    }

    fn test_cache(
        latest_version: &str,
        checked_at: Option<u64>,
        last_notified_at: Option<u64>,
    ) -> UpdateCache {
        UpdateCache {
            checked_at,
            latest_tag: Some(format!("v{latest_version}")),
            latest_version: Some(latest_version.to_string()),
            last_notified_at,
        }
    }
}
