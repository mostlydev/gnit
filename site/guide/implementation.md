# Implementation

Nit is implemented as a Rust single-binary CLI.

The first implementation shells out to `git`. Rust handles command parsing,
transaction planning, diagnostics, state modeling, and tests. Git remains the
source of truth for repository operations.

## Release Path

Nit follows the same release shape as Clawdapus:

- GitHub Releases publish platform tarballs.
- `checksums.txt` verifies downloads.
- `install.sh` installs the latest release.
- `nit update` uses the same installer path.

`nit update` is the explicit binary replacement command. Official binaries may
check release metadata on a cached, best-effort schedule and print a notice. Dev
builds and package-manager installs do not replace themselves unless forced.

## Transparent Upkeep

Nit should not make users run maintenance commands that can be inferred safely.
Every command may perform non-destructive upkeep first: local exclude repair,
generated helper refresh, roster cache refresh, and release metadata checks.

Destructive or meaning-changing operations remain explicit.

Automatic upkeep is not the same as automatic binary replacement. Upkeep is
local, idempotent, fast, and non-destructive. Binary replacement stays explicit
until releases are signed.
