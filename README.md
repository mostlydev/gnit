# Nit

Nit is a Git-native workspace layer for changes that span multiple independent
repositories.

The current design is in [docs/planning/nit-design.md](docs/planning/nit-design.md).
It defines the v1 primitives:

- **Change**: a logical cross-repo change grouped by `Nit-Change-Id`.
- **Pin**: a committed, reproducible snapshot of exact member repo commits.
- **Checkout**: safe materialization of a Pin across the workspace.
- **Review**: a combined review artifact for a Change or Pin.

The public documentation site lives in [site/](site/). It is a VitePress site
with a GitHub Pages workflow in [.github/workflows/deploy-site.yml](.github/workflows/deploy-site.yml).
The workflow is ready to deploy on pushes to `master` once a remote repository
and Pages settings are configured.

## Repository Layout

```text
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

