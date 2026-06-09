# 0009: Branch-Aware Checkout

## Status

Accepted. Shipped in v0.4.0.

## Context

Pins record exact member commits and optional `branch_hint` values. v0.3.0
materialized every pinned member with `git checkout --detach`, even when the
commit was the tip of a normal branch. That reproduced one of the submodule
footguns Gnit is meant to remove: users could start work from a detached HEAD and
lose track of new commits.

## Decision

`gnit checkout <pin>` is branch-aware by default.

- Dirty member worktrees still fail unless `--exact` is passed.
- If the pinned commit is the tip of a local branch, Gnit checks out that branch.
- If the pinned commit is the tip of a remote branch and the local branch is
  missing, Gnit creates a tracking branch and checks it out.
- If the local branch exists and can fast-forward to the pinned commit, Gnit
  fast-forwards it and checks it out.
- If no safe branch points at the pinned commit, Gnit detaches HEAD and prints a
  warning that names the member and commit.
- `--exact` cleans uncommitted work before materialization but does not secretly
  reset the current branch ref to the pinned commit before detaching.

## Consequences

- Common pinned checkouts stay on normal Git branches.
- Reproducible checkout still works for commits that are not branch tips.
- Detached HEAD is explicit and visible instead of a hidden default.
- Push-resume and partial-landing reports are covered separately in
  [0010](0010-strict-push-resume.md).
