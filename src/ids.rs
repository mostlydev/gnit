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

fn disambiguator() -> String {
    let counter = NEXT_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:08x}{counter:016x}", process::id())
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
        let parts = id.split('-').collect::<Vec<_>>();

        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "NCH");
        assert!(parts[1].parse::<u128>().is_ok());
        assert_eq!(parts[2].len(), 24);
        assert!(parts[2].chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn pin_ids_keep_millis_parseable_and_sanitize_label() {
        let id = pin_id(Some("Release 2026.06"));
        let parts = id.split('-').collect::<Vec<_>>();

        assert!(id.starts_with("PIN-"));
        assert!(id.ends_with("-release-2026-06"));
        assert!(parts[1].parse::<u128>().is_ok());
        assert_eq!(parts[2].len(), 24);
        assert!(parts[2].chars().all(|ch| ch.is_ascii_hexdigit()));
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
