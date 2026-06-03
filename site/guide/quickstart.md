# Quickstart

Nit has an early Rust CLI for workspace creation, adoption, status, diagnosis,
and explicit updates. The full publish workflow below is the intended day-one
surface as the remaining verbs land.

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

## Reconstruct A Workspace

```sh
nit clone git@github.com:example/product-workspace.git product --pin baseline
```

`nit clone` clones the control repo and hydrates member repos from the roster.
With `--pin`, it also materializes the selected Pin.
