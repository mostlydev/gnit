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
nit doctor
nit pin <name>
nit pin <name> --change <change-id>
nit change status <id>
nit change show <id>
nit change log [<id>]
nit change diff <id>
nit update --dry-run
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
worktrees unless `--exact` is passed. v0 uses detached checkout for pinned
commits; treat it as reproducible materialization, not a place to continue
normal branch work.

`nit update` follows the release installer path and is the explicit update
command. Official release builds may also self-update transparently after a
verified release check.
