# 0002: Change And Pin Are Co-Primary

## Status

Accepted.

## Decision

Gnit has two co-primary v1 primitives:

- **Change** groups ordinary Git commits across repos with a `Gnit-Change-Id`
  trailer.
- **Pin** records an exact, committed, reproducible cross-repo snapshot.

`gnit checkout <pin>` materializes a Pin. `gnit change log` reconstructs the
shared graph from Change trailers.

## Rationale

Change answers "which commits belong together?" Pin answers "which exact commits
should this workspace materialize?" Submodules collapse these concerns into
manual parent pointer updates; Gnit keeps them explicit.
