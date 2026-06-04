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
```

The implemented CLI can create a workspace, adopt existing repos, preserve local
excludes, clone and hydrate a workspace, convert a submodule to a member, stage
workspace paths, commit staged root/member changes under one `Nit-Change-Id`,
land a change with a Pin, inspect trailer-based changes, record committed member
HEADs as a Pin, materialize Pins with safe checkout defaults, push members
before workspace metadata, render combined review output, repair local excludes
with `doctor`, and follow the explicit update path.

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

nit pin <name>
nit checkout <pin>
nit checkout <pin> --exact
nit review <change|pin>
```

`nit land` is the commit-plus-pin publish verb. The decomposed form is
`nit commit -m <msg>` followed by `nit pin <name>` when you intentionally want
separate steps.

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
