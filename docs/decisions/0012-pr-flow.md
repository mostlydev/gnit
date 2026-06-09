# 0012: GitHub PR Flow

## Status

Accepted.

## Decision

Nit projects a workspace Change onto ordinary GitHub pull requests with two
commands:

```sh
nit pr
nit pr open
```

`nit pr` is read-only status. It is the safe first debugging command and must
degrade when GitHub is unavailable by showing the local repo / branch / Change
projection with PR state marked unknown.

`nit pr open` is idempotent. It creates missing PRs, adopts an existing same-head
manual PR, and refreshes the Nit-owned body marker on every linked PR. It opens
draft PRs by default; `--ready` opts out.

The single durable join key is `Nit-Change-Id`. `--pin <pin>` is only an alias
when that Pin records exactly one provenance Change; the PR relationship still
tracks the Change. A Pin label may appear in the marker as provenance, but it is
not a second projection model.

No-argument operation derives the current Change from commits on the current
published branch since the merge-base with the selected base branch. If zero or
multiple Change ids are present, Nit refuses and prints the explicit
`--change <id>` remediation.

The escape hatches are explicit: `--change`, `--pin`, and `--base` for status or
open; `--title`, `--branch`, and `--ready` only for `nit pr open`.

Nit shells out to `gh` for GitHub operations and uses `gh -R <repo>` for PR
commands so each workspace member targets its own repository. Git remains the
source of truth for branches, commits, remotes, and push state.

## Consequences

- There is one projection engine. A separate `nit pr sync` command is deferred;
  rerunning `nit pr open` performs the reconciliation.
- `nit pr review`, PR comments, reopening closed PRs, and coordinated merge are
  deferred.
- Existing user-authored PR body text is preserved outside:

```markdown
<!-- nit-pr-sync:start -->
...
<!-- nit-pr-sync:end -->
```

- A metadata-only root/control PR can anchor a `nit land` Pin when the root
  branch committed a Pin whose provenance contains the selected Change. This is
  intentionally PR-specific; pin metadata commits do not gain
  `Nit-Change-Id` trailers because that would pollute `nit change` and
  `nit review`.
