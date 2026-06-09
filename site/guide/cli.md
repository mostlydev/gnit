# CLI

Gnit deliberately mirrors Git where the existing verb is clear and adds verbs
only when the operation is a new workspace-level concept.

```sh
gnit init
gnit clone <workspace-url> [path] [--pin <pin>]
gnit adopt <path>...
gnit ignore <path>...
gnit import-submodule <path> [--id <id>]
gnit add <path>...
gnit add -A
gnit commit -m <msg>
gnit land [<name>] -m <msg>
gnit checkout <pin> [--exact]
gnit push [--resume]
gnit pr [--change <id>|--pin <pin>] [--base <branch>]
gnit pr open [--change <id>|--pin <pin>] [--base <branch>] [--title <title>] [--branch <branch>] [--ready]
gnit review <change-id|pin>
gnit status
gnit log
gnit doctor
gnit pin <name>
gnit pin <name> --change <change-id>
gnit change status <id>
gnit change show <id>
gnit change log [<id>]
gnit change diff <id>
gnit update --dry-run
gnit update --check
gnit skills install [<harness>...] [--all] [--copy|--link] [--print] [--force]
gnit skills uninstall [<harness>...] [--all] [--print]
gnit skills list
```

The implemented CLI can create a workspace, adopt existing repos, preserve local
excludes, clone and hydrate a workspace, convert a submodule to a member, stage
workspace paths, commit staged root/member changes under one `Gnit-Change-Id`,
land a change with a Pin, inspect trailer-based changes, record committed member
HEADs as a Pin, materialize Pins with safe checkout defaults, push members
before workspace metadata, create/adopt linked GitHub PRs, render combined
review output, repair local excludes and refresh agent guidance with `doctor`,
follow the explicit update path, and install its bundled agent skill into
supported harnesses.

The v0 human workflow is intentionally small:

```sh
gnit init
gnit clone <workspace-url> [path] [--pin <pin>]
gnit adopt <path>... [--id <id>] [--no-commit]
gnit import-submodule <path> [--id <id>]
gnit ignore <path>...
gnit doctor
gnit migrate

gnit status

gnit add <path>...
gnit add -A
gnit commit -m <msg>
gnit land [<name>] -m <msg>
gnit push
gnit pr
gnit pr open

gnit pin <name>
gnit checkout <pin>
gnit checkout <pin> --exact
gnit review <change|pin>
```

`gnit land` is the commit-plus-pin publish verb. The decomposed form is
`gnit commit -m <msg>` followed by `gnit pin <name>` when you intentionally want
separate steps.

`gnit add` and `gnit commit` honor the Git index in every member, root included:
`gnit commit` records exactly what you staged and leaves unstaged tracked changes
in place, so you can split independent work into separate Changes from one dirty
worktree. Staged workspace metadata under `.gnit` is never folded into a Change —
Gnit commits its own metadata separately and tells you to unstage it if you try.

`gnit push` publishes members first and the workspace root/control repo last. It
prints a report for every target: pushed, already landed, failed, not attempted,
or held back. If a member fails, the root stays unpublished so Pins do not point
at missing member commits. Root metadata is also held back when a current
member's pinned commit is not reachable from local member `HEAD` or an
`origin/*` remote-tracking ref. Retry with `gnit push` or `gnit push --resume`;
both use the same strict ordered policy, and `--resume` is just the explicit
retry spelling.

`gnit pr` shows the linked GitHub PR projection for the current Change. It is
read-only and degrades when GitHub is unavailable by keeping local branch/change
information visible and marking remote PR/check state unknown. `gnit pr open`
preflights every PR-capable participant, then creates missing draft PRs, adopts
existing same-head PRs, and refreshes the Gnit-owned marker block in each PR body.
The common case has no required flags after `gnit push`; Gnit derives the Change,
head branch, base branch, and title from Git. Use `--change`, `--pin`, `--base`,
`--title`, `--branch`, or `--ready` only when the derived default is not right.
`--branch` is a last-resort head override; after `gnit push` every participant is
already on a published branch, so the common path never needs it.

`gnit pr` projects the workspace Change onto one row per repo. A member with no PR
yet shows `missing`; a metadata-only root is labelled `root (metadata)`:

```text
$ gnit pr
Workspace change GCH-1780970169140-18d6
repo                         branch              base        pr        state     checks
root (metadata)              feature/pr-flow     master      #1        open      pending
sdk                          feature/pr-flow     master      #2        open      pass
app                          feature/pr-flow     master      missing   -         -
```

