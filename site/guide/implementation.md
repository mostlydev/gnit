# Implementation

Gnit is implemented as a Rust single-binary CLI.

The first implementation shells out to `git`. Rust handles command parsing,
transaction planning, diagnostics, state modeling, and tests. Git remains the
source of truth for repository operations.

The current slice implements workspace init/adoption, typed roster metadata,
cross-repo staging, trailer-based Change commits and views, `land`,
branch-aware Pin checkout, ordered push with strict retry reports,
status/doctor, explicit update, cached update notices, Pin recording for
committed member HEADs, GitHub PR projection, and explicit agent skill
installation.

## Release Path

Gnit follows the same release shape as Clawdapus:

- GitHub Releases publish platform tarballs.
- `checksums.txt` verifies downloads.
- `install.sh` installs the latest release.
- `gnit update` uses the same installer path.

`gnit update` is the explicit binary replacement command. `gnit update --check`
refreshes release metadata without replacing the binary. Official binaries may
refresh that metadata on a cached, best-effort schedule and may print a notice.
Dev builds and package-manager installs do not replace themselves unless forced.

## Agent Skill Distribution

Gnit ships its `skills/gnit/SKILL.md` content inside the binary. `gnit skills
install` materializes that embedded skill into a managed source under
`$GNIT_DATA_DIR`, `$XDG_DATA_HOME/gnit`, or `~/.local/share/gnit`, then links or
copies it into supported harness skill directories. This keeps binary-only
release installs working without requiring a source checkout.

## GitHub PR Projection

`gnit pr` and `gnit pr open` keep GitHub PRs ordinary while making a workspace
Change legible across repos. The join key is `Gnit-Change-Id`; Pins can be used as
an alias only when they record exactly one provenance Change. Gnit derives the
current Change from commits on the current branch since the merge-base with the
base branch, uses `gh -R` for repo-specific PR operations, and owns only the
`gnit-pr-sync` marker block in PR bodies.

The marker block is the durable cross-link Gnit writes into each participating PR
body. Everything outside it is author-owned and preserved across reruns; only the
block between the markers is regenerated:

```markdown
<!-- gnit-pr-sync:start -->
Gnit-Change-Id: GCH-1780970169140-18d6

Workspace PR: acme/root#1

Member PRs:
- acme/sdk#2 @ bf8097650e5b
- acme/app#3 @ be0a2ab45be8

Commits:
- root: . @ dcb6d20de793
- sdk: sdk @ bf8097650e5b
- app: app @ be0a2ab45be8

Recover:
  gnit pr
<!-- gnit-pr-sync:end -->
```

Because the block records the `Gnit-Change-Id` and each repo's commit, the PR set
is re-discoverable from the remote alone: a rerun finds existing PRs by marker or
by head branch, so a partial failure resumes by creating only the missing PRs.

A workspace root with no code change of its own still anchors the set when it is
a `gnit land` Pin host: if the root branch committed a Pin whose provenance
contains the selected Change, Gnit opens a metadata-only root PR (shown as
`root (metadata)` in status). Pin metadata commits deliberately do **not** carry
`Gnit-Change-Id` trailers — that projection is PR-specific, so `gnit change` and
`gnit review` stay free of metadata noise.

## Transparent Upkeep

Gnit should not make users run maintenance commands that can be inferred safely.
Every command may perform non-destructive upkeep first: local exclude repair,
generated helper refresh, and roster cache refresh. Release metadata notices read
a local cache only; stale-cache refreshes are detached, bounded, and limited to
interactive official builds outside CI, so foreground commands do not wait on
the network.

Destructive or meaning-changing operations remain explicit.

Automatic upkeep is not the same as automatic binary replacement. Upkeep is
local, idempotent, fast, and non-destructive. Binary replacement stays explicit
until releases are signed.
