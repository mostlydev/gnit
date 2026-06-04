# 0008: Cached Update Notice

## Status

Accepted. Shipped in v0.3.0.

## Context

Nit should keep itself easy to operate, but binary replacement is a supply-chain
operation. v0.2.x already had the explicit `nit update` installer path and
transparent local upkeep. The next missing piece was a low-friction way to tell
users that a newer release exists without blocking normal commands or silently
installing anything.

## Decision

Official release builds maintain a local update metadata cache.

- Normal commands read the cache only. If it says a newer release exists, Nit
  prints a one-line notice at most once per day:
  `nit <version> is available (current <version>); run nit update`.
- If the cache is missing or older than one day, Nit may schedule a detached
  background refresh and immediately continues the foreground command. This only
  happens for official release builds in an interactive TTY outside CI.
- The refresh uses the GitHub Releases latest-release API through `curl` with a
  short timeout. Network failures are ignored for normal commands.
- The cache lives at `$NIT_UPDATE_CACHE_PATH` when set, otherwise
  `$XDG_CACHE_HOME/nit/update-check`, otherwise `~/.cache/nit/update-check`.
- Dev builds, CI, non-TTY runs, `--no-upkeep`, and `NIT_NO_UPKEEP=1` never print
  or schedule the notice.
- `NIT_NO_UPDATE_NOTICE=1` also disables the notice and refresh scheduling.
- `nit update --check` refreshes release metadata explicitly and prints whether
  the cached latest release is newer than the current binary.
- `nit update` remains the only binary replacement path.

The background worker invokes `nit --no-upkeep update --check` with stdio
discarded. The human command surface uses the same `--check` spelling.

## Consequences

- The hot path never waits on the network.
- Users get update awareness without a manual `rehash`-style chore.
- Notice state is local and disposable.
- A package manager can avoid the notice by building without `NIT_COMMIT` or by
  setting `NIT_NO_UPKEEP=1` or `NIT_NO_UPDATE_NOTICE=1` in its wrapper.
- Automatic binary replacement remains deferred until release authenticity is
  stronger than HTTPS + checksums.
