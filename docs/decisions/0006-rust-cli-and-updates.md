# 0006: Rust CLI, Releases, And Transparent Upkeep

## Status

Accepted.

## Decision

Gnit is implemented as a Rust single-binary CLI.

The first implementation shells out to `git` rather than binding to libgit2.
Rust owns transaction planning, state modeling, diagnostics, and integration
tests; Git remains the source of truth for repository operations.

Gnit follows the Clawdapus-style release path:

- GitHub Releases publish platform tarballs and `checksums.txt`.
- `install.sh` downloads the latest release and verifies the checksum before
  replacing the binary.
- `gnit update` uses the same installer path.
- `gnit update` is the explicit binary replacement command.
- Official release builds read cached release metadata and may print a notice.
  Stale metadata refresh is detached, bounded, and limited to interactive
  official builds outside CI. Dev builds and package-manager installs do not
  replace themselves unless explicitly forced.

Every command may run non-destructive upkeep before doing its primary work:

- repair local excludes from the roster
- refresh generated helper files or shims
- refresh cache/index state
- rescan roster metadata
- read cached release metadata without blocking command execution

There should be no manual equivalent of `rbenv rehash`. If Gnit can safely infer
and repair a stale generated artifact, it should do so.

Destructive or meaning-changing operations remain explicit. Examples include
`gnit checkout --exact`, recovery that rewrites refs, and repairs that alter
commits or Pin artifacts.

Automatic upkeep is distinct from automatic binary replacement. Upkeep is local,
idempotent, fast, and non-destructive. Binary replacement is a supply-chain
operation; it must be explicit until release authenticity is stronger than
checksums over HTTPS.

## Rationale

The website uses Node, but the CLI should not inherit Node's runtime ergonomics.
Rust gives a distributable binary and stronger modeling for a tool that plans
and mutates multi-repo Git state.

Transparent upkeep removes avoidable chores while preserving the core safety
principle: Gnit may automate non-destructive maintenance, but it must not hide
destructive workspace changes.

SHA256 checksums verify integrity of downloaded release artifacts, not
authenticity. Signed releases are required before any default auto-install policy
is reconsidered.
