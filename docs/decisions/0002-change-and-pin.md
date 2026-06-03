# 0002: Change And Pin Are Co-Primary

## Status

Accepted.

## Decision

Nit has two co-primary v1 primitives:

- **Change** groups ordinary Git commits across repos with a `Nit-Change-Id`
  trailer.
- **Pin** records an exact, committed, reproducible cross-repo snapshot.

`nit checkout <pin>` materializes a Pin. `nit log` reconstructs the shared graph
from Change trailers and Pin artifacts.

## Rationale

Change answers "which commits belong together?" Pin answers "which exact commits
should this workspace materialize?" Submodules collapse these concerns into
manual parent pointer updates; Nit keeps them explicit.

