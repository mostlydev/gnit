use std::env;

use crate::metadata::Roster;
use crate::workspace;

/// Transparent, automatic upkeep run before most commands. It must stay fast,
/// local-only, non-destructive, and quiet on a no-op. Today it repairs the
/// root repo's local `.git/info/exclude` from the roster (these excludes are
/// local, so a fresh clone/checkout needs them reapplied) — never touching
/// member working trees, never hitting the network, never committing.
pub fn run_transparent_upkeep(verbose: bool) {
    let Ok(cwd) = env::current_dir() else {
        return;
    };
    let Some(root) = workspace::find_gnit_workspace(&cwd) else {
        if verbose {
            eprintln!("gnit upkeep: not in a workspace");
        }
        return;
    };
    let Ok(roster) = Roster::read(&root) else {
        return;
    };

    match workspace::repair_required_excludes(&root, &roster) {
        Ok(0) => {
            if verbose {
                eprintln!("gnit upkeep: no pending local maintenance");
            }
        }
        Ok(n) => {
            let noun = if n == 1 { "entry" } else { "entries" };
            eprintln!("gnit upkeep: restored {n} local exclude {noun}");
        }
        Err(error) => {
            if verbose {
                eprintln!("gnit upkeep: skipped exclude repair: {error}");
            }
        }
    }
}