`gnit pr open` creates only what is missing, adopts an existing same-branch PR,
and reports per repo. Re-running is safe — already-open PRs are refreshed, not
duplicated:

```text
$ gnit pr open
Opening PRs for Change GCH-1780970169140-18d6
Title: Add linked PR flow
Mode: draft
  root                     already open
  sdk                      already open
  app                      created
PRs synchronized.
```

Every blocker stops before any PR is created and prints the exact command to fix
it. A participant whose pushed branch is behind its local HEAD:

```text
$ gnit pr open
Error: pr open blocked before creating PRs:
  root: origin/feature/not-pushed is not at local HEAD 85ebde94719d; run `gnit push` before `gnit pr open`
  sdk: origin/feature/not-pushed is not at local HEAD f799018b09e6; run `gnit push` before `gnit pr open`
  app: origin/feature/not-pushed is not at local HEAD 82373d353a2d; run `gnit push` before `gnit pr open`
```

And an ambiguous branch carrying more than one Change refuses rather than
guessing, naming the ids to pick from:

```text
$ gnit pr
Error: multiple Gnit Changes found on the PR branch (GCH-1780970236148-2c2c, GCH-1780970236230-2c78); rerun with `gnit pr --change <id>`
```

`gnit checkout <pin>` materializes exact member commits and refuses dirty member
worktrees unless `--exact` is passed. When the pinned commit is the tip of a
local branch, Gnit checks out that branch. When it is the tip of a remote branch,
Gnit creates or fast-forwards the local branch safely. If no branch points at the
commit, Gnit detaches HEAD and prints a warning instead of hiding the state.

`gnit status` is grouped and legible: it shows root and member staged / modified /
untracked counts, the branch, `missing locally`, member `drifted from pin`, plus
any discovered-but-unadopted nested repos. `gnit log` renders one newest-first
timeline of Changes and Pins across the workspace — the retrievable shared graph
as a single command.

Every command first runs a transparent, non-destructive upkeep pass that repairs
the root repo's local `.git/info/exclude` from the roster (local excludes are not
committed, so a fresh clone needs them reapplied). It is fast, silent on a no-op,
and hits no network. Disable it with `--no-upkeep` or `GNIT_NO_UPKEEP=1`.

Official release builds also keep a cached update notice. Normal commands read
only the local cache; if it is stale, Gnit may schedule a bounded background
refresh and keeps going. That refresh only runs for interactive official builds
outside CI. A newer cached version prints a one-line `gnit update` hint at most
once per day. Disable upkeep with `--no-upkeep` or `GNIT_NO_UPKEEP=1`.

`gnit update` follows the release installer path and is the explicit update
command. `gnit update --check` refreshes release metadata without replacing the
binary. Gnit does not auto-update.

`gnit skills install` installs the bundled Gnit skill into Claude Code, Codex,
OpenCode, and Grok Build. By default it links each harness to a Gnit-managed skill
source under the Gnit data directory; `--copy` writes standalone snapshots.
`--all` targets detected harness config directories, while explicit harness
names such as `claude`, `codex`, `opencode`, or `grok-build` create their
missing harness directories as needed. Gnit never clobbers a non-Gnit-owned
`skills/gnit` target unless `--force` is passed.

`gnit init` also writes a short, version-stable workspace note into the repo's
agent-instruction docs — `AGENTS.md`, plus `CLAUDE.md` when that file already
exists — so any agent reading the file it scans first learns to drive cross-repo
work with the `gnit` CLI and skill instead of hand-managing repos with raw Git.
The note is bounded by `<!-- gnit:workspace:start -->` / `<!-- gnit:workspace:end
-->` markers, so re-running never duplicates it and any edits you make inside the
block survive. `gnit doctor` reports `agent guidance: ok` when the block is present
and `agent guidance: added` when it inserts a missing one, mirroring the local
exclude repair. The wording carries no command or version detail, so it survives
releases without churn. Gnit writes these docs only on the explicit `gnit init` and
`gnit doctor` invocations — never during silent upkeep.

`gnit migrate` converts a workspace created before the gnit rename: it moves
`.nit/` to `.gnit/` (via `git mv` when tracked), replaces the legacy
`<!-- nit:workspace -->` guidance block with the current one, and commits the
result as a single metadata commit. `gnit doctor` points at it when it sees
legacy `.nit/` metadata or a leftover legacy guidance block. Re-running
`gnit migrate` is a no-op. Commits keep their old `Nit-Change-Id` trailers and
are not regrouped; new commits use `Gnit-Change-Id`.
