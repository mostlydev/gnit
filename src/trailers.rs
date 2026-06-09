//! Shared parsing for the `Gnit-Change-Id` commit trailer.
//!
//! Implements git's trailer-block rule — trailers live only in the final
//! paragraph of the message — instead of substring-matching every body line,
//! plus strict validation of the id format minted by `ids::change_id`.

/// The trailer key written by `gnit commit` and parsed everywhere else.
pub const TRAILER: &str = "Gnit-Change-Id";

/// Extract the change id from a raw commit message (`git log --format=%B`).
///
/// Rules:
/// - only the final paragraph of the message is considered (git trailer block);
/// - the key must start the line, spelled exactly `Gnit-Change-Id`, followed by
///   `:` and optional whitespace (`Gnit-Change-Id:GCH-…` is valid);
/// - the value must be a well-formed change id (`GCH-<millis>-<hex>[-<hex>]`);
/// - when several trailers are present, the last one wins.
pub fn change_id(message: &str) -> Option<String> {
    let lines: Vec<&str> = message.lines().collect();
    // The trailer block is the last paragraph: everything after the final
    // blank line, ignoring trailing blank lines.
    let end = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .map(|index| index + 1)
        .unwrap_or(0);
    let start = lines[..end]
        .iter()
        .rposition(|line| line.trim().is_empty())
        .map(|index| index + 1)
        .unwrap_or(0);

    let mut found = None;
    for line in &lines[start..end] {
        let Some(rest) = line.strip_prefix(TRAILER) else {
            continue;
        };
        let Some(value) = rest.strip_prefix(':') else {
            continue;
        };
        let value = value.trim();
        if is_valid_change_id(value) {
            found = Some(value.to_string());
        }
    }
    found
}

/// Strict validation of the id shape minted by `ids::change_id`:
/// `GCH-<millis>-<pid hex>` with an optional `-<counter hex>` suffix.
pub fn is_valid_change_id(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("GCH-") else {
        return false;
    };
    let segments: Vec<&str> = rest.split('-').collect();
    if !(2..=3).contains(&segments.len()) {
        return false;
    }
    let millis_ok = !segments[0].is_empty() && segments[0].bytes().all(|b| b.is_ascii_digit());
    let hex_ok = segments[1..].iter().all(|segment| {
        !segment.is_empty()
            && segment
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    });
    millis_ok && hex_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_trailer_in_final_paragraph() {
        let body = "Subject line\n\nGnit-Change-Id: GCH-1760000000000-72e5";
        assert_eq!(change_id(body), Some("GCH-1760000000000-72e5".to_string()));
    }

    #[test]
    fn parses_no_space_trailer_spelling() {
        let body = "Subject line\n\nGnit-Change-Id:GCH-1760000000000-72e5";
        assert_eq!(change_id(body), Some("GCH-1760000000000-72e5".to_string()));
    }

    #[test]
    fn trims_trailing_whitespace_around_value() {
        let body = "Subject line\n\nGnit-Change-Id:   GCH-1760000000000-72e5   \n";
        assert_eq!(change_id(body), Some("GCH-1760000000000-72e5".to_string()));
    }

    #[test]
    fn ignores_prose_mention_outside_final_paragraph() {
        let body = "Subject line\n\nSee Gnit-Change-Id: GCH-1760000000000-72e5 for context.\n\nUnrelated final paragraph.";
        assert_eq!(change_id(body), None);
    }

    #[test]
    fn ignores_trailer_shaped_line_in_middle_paragraph() {
        let body =
            "Subject line\n\nGnit-Change-Id: GCH-1760000000000-72e5\n\nFinal prose paragraph.";
        assert_eq!(change_id(body), None);
    }

    #[test]
    fn rejects_prose_after_the_id_value() {
        let body = "Subject line\n\nGnit-Change-Id: GCH-1760000000000-72e5 for context";
        assert_eq!(change_id(body), None);
    }

    #[test]
    fn rejects_malformed_ids() {
        for value in [
            "GCH-",
            "GCH-abc-72e5",
            "GCH-1760000000000",
            "GCH-1760000000000-",
            "GCH-1760000000000-XYZ",
            "GCH-1760000000000-72e5.",
            "NCH-1760000000000-72e5",
            "GCH-1760000000000-72e5-",
            "GCH-1760000000000-72e5-1-2",
        ] {
            let body = format!("Subject line\n\nGnit-Change-Id: {value}");
            assert_eq!(change_id(&body), None, "value {value:?} should be rejected");
        }
    }

    #[test]
    fn accepts_burst_counter_suffix() {
        let body = "Subject line\n\nGnit-Change-Id: GCH-1760000000000-72e5-2";
        assert_eq!(
            change_id(body),
            Some("GCH-1760000000000-72e5-2".to_string())
        );
    }

    #[test]
    fn last_duplicate_trailer_wins() {
        let body = "Subject line\n\nGnit-Change-Id: GCH-1760000000000-72e5\nGnit-Change-Id: GCH-1760000000001-72e5";
        assert_eq!(change_id(body), Some("GCH-1760000000001-72e5".to_string()));
    }

    #[test]
    fn coexists_with_other_trailers_in_the_block() {
        let body =
            "Subject line\n\nReviewed-by: Someone <x@example.com>\nGnit-Change-Id: GCH-1760000000000-72e5\nSigned-off-by: Someone <x@example.com>";
        assert_eq!(change_id(body), Some("GCH-1760000000000-72e5".to_string()));
    }

    #[test]
    fn ignores_indented_continuation_lines() {
        let body = "Subject line\n\nNote: something\n  Gnit-Change-Id: GCH-1760000000000-72e5";
        assert_eq!(change_id(body), None);
    }

    #[test]
    fn returns_none_without_any_trailer() {
        assert_eq!(change_id("Subject only"), None);
        assert_eq!(change_id(""), None);
    }

    #[test]
    fn handles_single_paragraph_message_with_trailer() {
        // Degenerate but unambiguous: the only paragraph is the final one.
        let body = "Gnit-Change-Id: GCH-1760000000000-72e5";
        assert_eq!(change_id(body), Some("GCH-1760000000000-72e5".to_string()));
    }
}
