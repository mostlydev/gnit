---
name: nit
description: Use when working in a Nit workspace (a `.nit/` directory or `.nit/roster.yaml` is present), or when a single change spans several independent Git repositories. Teaches the Change / Pin / branch-aware-checkout / ordered-push workflow through the `nit` CLI instead of hand-managing submodules or raw Git across repos.
---

# Driving Nit

Nit is a Git-native workspace layer for changes that span multiple independent
repositories. Each member stays an ordinary Git repo — its own remote, branches,
and history. Nit groups a cross-repo change under one id, snapshots exact commits
as a reproducible Pin, and publishes everything in a safe order. It shells out to
`git` and keeps Git as the source of truth.

Use this skill when the workspace has a `.nit/` directory, when the user asks to
commit/pin/checkout/push across repos, or when one logical change touches more
than one repository.

## Core model

- **Member**: an ordinary Git repo registered in the workspace roster.
- **Change**: commits across members that share one `Nit-Change-Id` trailer.
- **Pin**: a committed, reproducible snapshot of exact member commits.
- **Checkout**: safe materialization of a Pin across the workspace.

Nit never rewrites member repos. Walk away and you still have plain Git.

## The workflow

```sh
# Build a workspace from a directory of related repos.
nit init                 # or: nit init --control   (no natural root repo)
nit adopt app sdk infra  # register existing repos as members (each a repo root)

# Make one change across several repos.
nit add app/src sdk/src  # stage across members (or: nit add -A)
nit commit -m "Wire the new field end to end"   # one Nit-Change-Id across repos
nit push                 # publish members first, then workspace metadata

# Or publish a reproducible snapshot in one step.
nit land -m "Release the new field"   # commit + pin together
nit push
```

Inspect and reconstruct:

```sh
nit status               # root/members, staged/modified/untracked, pin drift, discovered repos
nit log                  # unified newest-first timeline of Changes and Pins
nit change show <id>     # the commits that make up a Change
nit review <id-or-pin>   # combined review artifact
nit clone <url> dir --pin <label>   # rebuild a workspace and materialize a Pin
```

## Rules that matter

- **Prefer `nit` verbs over raw Git for cross-repo actions.** Use `nit add`,
  `nit commit`, `nit land`, `nit push`, `nit checkout`. Per-repo Git is fine
  inside a single member; reach for Nit when the action spans members or touches
  workspace metadata under `.nit/`.
- **Push is ordered and safe to retry.** `nit push` publishes members in roster
  order, then the workspace root/control repo last, and holds the root back if
  any member fails or if a Pin references a member commit that is not yet pushed.
  After fixing the member, run `nit push` again, or `nit push --resume` as the
  explicit retry spelling. Nit never force-pushes; a non-fast-forward is a hard
  failure to resolve in the member, not to override.
- **Pins require clean members.** `nit pin` and `nit land` refuse a dirty member
  worktree, because a Pin must capture exact, reproducible commits. Commit or
  stash member changes first.
- **Checkout is branch-aware and refuses to clobber.** `nit checkout <pin>` stays
  on a branch when the pinned commit is a branch tip and only detaches when it
  must, warning clearly. It refuses to overwrite dirty member worktrees unless
  you pass `--exact`, which resets and cleans them — treat `--exact` as
  destructive and confirm intent before using it.
- **Let metadata commits happen.** Commands like `nit adopt`, `nit pin`, and
  `nit land` auto-commit `.nit/` metadata. Pass `--no-commit` only when you mean
  to stage the metadata change yourself.
- **Upkeep is automatic and non-destructive.** Nit repairs local excludes quietly
  on each command. Do not hand-edit `.nit/` machinery to work around it.
- **Updating is explicit.** `nit update` replaces the binary on request; Nit
  never auto-updates. `nit doctor` diagnoses the install and workspace.

## When something is off

Run `nit status` and `nit doctor` first. `status` shows per-member state, drift
from the current Pin, and nested repos that are not yet adopted. `doctor` reports
trailer, pin, exclude, and remote-drift problems. Fix the named member with
ordinary Git, then re-run the Nit command.
