# 0010: Strict Push Resume

## Status

Accepted. Implemented for v0.5.0.

## Context

Multi-repo push is not atomic. Nit must push member repositories before the
workspace root/control repo, because root metadata can contain Pins that
reference member commits.

The first `nit push --resume` implementation was only a banner. The command was
accidentally idempotent when every repo had already landed, but a real partial
push failure still left users without a report of what landed, what failed, and
what was intentionally held back.

## Decision

`nit push` has one strict ordered policy. `nit push --resume` is an explicit
retry spelling for the same policy, not a second push mode.

- The ordered plan is all roster members in roster order, then the workspace
  root/control repo.
- Remote refs are the resume journal; Nit does not write persistent push state.
- Preflight checks every target for a branch, an `origin`, and a reachable remote.
- Already-landed repos are skipped by comparing local `HEAD` to the remote branch
  tip.
- Repos that still need publishing are pushed in order.
- The first member failure stops later member pushes.
- The root/control repo is pushed last and only if every member has landed.
- If root metadata contains Pins for current roster members, each pinned member
  commit must be reachable from the corresponding local member `HEAD` or from an
  `origin/*` remote-tracking ref before the root can be pushed.
- Non-fast-forward rejection is a hard failure; Nit never forces.
- Every run prints a report that lists pushed, already-landed, failed,
  not-attempted, and held-back targets.
- The command exits non-zero unless every target is landed.

## Consequences

- A failed push can be retried with either `nit push` or `nit push --resume`.
- A partial landing is visible instead of hidden behind the first Git error.
- Nit does not publish a Pin that points at an unpushed or rewritten-away member
  commit.
- Retained historical Pins for retired members do not block today's root push.
- Humans have one push policy to learn.

## Rejected Alternative

`nit push --resume` could have been a best-effort mode that skips already-landed
members, continues past new member failures, and pushes every still-pushable
member while still holding the root back.

That may be useful for agents or CI when independent members should make as much
progress as possible. It is deferred to a future honestly named policy such as
`--keep-going`; it is not the default meaning of "resume."
