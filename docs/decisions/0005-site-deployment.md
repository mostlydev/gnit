# 0005: Site Deployment

## Status

Accepted for local scaffold. Remote deployment is pending a repository name and
Pages configuration.

## Decision

The public documentation site lives in `site/` and uses VitePress. The GitHub
Pages workflow deploys on pushes to `master` when `site/**` or the workflow
changes.

The VitePress base path is controlled by `VITEPRESS_BASE`, defaulting to `/`.
Use `/` for a custom domain and `/<repo-name>/` for a GitHub Pages project site.

## Rationale

This follows the structure used by the Clawdapus site while avoiding a hardcoded
base path before the public repository/deployment target is known.

