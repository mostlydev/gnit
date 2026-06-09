# 0003: Human-Facing Verbs

## Status

Accepted.

## Decision

Gnit reuses Git verbs where they are clear, but it keeps new verbs when they name
human-important operations that would otherwise hide behind easy-to-forget
arguments.

The canonical publish flow is:

```sh
gnit add -A
gnit land -m "Publish change"
gnit push
```

`gnit land` remains canonical because it names a distinct transaction: commit
staged member changes, create an unnamed Pin, and prepare ordered push. The
decomposed form remains available as `gnit commit -m <msg>` followed by
`gnit pin <name>` when separate steps are intentional.

New members are created with plain `git init` or `git clone`, then registered
with `gnit adopt`.

## Rationale

Minimizing verbs is not the goal if it forces humans to remember flags whose
absence changes the meaning of the operation.
