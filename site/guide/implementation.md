# Implementation

Nit is implemented as a Rust single-binary CLI.

The first implementation shells out to `git`. Rust handles command parsing,
transaction planning, diagnostics, state modeling, and tests. Git remains the
source of truth for repository operations.

The current slice implements workspace init/adoption, typed roster metadata,
cross-repo staging, trailer-based Change commits and views, `land`, safe Pin
checkout, ordered push, status/doctor, explicit update, cached update notices,
and Pin recording for committed member HEADs.

## Release Path

Nit follows the same release shape as Clawdapus:

- GitHub Releases publish platform tarballs.
- `checksums.txt` verifies downloads.
- `install.sh` installs the latest release.
- `nit update` uses the same installer path.

`nit update` is the explicit binary replacement command. `nit update --check`
refreshes release metadata without replacing the binary. Official binaries may
refresh that metadata on a cached, best-effort schedule and may print a notice.
Dev builds and package-manager installs do not replace themselves unless forced.

## Transparent Upkeep

Nit should not make users run maintenance commands that can be inferred safely.
Every command may perform non-destructive upkeep first: local exclude repair,
generated helper refresh, and roster cache refresh. Release metadata notices read
a local cache only; stale-cache refreshes are detached, bounded, and limited to
interactive official builds outside CI, so foreground commands do not wait on
the network.

Destructive or meaning-changing operations remain explicit.

Automatic upkeep is not the same as automatic binary replacement. Upkeep is
local, idempotent, fast, and non-destructive. Binary replacement stays explicit
until releases are signed.
