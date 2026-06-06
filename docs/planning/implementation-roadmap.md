# Implementation Roadmap

## Direction

Nit is a Rust CLI distributed as a single binary. The implementation shells out
to `git` at first and keeps Git as the source of truth.

## Phase 0: Skeleton And First Product Slice

- Root Rust package named `nit`.
- Clap-based command surface.
- `nit init`, `nit clone`, `nit adopt`, `nit ignore`, `nit import-submodule`,
  `nit add`, `nit commit`, `nit land`, `nit checkout`, `nit push`, `nit status`,
  `nit doctor`, `nit pin`, `nit change`, `nit review`, and `nit update` initial
  commands.
- typed roster and Pin metadata persisted as YAML.
- transparent upkeep hook, wired but intentionally non-destructive
- `install.sh` matching the GitHub Release tarball/checksum pattern.
- Workflow tests for root repo setup, nested repo adoption, local excludes,
  metadata-only commits, dirty-worktree pin refusal, Pin creation, ordered push
  resume, clone plus pinned checkout, combined review, submodule import, doctor
  exclude repair, and update dry-run.

## Phase 1: Roster And Discovery

- `nit init`
- `.nit/roster.yaml`
- `nit adopt`
- automatic local-exclude repair
- workspace root discovery
- status grouping by member repo

## Phase 2: Change Grouping

- `nit add`
- `nit commit`
- `Nit-Change-Id` trailer generation
- Change projection and ambiguity reporting
- trailer-based `nit change status/show/log/diff`

## Phase 3: Pins And Land

- `nit pin`
- `nit land`
- pin artifacts under `.nit/pins/`
- metadata auto-commit
- `nit pin --change` provenance recording

## Phase 4: Checkout

- `nit checkout <pin>`
- missing member clone/materialization
- `--exact` destructive mode with confirmation/policy
- safe checkout refuses dirty members unless `--exact`

## Phase 5: Push, Review, Doctor

- ordered `nit push`
- strict `nit push --resume` retry
- partial-landing reports
- `nit review`
- `nit doctor` recovery for trailers, pins, excludes, and remote drift

## Phase 6: Legible And Self-Healing Workspace (v0.2.0)

- rich `nit status`: root and per-member staged/modified/untracked counts,
  branch, missing members, member drift from the current pin, and
  discovered-but-unadopted repos
- `nit log`: unified, newest-first timeline of Changes and Pins
- real transparent upkeep: self-healing local-exclude repair on every command
  (fast, quiet, non-destructive, no network)

## Phase 7: Cached Update Notice (v0.3.0)

- official release builds read a local update cache on normal commands
- if a newer release is cached, print a one-line `nit update` notice at most
  once per day
- if the cache is missing or stale, schedule a detached refresh with a bounded
  GitHub Releases request only for interactive official builds outside CI
- keep binary replacement explicit through `nit update`
- expose `nit update --check` for an explicit metadata refresh

## Phase 8: Branch-Aware Checkout (v0.4.0)

- use Pin branch hints and local/remote branch tips during `nit checkout`
- keep normal checkouts on branches when safe
- detach only when no branch points at the pinned commit, and warn clearly
- avoid hidden branch ref rewrites in `nit checkout --exact`
- harden cached-update parsing from the v0.3.0 third-party review

## Phase 9: Strict Push Resume (v0.5.0)

- report pushed, already-landed, failed, not-attempted, and held-back targets
- keep `nit push` and `nit push --resume` on one strict ordered policy
- use remote refs as the resume journal; do not write local push state
- stop at the first member failure and hold root/control metadata back
- block root/control push when a Pin references a member commit no longer
  reachable from the member branch being published

## Release And Update

- GitHub Release assets for supported platforms.
- `checksums.txt` verification.
- `nit update` explicit binary replacement once release assets exist.
- `nit update --check` explicit release metadata check.
- cached update notices for official binaries.
- no self-update for dev builds unless forced.
- signed releases before any default auto-install policy is reconsidered.
