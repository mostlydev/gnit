# Design

The authoritative design lives in the repository at
`docs/planning/nit-design.md`.

The locked decisions are recorded in `docs/decisions/`:

- Name and scope: `nit`.
- Change and Pin are co-primary.
- `nit land` is the canonical human publish verb.
- Hooks are not installed by default.
- The public site deploys from `site/` on pushes to `master` once GitHub Pages is
  configured.
- The CLI is Rust, distributed as a single binary, with Clawdapus-style
  install/update/release ergonomics.

The central principle is reconstruct-not-enforce: Git remains the source of
truth, and Nit rebuilds the workspace graph from ordinary Git commits, trailers,
the roster, and Pin artifacts.
