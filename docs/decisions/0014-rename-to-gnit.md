# 0014: Rename nit to gnit

## Status

Accepted. Applied in one pass across the repository in June 2026.

## Context

The tool launched as `nit` (0001). Before any public presence existed, a
naming review found the word effectively un-searchable: it collides with the
Nit programming language, an unrelated linter, the English word, and the `nit`
crate on crates.io (taken by an unrelated 2025 crate, so `cargo install nit`
could never be this tool). Candidates were weighed while the user count was
zero:

- `knit`: thematically ideal, but the crates.io name is held by an active
  unrelated crate and search is dominated by R's knitr and a family of
  knitting-machine projects.
- `gnit`: near-empty GitHub and web search footprint, keeps the three-letter
  verb ergonomics (0003), and reads naturally as git + knit. The crates.io
  name is squatted by a dead 2018 crate, which matters little while
  distribution is binary releases plus `install.sh` (0006).

## Decision

Rename the product to **gnit**, as a clean break with no legacy-name support:

- Crate, binary, and CLI command: `gnit`.
- Workspace metadata directory: `.gnit/` (roster at `.gnit/roster.yaml`,
  pins under `.gnit/pins/`).
- Commit trailer: `Gnit-Change-Id`. Change id prefix: `GCH-`.
- Environment variables: `GNIT_*`.
- Agent guidance markers: `<!-- gnit:workspace:start/end -->`.
- Skill names: `gnit` (bundled) and `gnit-release` (dev-only).
- GitHub repository: `mostlydev/gnit` (GitHub redirects the old URL); site at
  `mostlydev.github.io/gnit/`; release artifacts `gnit-<version>-<os>-<arch>`.

Earlier decision documents and planning docs were retro-renamed in the same
pass rather than preserved verbatim; this document is the record that the
rename happened and that pre-rename history reads `nit` in original commits.

## Consequences

- Workspaces created before the rename are not auto-migrated. Migration is
  manual: `mv .nit .gnit`, re-run `gnit doctor`. Commits carrying the old
  `Nit-Change-Id` trailer and `NCH-` ids are not grouped by the renamed tool;
  with zero external users this was judged cheaper than carrying legacy read
  support indefinitely.
- Old installed binaries (`nit`) and harness skill installs under the old name
  must be removed by hand; `gnit skills install` lays down the renamed skill.
- Releases prior to the rename keep `nit-*` artifact names on GitHub; the
  installer only knows the new names, so the first post-rename release is the
  oldest installable version.
