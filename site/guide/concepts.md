# Concepts

## Roster

The roster records which repositories belong to the workspace and where they
live. It changes rarely.

## Change

A Change groups ordinary Git commits across member repositories with a
`Nit-Change-Id` trailer.

## Pin

A Pin is a committed cross-repo snapshot. It records exact member commits and
provenance so a workspace can be reconstructed later.

## Checkout

`nit checkout <pin>` materializes a Pin safely. It fetches, verifies
reachability, and refuses destructive resets unless `--exact` and policy allow
them. When the pinned commit is the tip of a branch, Nit checks out that branch
(creating or fast-forwarding a local branch from its remote when that is safe);
when no branch points at the commit, it detaches HEAD and warns instead of
hiding the state.

## Review

`nit review <change|pin>` produces a combined cross-repo review artifact.

