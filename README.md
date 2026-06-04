# Nit

Nit is a Git-native workspace layer for changes that span multiple independent
repositories.

The current design is in [docs/planning/nit-design.md](docs/planning/nit-design.md).
It defines the v1 primitives:

- **Change**: a logical cross-repo change grouped by `Nit-Change-Id`.
- **Pin**: a committed, reproducible snapshot of exact member repo commits.
- **Checkout**: safe materialization of a Pin across the workspace.
- **Review**: a combined review artifact for a Change or Pin.

The CLI implementation is Rust. The current roadmap is in
[docs/planning/implementation-roadmap.md](docs/planning/implementation-roadmap.md).

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/mostlydev/nit/master/install.sh | sh
```

This downloads the latest release for your platform, verifies its SHA-256
checksum, and installs `nit` to `~/.local/bin` (override with `NIT_INSTALL_DIR`).
It requires `git` and `curl`. Verify the install with:

```sh
nit doctor
```

Update later with `nit update`, which re-runs the verified installer. Nit never
auto-updates; it only updates when you ask.

## Quickstart

```sh
# Turn a directory of related repos into one workspace.
cd my-workspace
nit init                 # or: nit init --control  (when there is no root repo)
nit adopt app sdk infra  # register existing repos as members (each a repo root)

# Make one change across several repos.
nit add app/src sdk/src  # stage paths across members (or: nit add -A)
nit commit -m "Wire the new field end to end"   # one Nit-Change-Id across repos
nit push                 # publish members first, then workspace metadata

# Or publish a reproducible snapshot (a Pin) in one step.
nit land -m "Release the new field"   # commit + pin together
nit push

nit status               # members, staged/modified/untracked, pin drift, discovered repos
nit log                  # unified timeline of changes and pins
nit change show <id>     # the commits that make up a change
nit review <id-or-pin>   # combined review artifact
```

Reconstruct the workspace on another machine:

```sh
nit clone git@github.com:example/product-workspace.git product --pin baseline
```

See the [full guide](https://mostlydev.github.io/nit/) for clone, pins,
checkout, review, and the design rationale.

The public documentation site is live at **https://mostlydev.github.io/nit/**.
It lives in [site/](site/) as a VitePress site and redeploys via
[.github/workflows/deploy-site.yml](.github/workflows/deploy-site.yml) on every
push to `master` that touches `site/**` or the workflow. The build sets
`VITEPRESS_BASE=/nit/` for the project-page path; if a custom domain (e.g.
`nit.dev`) is added later, set the base to `/` and add `site/public/CNAME`.

## Repository Layout

```text
src/                Rust CLI implementation.
tests/              CLI integration tests.
install.sh          Release installer (used by `nit update`).
docs/
  decisions/        Locked product and design decisions.
  planning/         Design plans and archived drafts.
site/               Public VitePress documentation site.
```

## Local Site

```sh
cd site
npm install
npm run build
npm run dev
```

If the site is deployed as a GitHub Pages project site instead of a custom
domain, set `VITEPRESS_BASE=/<repo-name>/` for the build.

## CLI Development

```sh
cargo test
cargo run -- doctor
cargo run -- status
```
