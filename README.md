# Nit

Nit is a Git-native workspace layer for changes that span multiple independent
repositories.

The current design is in [docs/planning/nit-design.md](docs/planning/nit-design.md).
It defines the v1 primitives:

- **Change**: a logical cross-repo change grouped by `Nit-Change-Id`.
- **Pin**: a committed, reproducible snapshot of exact member repo commits.
- **Checkout**: safe materialization of a Pin across the workspace.
- **Review**: a combined review artifact for a Change or Pin.

The CLI implementation is Rust. The current roadmap is in
[docs/planning/implementation-roadmap.md](docs/planning/implementation-roadmap.md).

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/mostlydev/nit/master/install.sh | sh
```

This downloads the latest release for your platform, verifies its SHA-256
checksum, and installs `nit` to `~/.local/bin` (override with `NIT_INSTALL_DIR`).
It requires `git` and `curl`. Verify the install with:

```sh
nit doctor
```

Update later with `nit update`, which re-runs the verified installer. Use
`nit update --check` to refresh cached release metadata explicitly. Official
release builds may print a one-line cached hint when a newer release is
available, but dev, CI, and noninteractive runs stay quiet. Nit never
auto-updates; it only updates when you ask.

## Agent Skills

Install the bundled Nit skill into agent harnesses so agents use the `nit`
workflow instead of guessing at raw Git across member repos:

```sh
nit skills install --all
nit skills install claude codex opencode grok-build --copy
nit skills list
```

The default install mode is `--link`, which points each harness at Nit's managed
skill source under the Nit data directory. Use `--copy` for standalone snapshots.
Supported harnesses are Claude Code, Codex, OpenCode, and Grok Build.

## Quickstart

```sh
# Turn a directory of related repos into one workspace.
cd my-workspace
nit init                 # or: nit init --control  (when there is no root repo)
nit adopt app sdk infra  # register existing repos as members (each a repo root)

# Make one change across several repos.
nit add app/src sdk/src  # stage paths across members (or: nit add -A)
nit commit -m "Wire the new field end to end"   # one Nit-Change-Id across repos
nit push                 # publish members first, then workspace metadata

# Or publish a reproducible snapshot (a Pin) in one step.
nit land -m "Release the new field"   # commit + pin together
nit push
nit pr open            # create/adopt linked draft GitHub PRs

nit status               # root/members, staged/modified/untracked, pin drift, discovered repos
nit pr                   # linked PR status for the current Change
nit log                  # unified timeline of changes and pins
nit change show <id>     # the commits that make up a change
nit review <id-or-pin>   # combined review artifact
nit skills install --all # teach installed agents the Nit workflow
```

`nit push` reports every target and is safe to retry. If a member fails, Nit
holds the workspace metadata back; after fixing the member, run `nit push` again
or `nit push --resume` for the explicit retry spelling.

`nit pr` is read-only status for the current workspace Change. `nit pr open`
creates missing draft GitHub PRs, adopts existing same-branch PRs, and refreshes
Nit-owned cross-links in each PR body. It derives the Change, branch, base, and
title from Git state in the common case; use `--change`, `--pin`, `--base`, or
`--title` only as escape hatches.

One workspace Change becomes one ordinary PR per touched repo, visible at a glance:

```text
$ nit pr
Workspace change NCH-1780970169140-000018d60000000000000000
repo                         branch              base        pr        state     checks
root (metadata)              feature/pr-flow     master      #1        open      pending
sdk                          feature/pr-flow     master      #2        open      pass
app                          feature/pr-flow     master      missing   -         -

$ nit pr open
  root                     already open
  sdk                      already open
  app                      created
PRs synchronized.
```

`nit pr open` only creates what is missing, so it is safe to re-run after a
failure — already-open PRs are refreshed, never duplicated.

Reconstruct the workspace on another machine:

```sh
nit clone git@github.com:example/product-workspace.git product --pin baseline
```

See the [full guide](https://mostlydev.github.io/nit/) for clone, pins,
checkout, review, and the design rationale.

The public documentation site is live at **https://mostlydev.github.io/nit/**.
It lives in [site/](site/) as a VitePress site and redeploys via
[.github/workflows/deploy-site.yml](.github/workflows/deploy-site.yml) on every
push to `master` that touches `site/**` or the workflow. The build sets
`VITEPRESS_BASE=/nit/` for the project-page path; if a custom domain (e.g.
`nit.dev`) is added later, set the base to `/` and add `site/public/CNAME`.

## Repository Layout

```text
src/                Rust CLI implementation.
tests/              CLI integration tests.
skills/             Bundled agent skills embedded into the binary.
install.sh          Release installer (used by `nit update`).
docs/
  decisions/        Locked product and design decisions.
  planning/         Design plans and archived drafts.
site/               Public VitePress documentation site.
```

## Local Site

```sh
cd site
npm install
npm run build
npm run dev
```

If the site is deployed as a GitHub Pages project site instead of a custom
domain, set `VITEPRESS_BASE=/<repo-name>/` for the build.

## CLI Development

```sh
cargo test
cargo run -- doctor
cargo run -- status
```
