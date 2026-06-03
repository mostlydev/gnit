# Quickstart

Turn a directory of related repos into one workspace, then commit and publish
across all of them at once. The Rust CLI now ships the v0 loop: construct a
workspace, land a cross-repo change, push in dependency-safe order, clone it on a
fresh machine, and review the combined artifact.

## Create A Workspace

```sh
mkdir product && cd product
nit init --control --remote git@github.com:example/product-workspace.git

git clone git@github.com:example/app.git app
git clone git@github.com:example/sdk.git sdk
git clone git@github.com:example/docs.git docs

nit adopt app sdk docs
nit pin baseline
nit push
```

## Publish A Cross-Repo Change

```sh
nit add -A
nit land -m "Publish webhook retry update"
nit push
```

`nit land` is the human-facing publish verb. It commits staged member changes,
creates an unnamed Pin, and lets `nit push` publish member commits before the Pin.

```sh
nit review <change-id-or-pin>
```

## Reconstruct A Workspace

```sh
nit clone git@github.com:example/product-workspace.git product --pin baseline
```

`nit clone` clones the control repo and hydrates member repos from the roster.
With `--pin`, it also materializes the selected Pin.

Pinned checkout is intentionally exact. v0 materializes member commits in
detached HEAD state and refuses to overwrite dirty member worktrees unless you
use `nit checkout <pin> --exact`.
