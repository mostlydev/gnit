# CLI

Nit deliberately mirrors Git where the existing verb is clear and adds verbs
only when the operation is a new workspace-level concept.

```sh
nit init
nit clone <workspace-url> [path] [--pin <pin>]
nit adopt <path>...
nit ignore <path>...
nit import-submodule <path> [--id <id>]
nit add <path>...
nit add -A
nit commit -m <msg>
nit land [<name>] -m <msg>
nit checkout <pin> [--exact]
nit push [--resume]
nit pr [--change <id>|--pin <pin>] [--base <branch>]
nit pr open [--change <id>|--pin <pin>] [--base <branch>] [--title <title>] [--branch <branch>] [--ready]
nit review <change-id|pin>
nit status
nit log
nit doctor
nit pin <name>
nit pin <name> --change <change-id>
nit change status <id>
nit change show <id>
nit change log [<id>]
nit change diff <id>
nit update --dry-run
nit update --check
nit skills install [<harness>...] [--all] [--copy|--link] [--print] [--force]
nit skills uninstall [<harness>...] [--all] [--print]
nit skills list
```

The implemented CLI can create a workspace, adopt existing repos, preserve local
excludes, clone and hydrate a workspace, convert a submodule to a member, stage
workspace paths, commit staged root/member changes under one `Nit-Change-Id`,
land a change with a Pin, inspect trailer-based changes, record committed member
HEADs as a Pin, materialize Pins with safe checkout defaults, push members
before workspace metadata, create/adopt linked GitHub PRs, render combined
review output, repair local excludes and refresh agent guidance with `doctor`,
follow the explicit update path, and install its bundled agent skill into
supported harnesses.

The v0 human workflow is intentionally small:

```sh
nit init
nit clone <workspace-url> [path] [--pin <pin>]
nit adopt <path>... [--id <id>] [--no-commit]
nit import-submodule <path> [--id <id>]
nit ignore <path>...
nit doctor

nit status

nit add <path>...
nit add -A
nit commit -m <msg>
nit land [<name>] -m <msg>
nit push
nit pr
nit pr open

nit pin <name>
nit checkout <pin>
nit checkout <pin> --exact
nit review <change|pin>
```

`nit land` is the commit-plus-pin publish verb. The decomposed form is
`nit commit -m <msg>` followed by `nit pin <name>` when you intentionally want
separate steps.

`nit add` and `nit commit` honor the Git index in every member, root included:
`nit commit` records exactly what you staged and leaves unstaged tracked changes
in place, so you can split independent work into separate Changes from one dirty
worktree. Staged workspace metadata under `.nit` is never folded into a Change —
Nit commits its own metadata separately and tells you to unstage it if you try.

`nit push` publishes members first and the workspace root/control repo last. It
prints a report for every target: pushed, already landed, failed, not attempted,
or held back. If a member fails, the root stays unpublished so Pins do not point
at missing member commits. Retry with `nit push` or `nit push --resume`; both use
the same strict ordered policy, and `--resume` is just the explicit retry
spelling.

`nit pr` shows the linked GitHub PR projection for the current Change. It is
read-only and degrades when GitHub is unavailable by keeping local branch/change
information visible and marking remote PR/check state unknown. `nit pr open`
preflights every PR-capable participant, then creates missing draft PRs, adopts
existing same-head PRs, and refreshes the Nit-owned marker block in each PR body.
The common case has no required flags after `nit push`; Nit derives the Change,
head branch, base branch, and title from Git. Use `--change`, `--pin`, `--base`,
`--title`, `--branch`, or `--ready` only when the derived default is not right.
`--branch` is a last-resort head override; after `nit push` every participant is
already on a published branch, so the common path never needs it.

`nit pr` projects the workspace Change onto one row per repo. A member with no PR
yet shows `missing`; a metadata-only root is labelled `root (metadata)`:

