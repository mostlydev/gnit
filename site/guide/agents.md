# Gnit for agent harnesses

Multi-repo tooling has historically failed for one reason: it only works if
everyone adopts it. Agents remove that failure mode. A harness that has the
skill installed, or that reads the workspace guidance block, follows the Gnit
workflow on every single invocation. This page covers the three pieces that
make that work.

## Install the skill

The Gnit skill ships inside the binary. Install it into every supported
harness on the machine:

```sh
gnit skills install --all
gnit skills list
```

Supported harnesses are Claude Code, Codex, OpenCode, and Grok Build. The
default mode is `--link`, which points each harness at Gnit's managed skill
source so `gnit update` refreshes everyone at once. Use `--copy` for
standalone snapshots:

```sh
gnit skills install claude codex --copy
```

## The workspace guidance block

`gnit init` drops a short, version-stable note into the repo's
agent-instruction docs — `AGENTS.md`, plus `CLAUDE.md` when that file already
exists. Any agent that reads the file it scans first learns to drive
cross-repo work through the `gnit` CLI instead of hand-managing member repos
with raw Git.

The note lives between `<!-- gnit:workspace:start -->` and
`<!-- gnit:workspace:end -->` markers: re-running `gnit init` never duplicates
it, and edits you make inside survive. `gnit doctor` reports
`agent guidance: ok` when the block is present and re-adds it when missing.

This is the no-onboarding path: an agent that has never seen Gnit lands in the
workspace, reads `AGENTS.md`, and uses the workflow.

## Multiple agents, one workspace

Agent workspaces make concurrent invocations the normal case, not an edge
case. Gnit takes an advisory lock on `.gnit/lock` for the duration of any
mutating command (`commit`, `pin`, `land`, `push`, `checkout`, `adopt`, …), so
two agents — or you and an agent — cannot race on workspace state. Read-only
commands (`status`, `log`, `change`) stay lock-free, and a contended lock
fails fast with a clear message instead of corrupting anything.

Interrupted work recovers cleanly too: if a cross-repo `gnit commit` fails
partway (a pre-commit hook, an index lock), the error names the exact resume
command — `gnit commit --change <id>` — so the change reunifies under one id
instead of silently splitting in two.

## Enforce it in CI

Agents follow instructions; CI proves it. `gnit ci-check` validates on every
PR that member commits carry a well-formed `Gnit-Change-Id` trailer and that
root metadata never publishes a Pin whose member commits are unreachable:

```sh
gnit ci-check --base origin/main --head HEAD
```

A thin composite Action wraps the same binary:

```yaml
- uses: mostlydev/gnit/.github/actions/gnit-ci-check@master
```

Reconstruct locally, enforce at the boundary — the agents do the bookkeeping,
CI rejects drift.
