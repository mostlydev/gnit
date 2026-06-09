# Gnit

Gnit is a Git-native workspace layer for changes that span multiple independent
repositories.

The current design is in [docs/planning/gnit-design.md](docs/planning/gnit-design.md).
It defines the v1 primitives:

- **Change**: a logical cross-repo change grouped by `Gnit-Change-Id`.
- **Pin**: a committed, reproducible snapshot of exact member repo commits.
- **Checkout**: safe materialization of a Pin across the workspace.
- **Review**: a combined review artifact for a Change or Pin.

The CLI implementation is Rust. The current roadmap is in
[docs/planning/implementation-roadmap.md](docs/planning/implementation-roadmap.md).

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/mostlydev/gnit/master/install.sh | sh
```

This downloads the latest release for your platform, verifies its SHA-256
checksum, and installs `gnit` to `~/.local/bin` (override with `GNIT_INSTALL_DIR`).
It also removes a pre-rename `nit` install when it can verify the binary, skill
links, and skill data are this tool's — unrelated tools named `nit` are never
touched. It requires `git` and `curl`. Verify the install with:

```sh
gnit doctor
```

Update later with `gnit update`, which re-runs the verified installer. Use
`gnit update --check` to refresh cached release metadata explicitly. Official
release builds may print a one-line cached hint when a newer release is
available, but dev, CI, and noninteractive runs stay quiet. Gnit never
auto-updates; it only updates when you ask.

## Agent Skills

Install the bundled Gnit skill into agent harnesses so agents use the `gnit`
workflow instead of guessing at raw Git across member repos:

```sh
gnit skills install --all
gnit skills install claude codex opencode grok-build --copy
gnit skills list
```

The default install mode is `--link`, which points each harness at Gnit's managed
skill source under the Gnit data directory. Use `--copy` for standalone snapshots.
Supported harnesses are Claude Code, Codex, OpenCode, and Grok Build.

Beyond the installable skill, `gnit init` drops a short, version-stable workspace
note into the repo's agent-instruction docs — `AGENTS.md`, plus `CLAUDE.md` when
that file already exists — so any agent reading the file it scans first learns to
drive cross-repo work with the `gnit` CLI and skill instead of hand-managing repos
with raw Git. The note lives between `<!-- gnit:workspace:start -->` and
`<!-- gnit:workspace:end -->` markers, so re-running never duplicates it and any
edits you make inside survive. `gnit doctor` reports `agent guidance: ok` when the
block is present and re-adds it when it is missing.

## Quickstart

```sh
# Turn a directory of related repos into one workspace.
cd my-workspace
gnit init                 # or: gnit init --control  (when there is no root repo)
gnit adopt app sdk infra  # register existing repos as members (each a repo root)

# Make one change across several repos.
gnit add app/src sdk/src  # stage paths across members (or: gnit add -A)
gnit commit -m "Wire the new field end to end"   # one Gnit-Change-Id across repos
gnit push                 # publish members first, then workspace metadata

# Or publish a reproducible snapshot (a Pin) in one step.
gnit land -m "Release the new field"   # commit + pin together
gnit push
gnit pr open            # create/adopt linked draft GitHub PRs

gnit status               # root/members, staged/modified/untracked, pin drift, discovered repos
gnit pr                   # linked PR status for the current Change
gnit log                  # unified timeline of changes and pins
gnit change show <id>     # the commits that make up a change
gnit review <id-or-pin>   # combined review artifact
gnit skills install --all # teach installed agents the Gnit workflow
```

`gnit push` reports every target and is safe to retry. If a member fails, Gnit
holds the workspace metadata back; after fixing the member, run `gnit push` again
or `gnit push --resume` for the explicit retry spelling. It also holds metadata
back if a Pin references a current member commit that is not reachable from the
member's local `HEAD` or an `origin/*` remote-tracking ref.

`gnit pr` is read-only status for the current workspace Change. `gnit pr open`
creates missing draft GitHub PRs, adopts existing same-branch PRs, and refreshes
Gnit-owned cross-links in each PR body. It derives the Change, branch, base, and
title from Git state in the common case; use `--change`, `--pin`, `--base`, or
`--title` only as escape hatches.

One workspace Change becomes one ordinary PR per touched repo, visible at a glance:

```text
$ gnit pr
Workspace change GCH-1780970169140-18d6
repo                         branch              base        pr        state     checks
root (metadata)              feature/pr-flow     master      #1        open      pending
sdk                          feature/pr-flow     master      #2        open      pass
app                          feature/pr-flow     master      missing   -         -

$ gnit pr open
  root                     already open
  sdk                      already open
  app                      created
PRs synchronized.
```

`gnit pr open` only creates what is missing, so it is safe to re-run after a
failure — already-open PRs are refreshed, never duplicated.

## CI Enforcement

Use `gnit ci-check` as the CI/server-side enforcement point for the
reconstruct-not-enforce model:

```sh
gnit ci-check --base origin/main --head HEAD
```

For ordinary member repositories, the command requires every commit in
`base..head` to carry a well-formed `Gnit-Change-Id` trailer. When run from a
Gnit workspace root, metadata-only commits under `.gnit/` and managed agent
guidance files are allowed without trailers, and every committed Pin at `head`
is checked against each active member's `origin` refs after fetching that member
origin.

The repository also ships a thin composite Action wrapper:

```yaml
steps:
  - uses: actions/checkout@v6
    with:
      fetch-depth: 0
  - run: |
      mkdir -p "$HOME/.local/bin"
      echo "$HOME/.local/bin" >> "$GITHUB_PATH"
      curl -fsSL https://raw.githubusercontent.com/mostlydev/gnit/master/install.sh | sh
  - uses: mostlydev/gnit/.github/actions/gnit-ci-check@<ref>
    with:
      base: origin/${{ github.base_ref }}
```

Workspace-root CI jobs must check out or hydrate member repos before running the
Action so pinned member commits can be verified against their remotes.

Reconstruct the workspace on another machine:

```sh
gnit clone git@github.com:example/product-workspace.git product --pin baseline
```

See the [full guide](https://mostlydev.github.io/gnit/) for clone, pins,
checkout, review, and the design rationale.

The public documentation site is live at **https://mostlydev.github.io/gnit/**.
It lives in [site/](site/) as a VitePress site and redeploys via
[.github/workflows/deploy-site.yml](.github/workflows/deploy-site.yml) on every
push to `master` that touches `site/**` or the workflow. The build sets
`VITEPRESS_BASE=/gnit/` for the project-page path; if a custom domain (e.g.
`gnit.dev`) is added later, set the base to `/` and add `site/public/CNAME`.

## Repository Layout

```text
src/                Rust CLI implementation.
tests/              CLI integration tests.
skills/
  gnit/              Bundled agent skill, embedded into the binary.
  gnit-release/      Maintainer release runbook skill (dev-only; not shipped).
.agents/skills/     Cross-harness project skill links (Codex, OpenCode, ...).
.claude/skills/     Claude Code project skill links.
.grok/skills/       Grok project skill links — all symlink into skills/.
install.sh          Release installer (used by `gnit update`).
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