```text
$ nit pr
Workspace change NCH-1780970169140-18d6
repo                         branch              base        pr        state     checks
root (metadata)              feature/pr-flow     master      #1        open      pending
sdk                          feature/pr-flow     master      #2        open      pass
app                          feature/pr-flow     master      missing   -         -
```

`nit pr open` creates only what is missing, adopts an existing same-branch PR,
and reports per repo. Re-running is safe — already-open PRs are refreshed, not
duplicated:

```text
$ nit pr open
Opening PRs for Change NCH-1780970169140-18d6
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
$ nit pr open
Error: pr open blocked before creating PRs:
  root: origin/feature/not-pushed is not at local HEAD 85ebde94719d; run `nit push` before `nit pr open`
  sdk: origin/feature/not-pushed is not at local HEAD f799018b09e6; run `nit push` before `nit pr open`
  app: origin/feature/not-pushed is not at local HEAD 82373d353a2d; run `nit push` before `nit pr open`
```

And an ambiguous branch carrying more than one Change refuses rather than
guessing, naming the ids to pick from:

```text
$ nit pr
Error: multiple Nit Changes found on the PR branch (NCH-1780970236148-2c2c, NCH-1780970236230-2c78); rerun with `nit pr --change <id>`
```

`nit checkout <pin>` materializes exact member commits and refuses dirty member
worktrees unless `--exact` is passed. When the pinned commit is the tip of a
local branch, Nit checks out that branch. When it is the tip of a remote branch,
Nit creates or fast-forwards the local branch safely. If no branch points at the
commit, Nit detaches HEAD and prints a warning instead of hiding the state.

`nit status` is grouped and legible: it shows root and member staged / modified /
untracked counts, the branch, `missing locally`, member `drifted from pin`, plus
any discovered-but-unadopted nested repos. `nit log` renders one newest-first
timeline of Changes and Pins across the workspace — the retrievable shared graph
as a single command.

Every command first runs a transparent, non-destructive upkeep pass that repairs
the root repo's local `.git/info/exclude` from the roster (local excludes are not
committed, so a fresh clone needs them reapplied). It is fast, silent on a no-op,
and hits no network. Disable it with `--no-upkeep` or `NIT_NO_UPKEEP=1`.

Official release builds also keep a cached update notice. Normal commands read
only the local cache; if it is stale, Nit may schedule a bounded background
refresh and keeps going. That refresh only runs for interactive official builds
outside CI. A newer cached version prints a one-line `nit update` hint at most
once per day. Disable upkeep with `--no-upkeep` or `NIT_NO_UPKEEP=1`.

`nit update` follows the release installer path and is the explicit update
command. `nit update --check` refreshes release metadata without replacing the
binary. Nit does not auto-update.

`nit skills install` installs the bundled Nit skill into Claude Code, Codex,
OpenCode, and Grok Build. By default it links each harness to a Nit-managed skill
source under the Nit data directory; `--copy` writes standalone snapshots.
`--all` targets detected harness config directories, while explicit harness
names such as `claude`, `codex`, `opencode`, or `grok-build` create their
missing harness directories as needed. Nit never clobbers a non-Nit-owned
`skills/nit` target unless `--force` is passed.

`nit init` also writes a short, version-stable workspace note into the repo's
agent-instruction docs — `AGENTS.md`, plus `CLAUDE.md` when that file already
exists — so any agent reading the file it scans first learns to drive cross-repo
work with the `nit` CLI and skill instead of hand-managing repos with raw Git.
The note is bounded by `<!-- nit:workspace:start -->` / `<!-- nit:workspace:end
-->` markers, so re-running never duplicates it and any edits you make inside the
block survive. `nit doctor` reports `agent guidance: ok` when the block is present
and `agent guidance: added` when it inserts a missing one, mirroring the local
exclude repair. The wording carries no command or version detail, so it survives
releases without churn. Nit writes these docs only on the explicit `nit init` and
`nit doctor` invocations — never during silent upkeep.
