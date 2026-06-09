# 0001: Name And Scope

## Status

Accepted.

## Decision

The product name is `gnit`.

Gnit v1 is a Git-native workspace layer for changes that span multiple
independent repositories. It is not a replacement VCS and does not merge member
repositories into one history.

## Rationale

The design goal is a small, memorable command surface for "knitting" related
repositories together while preserving ordinary Git underneath.

Governance concepts such as Aims, Attempts, Evidence, and Collapse remain a v2
layer built on the v1 primitives.

