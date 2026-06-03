# Nit

Nit is a Git-native workspace layer for changes that span multiple independent
repositories.

The current design is in [docs/planning/nit-design.md](docs/planning/nit-design.md).
It defines the v1 primitives:

- **Change**: a logical cross-repo change grouped by `Nit-Change-Id`.
- **Pin**: a committed, reproducible snapshot of exact member repo commits.
- **Checkout**: safe materialization of a Pin across the workspace.
- **Review**: a combined review artifact for a Change or Pin.

The public documentation site is live at **https://mostlydev.github.io/nit/**.
It lives in [site/](site/) as a VitePress site and redeploys via
[.github/workflows/deploy-site.yml](.github/workflows/deploy-site.yml) on every
push to `master` that touches `site/**` or the workflow. The build sets
`VITEPRESS_BASE=/nit/` for the project-page path; if a custom domain (e.g.
`nit.dev`) is added later, set the base to `/` and add `site/public/CNAME`.

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

