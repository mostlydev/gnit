# CLI

Nit deliberately mirrors Git where the existing verb is clear and adds verbs
only when the operation is a new workspace-level concept.

```sh
nit init
nit clone <workspace-url> [path] [--pin <pin>]
nit adopt <path>... [--id <id>] [--no-commit]
nit import-submodule <path> [--id <id>]
nit ignore <path>
nit doctor

nit status
nit log

nit add <path>...
nit add -A
nit commit -m <msg>
nit land [<name>] -m <msg>
nit commit --pin [<name>] -m <msg>
nit push

nit pin <name>
nit checkout <pin>
nit checkout <pin> --exact
nit review <change|pin>
```

`nit commit --pin` is the scriptable equivalent of `nit land`. The canonical
human workflow uses `nit land` because forgetting `--pin` changes the meaning of
the operation.

