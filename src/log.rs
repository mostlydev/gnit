use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fs;

use anyhow::{Context, Result};

use crate::git;
use crate::metadata::{Pin, Roster, PINS_DIR};
use crate::workspace;

const TRAILER: &str = "Nit-Change-Id";

/// Unified, interleaved workspace timeline of Changes and Pins, newest first.
/// Changes are reconstructed from `Nit-Change-Id` trailers across member repos;
/// Pins are read from `.nit/pins/`. This is the operator's retrievable shared
/// graph as a single command.
pub fn workspace_log() -> Result<()> {
    let cwd = env::current_dir()?;
    let root = workspace::find_nit_workspace(&cwd)
        .context("not in a Nit workspace; run `nit init` first")?;
    let roster = Roster::read(&root)?;

    // Aggregate changes across repos by Change-Id.
    struct Change {
        time: i64,
        subject: String,
        repos: BTreeSet<String>,
    }
    let mut changes: HashMap<String, Change> = HashMap::new();

    let mut repos: Vec<(String, std::path::PathBuf)> = Vec::new();
    if git::is_git_repo_root(&root) {
        repos.push(("root".to_string(), root.clone()));
    }
    for member in &roster.members {
        repos.push((member.id.clone(), root.join(&member.path)));
    }

    for (repo_id, repo_root) in &repos {
        if !git::is_git_repo_root(repo_root) {
            continue;
        }
        let log = git::output_in_args(repo_root, ["log", "--all", "--format=%ct%x1f%s%x1f%B%x1e"])
            .unwrap_or_default();
        for record in log.split('\x1e') {
            let record = record.trim();
            if record.is_empty() {
                continue;
            }
            let mut fields = record.splitn(3, '\x1f');
            let (Some(ct), Some(subject), Some(body)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let Some(change_id) = trailer_value(body) else {
                continue;
            };
            let time: i64 = ct.trim().parse().unwrap_or(0);
            let entry = changes.entry(change_id).or_insert(Change {
                time: 0,
                subject: String::new(),
                repos: BTreeSet::new(),
            });
            entry.repos.insert(repo_id.clone());
            if time >= entry.time {
                entry.time = time;
                entry.subject = subject.trim().to_string();
            }
        }
    }

    // Build the merged entry list.
    let mut entries: Vec<(i64, String)> = Vec::new();
    for (id, change) in changes {
        entries.push((
            change.time,
            format!(
                "change {id}  ({} repo{})  {}",
                change.repos.len(),
                plural(change.repos.len()),
                change.subject
            ),
        ));
    }
    for pin in all_pins(&root)? {
        let label = pin.label.clone().unwrap_or_else(|| pin.id.clone());
        entries.push((
            pin_time(&pin),
            format!(
                "pin    {label}  ({} member{})",
                pin.members.len(),
                plural(pin.members.len())
            ),
        ));
    }

    if entries.is_empty() {
        println!("No Nit changes or pins yet.");
        return Ok(());
    }

    entries.sort_by(|a, b| b.0.cmp(&a.0));
    for (time, line) in entries {
        println!("{}  {line}", date(time));
    }
    Ok(())
}

fn all_pins(root: &std::path::Path) -> Result<Vec<Pin>> {
    let pins_dir = root.join(PINS_DIR);
    if !pins_dir.exists() {
        return Ok(Vec::new());
    }
    let mut pins = Vec::new();
    for entry in fs::read_dir(&pins_dir).with_context(|| format!("read {}", pins_dir.display()))? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
            if let Some(id) = path.file_stem().and_then(|s| s.to_str()) {
                pins.push(Pin::read(root, id)?);
            }
        }
    }
    Ok(pins)
}

/// Pin ids embed creation millis as the segment after the `PIN-` prefix.
fn pin_time(pin: &Pin) -> i64 {
    pin.id
        .strip_prefix("PIN-")
        .and_then(|rest| rest.split('-').next())
        .and_then(|millis| millis.parse::<i64>().ok())
        .map(|millis| millis / 1000)
        .unwrap_or(0)
}

fn trailer_value(body: &str) -> Option<String> {
    body.lines()
        .rev()
        .find_map(|line| line.strip_prefix(&format!("{TRAILER}: ")))
        .map(|value| value.trim().to_string())
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Format a Unix timestamp as a UTC `YYYY-MM-DD` date with no external crates
/// (Howard Hinnant's civil-from-days algorithm).
fn date(secs: i64) -> String {
    if secs <= 0 {
        return "----------".to_string();
    }
    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::Pin;

    #[test]
    fn pin_time_reads_millis_before_id_disambiguator() {
        let pin = Pin::new("PIN-1760000000123-0000002a0000000000000007-baseline");

        assert_eq!(pin_time(&pin), 1_760_000_000);
    }
}
