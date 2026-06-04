# 0007: Legible And Self-Healing Workspace (v0.2.0)

## Status

Accepted. Shipped in v0.2.0; root repository status was added in v0.2.1 and
documented in v0.2.2.

## Context

v0.1.0 shipped the full publish/reproduce flow, but two commands were minimal:
`nit status` only listed member ids and paths, and there was no top-level
`nit log`. The transparent upkeep hook was wired but a no-op.

## Decision

v0.2.0 makes the workspace legible and self-healing, without adding network
calls to the hot path.

- **Rich `nit status`** reports the workspace root and each member: staged /
  modified / untracked counts and the current branch (or `detached`). Members
  also report `missing locally` and `drifted from pin` (member HEAD vs the newest
  pin's recorded commit). It also lists **discovered-but-unadopted** nested repos
  with adopt/ignore hints. The "current pin" is the newest pin by id (ids embed a
  creation timestamp).

- **`nit log`** renders one interleaved, newest-first timeline of Changes
  (reconstructed from `Nit-Change-Id` trailers across members) and Pins, each
  with a UTC date. This is the operator's "retrievable shared graph" as a single
  command. `nit change log` remains the change-only view.

- **Real upkeep.** The transparent upkeep hook now repairs the root repo's local
  `.git/info/exclude` from the roster on every command. Local excludes are not
  committed, so a fresh clone/checkout needs them reapplied; upkeep does this
  automatically. It stays fast and idempotent (only writes when an entry is
  missing), is silent on a no-op, prints one line when it repairs, never touches
  member working trees, never hits the network, and never commits. `--no-upkeep`
  / `NIT_NO_UPKEEP` still disables it.

## Consequences

- Discovery walks the tree but prunes at repo boundaries and caps depth, so it
  stays fast.
- Drift is computed against the newest pin; an explicit "current pin" pointer is
  deferred.
- The transparent **update notice** (a cached "vX available" hint) was deferred
  from v0.2.0 and shipped separately in v0.3.0.
