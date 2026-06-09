use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn change_id() -> String {
    format!("NCH-{}-{}", current_millis(), disambiguator())
}

pub fn pin_id(label: Option<&str>) -> String {
    let base = format!("PIN-{}-{}", current_millis(), disambiguator());
    match label.map(sanitize_label).filter(|label| !label.is_empty()) {
        Some(label) => format!("{base}-{label}"),
        None => base,
    }
}

fn current_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_millis()
}

/// A compact tiebreaker for ids minted within the same millisecond. The process
/// id separates concurrent `nit` invocations; the per-process counter separates
/// ids minted back-to-back inside one invocation. Both are rendered in
/// minimal-width hex (no zero padding), and the counter — almost always 0 for a
/// single-id invocation — is omitted entirely when it is 0. A typical id is
/// therefore just `NCH-<millis>-<pid hex>` (e.g. `NCH-1781013904682-72e5`); only
/// a burst that mints several ids in one process grows a `-<counter hex>` suffix.
/// The `-` before the counter keeps the (pid, counter) encoding unambiguous.
fn disambiguator() -> String {
    let counter = NEXT_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = process::id();
    if counter == 0 {
        format!("{pid:x}")
    } else {
        format!("{pid:x}-{counter:x}")
    }
}

fn sanitize_label(label: &str) -> String {
    label
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn change_ids_include_disambiguator_after_millis() {
        let id = change_id();
        let rest = id.strip_prefix("NCH-").expect("change id starts with NCH-");
        let (millis, disambiguator) = rest
            .split_once('-')
            .expect("change id has millis and disambiguator segments");

        assert!(millis.parse::<u128>().is_ok());
        // The disambiguator leads with the process id in minimal-width hex (no
        // zero padding), optionally followed by `-<counter hex>` in a burst.
        let pid_token = disambiguator.split('-').next().expect("pid token");
        assert!(!pid_token.is_empty());
        assert!(pid_token.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert!(!pid_token.starts_with('0'), "pid hex should not be padded");
    }

    #[test]
    fn pin_ids_keep_millis_parseable_and_sanitize_label() {
        let id = pin_id(Some("Release 2026.06"));
        let rest = id.strip_prefix("PIN-").expect("pin id starts with PIN-");
        let millis = rest.split('-').next().expect("millis segment");

        assert!(id.starts_with("PIN-"));
        assert!(id.ends_with("-release-2026-06"));
        assert!(millis.parse::<u128>().is_ok());
    }

    #[test]
    fn ids_do_not_collapse_under_burst_generation() {
        let mut ids = BTreeSet::new();
        for _ in 0..128 {
            ids.insert(change_id());
            ids.insert(pin_id(None));
        }

        assert_eq!(ids.len(), 256);
    }
}
