# 0013: Agent guidance in workspace docs

## Status

Accepted. Implementation handed to Codex.

## Context

A freshly nitted repository looks like an ordinary Git repo to an agent. Without
a hint, harnesses reach for raw `git` or submodule commands and fight the
workspace instead of driving it. Gnit already ships an agent skill (0011), but the
skill only helps once a harness has loaded it; the repository itself should also
announce that it is a Gnit workspace, in the file agents read first.

`AGENTS.md` is the emerging cross-harness convention; `CLAUDE.md` is the
Claude-specific equivalent. Both are plain Markdown that agents scan on entry.

## Decision

On `gnit init`, Gnit ensures a short, version-stable guidance block exists in the
workspace's agent-instruction docs. The block is worded so it never needs to
change as Gnit gains commands or cuts releases: it names *why* (multi-repo) and
points at two stable anchors — `gnit --help` (always current) and "the Gnit skill"
(a pointer whose content can evolve) — and deliberately lists no subcommands or
version numbers.

### The block

    <!-- gnit:workspace:start -->
    > **Gnit workspace** — this repository is one of several Git repos coordinated by Gnit.
    > For changes that span more than one repo, drive them with the `gnit` CLI and the Gnit
    > skill (run `gnit --help`) instead of hand-managing submodules or raw `git` across repos.
    <!-- gnit:workspace:end -->

The HTML-comment markers render invisibly and delimit a Gnit-managed region. They
give exact presence detection and a safe migration handle if the canonical text
must ever change, without parsing prose.

### Target files

- If the repo has neither file, create `AGENTS.md`.
- If `AGENTS.md` exists, ensure the block there.
- If `CLAUDE.md` exists, ensure the block there too.

So a repo with both gets it in both; a repo with neither gets a new `AGENTS.md`
only — Gnit never creates a Claude-specific file in a repo that is not already
using one. The block is appended (preceded by a blank line) to the end of an
existing file, leaving the user's own content untouched.

### Idempotency and respect for edits

Detection is by the `<!-- gnit:workspace:start -->` marker. If present, Gnit makes
no change — it never rewrites the region, so any user edits inside it survive. If
absent, Gnit inserts the block. Re-running `gnit init` or `gnit doctor` therefore
never duplicates it.

### When it runs

- `gnit init` writes the block automatically and includes the new or changed doc
  in the same metadata commit, so the workspace is self-documenting from its
  first commit. In `--local` mode the file is written but not committed.
- `gnit doctor` reports `agent guidance: ok` when the block is present and
  `agent guidance: added` when it inserts a missing one — mirroring the existing
  `.git/info/exclude` repair. This is the "offer/update if missing" path.

Gnit does **not** insert or re-add the block during silent transparent upkeep or
on ordinary commands. `init` and `doctor` are explicit user invocations, which
keeps this consistent with the project rule (0011) that Gnit changes the user's
files only when asked.

## Consequences

- A cloned or freshly initialized workspace tells any agent, in the file it reads
  first, to use Gnit rather than raw Git.
- The wording carries no version- or feature-specific detail, so it survives Gnit
  releases without churn; if it ever must change, the markers bound a safe
  in-place migration.
- `gnit init`'s commit now includes `AGENTS.md` (and/or `CLAUDE.md`); the existing
  init test that asserts the metadata commit contents must be updated.

## Rejected Alternatives

- **Silent upkeep insertion on every command.** Rejected: it mutates user-owned
  docs without an explicit ask, violating the 0011 "only when asked" rule.
- **Rewriting the managed region to keep it in sync.** Rejected for the default
  path: it would clobber user edits. The text is version-stable by design, so
  insert-if-absent is enough; the markers remain available for a deliberate
  future migration.
- **Enumerating key commands in the block.** Rejected: it would rot across
  releases. Pointing at `gnit --help` keeps it current for free.
