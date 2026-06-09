---
name: gnit
description: Use when working in a Gnit workspace (a `.gnit/` directory or `.gnit/roster.yaml` is present), or when a single change spans several independent Git repositories. Teaches the Change / Pin / branch-aware-checkout / ordered-push workflow through the `gnit` CLI instead of hand-managing submodules or raw Git across repos.
---

# Driving Gnit

Gnit is a Git-native workspace layer for changes that span multiple independent
repositories. Each member stays an ordinary Git repo — its own remote, branches,
and history. Gnit groups a cross-repo change under one id, snapshots exact commits
as a reproducible Pin, and publishes everything in a safe order. It shells out to
`git` and keeps Git as the source of truth.

Use this skill when the workspace has a `.gnit/` directory, when the user asks to
commit/pin/checkout/push across repos, or when one logical change touches more
than one repository.

## Core model

- **Member**: an ordinary Git repo registered in the workspace roster.
- **Change**: commits across members that share one `Gnit-Change-Id` trailer.
- **Pin**: a committed, reproducible snapshot of exact member commits.
- **Checkout**: safe materialization of a Pin across the workspace.

Gnit never rewrites member repos. Walk away and you still have plain Git.

## The workflow

```sh
# Build a workspace from a directory of related repos.
gnit init                 # or: gnit init --control   (no natural root repo)
gnit adopt app sdk infra  # register existing repos as members (each a repo root)

# Make one change across several repos.
gnit add app/src sdk/src  # stage across members (or: gnit add -A)
gnit commit -m "Wire the new field end to end"   # one Gnit-Change-Id across repos
gnit push                 # publish members first, then workspace metadata

# Or publish a reproducible snapshot in one step.
gnit land -m "Release the new field"   # commit + pin together
gnit push
gnit pr open             # create/adopt linked draft GitHub PRs
```

Inspect and reconstruct:

```sh
gnit status               # root/members, staged/modified/untracked, pin drift, discovered repos
gnit pr                   # linked PR status for the current Change
gnit log                  # unified newest-first timeline of Changes and Pins
gnit change show <id>     # the commits that make up a Change
gnit review <id-or-pin>   # combined review artifact
gnit clone <url> dir --pin <label>   # rebuild a workspace and materialize a Pin
```

## Rules that matter

- **Prefer `gnit` verbs over raw Git for cross-repo actions.** Use `gnit add`,
  `gnit commit`, `gnit land`, `gnit push`, `gnit checkout`. Per-repo Git is fine
  inside a single member; reach for Gnit when the action spans members or touches
  workspace metadata under `.gnit/`.
- **Push is ordered and safe to retry.** `gnit push` publishes members in roster
  order, then the workspace root/control repo last, and holds the root back if
  any member fails or if a Pin references a member commit that is not reachable
  from local member `HEAD` or an `origin/*` remote-tracking ref.
  After fixing the member, run `gnit push` again, or `gnit push --resume` as the
  explicit retry spelling. Gnit never force-pushes; a non-fast-forward is a hard
  failure to resolve in the member, not to override.
- **PRs stay ordinary GitHub PRs.** After `gnit push`, run `gnit pr` to inspect
  the PR projection or `gnit pr open` to create/adopt linked draft PRs. Gnit
  derives the current Change, branch, base, and title in the common case. Use
  `--change`, `--pin`, `--base`, `--title`, `--branch`, or `--ready` only as
  escape hatches. Gnit only rewrites its own marker block in PR bodies.
- **Pins require clean members.** `gnit pin` and `gnit land` refuse a dirty member
  worktree, because a Pin must capture exact, reproducible commits. Commit or
  stash member changes first.
- **Checkout is branch-aware and refuses to clobber.** `gnit checkout <pin>` stays
  on a branch when the pinned commit is a branch tip and only detaches when it
  must, warning clearly. It refuses to overwrite dirty member worktrees unless
  you pass `--exact`, which resets and cleans them — treat `--exact` as
  destructive and confirm intent before using it.
- **Let metadata commits happen.** Commands like `gnit adopt`, `gnit pin`, and
  `gnit land` auto-commit `.gnit/` metadata. Pass `--no-commit` only when you mean
  to stage the metadata change yourself.
- **Upkeep is automatic and non-destructive.** Gnit repairs local excludes quietly
  on each command. Do not hand-edit `.gnit/` machinery to work around it.
- **Updating is explicit.** `gnit update` replaces the binary on request; Gnit
  never auto-updates. `gnit doctor` diagnoses the install and workspace.

## Linking PRs

After `gnit push`, `gnit pr` shows one row per repo for the current Change — open,
`missing`, or a metadata-only `root (metadata)` anchor:

```text
$ gnit pr
Workspace change GCH-1780970169140-18d6
repo                         branch              base        pr        state     checks
root (metadata)              feature/pr-flow     master      #1        open      pending
sdk                          feature/pr-flow     master      #2        open      pass
app                          feature/pr-flow     master      missing   -         -
```

`gnit pr open` creates only the missing PRs, adopts an existing same-branch PR,
and is safe to re-run — it refreshes rather than duplicates. Blockers stop before
any PR is created and name the exact fix; act on the message instead of guessing:

- `... is not at local HEAD ...; run gnit push before gnit pr open` — the branch is
  not fully pushed. Run `gnit push`, then retry `gnit pr open`.
- `multiple Gnit Changes found on the PR branch (...); rerun with gnit pr --change <id>`
  — the branch carries more than one Change. Re-run with the `--change <id>` it
  names.

Never hand-edit the `<!-- gnit-pr-sync:start -->` / `<!-- gnit-pr-sync:end -->`
marker block in a PR body — it is Gnit-owned. Re-run `gnit pr open` to refresh it;
author text outside the block is preserved.

## When something is off

Run `gnit status` and `gnit doctor` first. `status` shows per-member state, drift
from the current Pin, and nested repos that are not yet adopted. `doctor` reports
trailer, pin, exclude, and remote-drift problems. Fix the named member with
ordinary Git, then re-run the Gnit command.

If the workspace has a `.nit/` directory instead of `.gnit/` (or `doctor`
reports legacy nit metadata or a legacy guidance block), it predates the gnit
rename: run `gnit migrate` once. It moves `.nit/` to `.gnit/`, refreshes the
agent guidance block, and commits the result; re-running is a no-op.
