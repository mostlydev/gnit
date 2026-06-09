# Quickstart

Turn a directory of related repos into one workspace, then commit and publish
across all of them at once. The Rust CLI now ships the v0 loop: construct a
workspace, land a cross-repo change, push in dependency-safe order, clone it on a
fresh machine, and review the combined artifact.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/mostlydev/nit/master/install.sh | sh
```

This downloads the latest release for your platform, verifies its SHA-256
checksum, and installs `nit` to `~/.local/bin` (override with `NIT_INSTALL_DIR`).
It needs `git` and `curl`. Verify the install with `nit doctor`, and update later
with `nit update`.

## Create A Workspace

```sh
mkdir product && cd product
nit init --control --remote git@github.com:example/product-workspace.git

git clone git@github.com:example/app.git app
git clone git@github.com:example/sdk.git sdk
git clone git@github.com:example/docs.git docs

nit adopt app sdk docs
nit pin baseline
nit push
```

## Publish A Cross-Repo Change

```sh
nit add -A
nit land -m "Publish webhook retry update"
nit push
nit pr open
```

`nit land` is the human-facing publish verb. It commits staged member changes,
creates an unnamed Pin, and lets `nit push` publish member commits before the Pin.
If one member push fails, Nit reports what landed, leaves later targets
not-attempted, and holds the workspace root back. After resolving the member
repo, run `nit push` again, or use `nit push --resume` as an explicit retry.

`nit pr open` creates or adopts the ordinary GitHub PRs for the current Change
and writes Nit-owned cross-links into each PR body. It opens draft PRs by
default. `nit pr` is the read-only status command to check what exists, what is
missing, and which checks are passing:

```text
$ nit pr
Workspace change NCH-1780970169140-18d6
repo                         branch              base        pr        state     checks
root (metadata)              feature/pr-flow     master      #1        open      pending
sdk                          feature/pr-flow     master      #2        open      pass
app                          feature/pr-flow     master      missing   -         -

$ nit pr open
Opening PRs for Change NCH-1780970169140-18d6
Title: Add linked PR flow
Mode: draft
  root                     already open
  sdk                      already open
  app                      created
PRs synchronized.
```

Re-running `nit pr open` is safe: it refreshes already-open PRs instead of
duplicating them, so it doubles as the recovery command after a network blip.

```sh
nit review <change-id-or-pin>
```

## Reconstruct A Workspace

```sh
nit clone git@github.com:example/product-workspace.git product --pin baseline
```

`nit clone` clones the control repo and hydrates member repos from the roster.
With `--pin`, it also materializes the selected Pin.

Pinned checkout is intentionally exact, but it stays branch-aware. If the pinned
commit is a local or remote branch tip, Nit checks out that branch instead of
leaving you on a detached HEAD. It refuses to overwrite dirty member worktrees
unless you use `nit checkout <pin> --exact`.
