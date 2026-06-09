# Concepts

## Roster

The roster records which repositories belong to the workspace and where they
live. It changes rarely.

## Change

A Change groups ordinary Git commits across member repositories with a
`Gnit-Change-Id` trailer.

## Pin

A Pin is a committed cross-repo snapshot. It records exact member commits and
provenance so a workspace can be reconstructed later.

## Checkout

`gnit checkout <pin>` materializes a Pin safely. It fetches, verifies
reachability, and refuses destructive resets unless `--exact` and policy allow
them. When the pinned commit is the tip of a branch, Gnit checks out that branch
(creating or fast-forwarding a local branch from its remote when that is safe);
when no branch points at the commit, it detaches HEAD and warns instead of
hiding the state.

## Review

`gnit review <change|pin>` produces a combined cross-repo review artifact. It is
local-only by default: when a pinned member commit is not available locally, Gnit
prints an explicit `gnit checkout <pin>` or member `git fetch origin` remediation
instead of fetching automatically.
