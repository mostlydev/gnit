---
name: nit-release
description: >
  Cuts a Nit release: runs pre-release checks (fmt, clippy, tests, clean tree,
  auth, fetched tags), proposes the next semver from the commits since the last
  tag, sweeps docs (README, site guide, the bundled CLI skill) for release-tied
  updates, bumps the version in Cargo.toml and Cargo.lock, commits and pushes the
  bump to master, then tags vX.Y.Z and pushes the tag so the tag-driven release
  workflow builds the binaries and publishes the GitHub release. Use this skill
  whenever the user says "release", "cut a release", "new version", "bump the
  version", "ship vX.Y.Z", or anything about shipping a new version of nit.
---

# Nit release

This skill cuts a release of the `nit` CLI. The pipeline is **tag-driven**:
pushing a `v*` tag triggers `.github/workflows/release.yml`, which builds the
binaries for every target and publishes the GitHub release. Two other workflows
react to the same push to `master`: `ci.yml` (fmt + clippy + tests on Ubuntu and
macOS) and `deploy-site.yml` (redeploys the VitePress site).

Nit is a single Rust binary with no submodules, no Docker images, and no runtime
infra pins — so a release is just: get `master` green and version-bumped, push
it, then push the tag. The release workflow does the building and publishing.

Your job is to prepare everything locally, push the version-bump commit to
`master`, then push the release tag and verify the workflows go green.

## Step 1: Pre-release sanity checks

```bash
# Clean working tree — don't fold unrelated work into a release.
git status --short

# Local tags lag the remote badly; sync before you reason about versions.
git fetch --tags

# Releases publish via gh in the workflow, but check your own auth for the
# manual steps (release notes, verification).
gh auth status

# Run the exact gate CI runs on the tag. If this fails locally, the release
# build fails too.
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
```

If the working tree is dirty, decide with the user whether those changes belong
in this release or should be committed/stashed separately.

CI runs clippy with the **current stable** toolchain and `-D warnings`. If your
local toolchain lags, a lint that is clean locally can fail CI on the tag. Run
`rustup update stable && cargo clippy --all-targets -- -D warnings` before
tagging to catch newer lints in one pass.

## Step 2: Determine the version

```bash
git tag --sort=-v:refname | head -3      # latest tags
gh release list --limit 5                # latest GitHub releases
git log "$(git describe --tags --abbrev=0)..HEAD" --oneline   # commits since last tag
```

Propose a semver bump from the commits and confirm with the user (or use the
version the user already named):

- **patch** (0.x.**Y**) — bug fixes, doc updates, dependency bumps, internal
  refactors, diagnostics.
- **minor** (0.**X**.0) — new subcommands or flags, new capabilities, changed
  default behavior that is additive.
- **major** (**X**.0.0) — breaking changes (not applicable pre-1.0).

If there are no commits since the last tag, there is nothing to release — say so.

## Step 3: Docs sweep

Release changes ripple into docs. Sweep each before bumping the version, and
commit doc changes as their own focused commit(s) ahead of the version bump.

### README — `README.md`

- Quickstart commands, flags, and example output still accurate.
- The feature/command list reflects reality (new commands, new behavior).
- The "Repository Layout" and install sections are current.
- Any version strings.

### Site guide — `site/guide/*.md`

- `cli.md` — new/changed subcommands, flags, default behavior, output examples,
  new error conditions. This is the command reference; keep it exhaustive.
- `quickstart.md`, `concepts.md`, `design.md` (locked-decision list).
- Validate the build before committing:
  ```bash
  cd site && npm run build   # vitepress build; output is the gitignored dist/
  ```

### Bundled CLI skill — `skills/nit/SKILL.md`

This is the **shipped** agent skill: it is embedded into the `nit` binary via
`include_str!("../skills/nit/SKILL.md")` in `src/skills.rs` and is what
`nit skills install` puts into agent harnesses. Any user-visible CLI change must
be reflected here, or every agent using the installed skill gives stale guidance.

(This `nit-release` skill is **not** shipped — `nit skills install` is hardcoded
to `skills/nit/` only, so the maintainer skills under `skills/` never reach end
users.)

### Decisions and examples

- `docs/decisions/` — if a decision changed user-visible behavior, make sure the
  README and guide reflect it.
- Keep example identifiers and output snippets consistent with current formats
  (e.g. Change/Pin ids).

Grep for stale version strings:

```bash
grep -rn "0\.[0-9][0-9]*\.[0-9]" README.md site/ skills/ --include='*.md'
```

## Step 4: Bump the version

Edit the version in both files — in `Cargo.lock`, change only the `name = "nit"`
package's `version`:

- `Cargo.toml` → `version = "X.Y.Z"` under `[package]`.
- `Cargo.lock` → the `version` line directly under `name = "nit"`.

Verify the bump is internally consistent and reported correctly:

```bash
cargo build --locked            # fails if Cargo.lock drifted from Cargo.toml
./target/debug/nit --version    # should print: nit X.Y.Z
```

## Step 5: Commit and push master first

Stage explicitly — do not use `git add -A`:

```bash
git add Cargo.toml Cargo.lock
git commit -m "Bump version to X.Y.Z"
git push origin master
```

Match the existing history: the version bump is its own commit (message exactly
`Bump version to X.Y.Z`), and the release tag points at it. Push `master` before
tagging so the release build checks out a commit that exists on the remote
default branch.

Do **not** add a signature or `Co-Authored-By` trailer to the commit (repo
convention).

If the push is rejected because the remote moved, rebase and re-push before any
tag work:

```bash
git pull --rebase origin master
git push origin master
```

## Step 6: Tag and push the release

Nit tags are **lightweight** and sit on the bump commit (match `v0.8.0`,
`v0.8.1`):

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

Pushing the tag triggers `release.yml`. The build job compiles three targets —
`linux-x86_64`, `darwin-x86_64`, `darwin-aarch64` — packages each as
`nit-X.Y.Z-<os>-<arch>.tar.gz`, and the release job creates the GitHub release
with those tarballs plus `checksums.txt`. GitHub marks the newest release Latest
automatically.

## Step 7: Verify

```bash
gh run list --limit 6
gh run watch <release-run-id> --exit-status   # watch the Release run to green
```

Then confirm the published release:

```bash
gh release view vX.Y.Z --json tagName,isDraft,isPrerelease,assets \
  --jq '{tag: .tagName, draft: .isDraft, prerelease: .isPrerelease, assets: [.assets[].name]}'
gh release list --limit 3   # vX.Y.Z should be Latest
```

Expect: not draft, not prerelease, marked Latest, and four assets —
`checksums.txt`, `nit-X.Y.Z-darwin-aarch64.tar.gz`,
`nit-X.Y.Z-darwin-x86_64.tar.gz`, `nit-X.Y.Z-linux-x86_64.tar.gz`.

Also confirm CI went green on both the master push and the tag, and watch the
`Deploy Site` run to success if the release touched `site/`:

```bash
gh run list --limit 8
gh run watch <deploy-site-run-id> --exit-status
```

Report the result to the user with links:

- Release: `https://github.com/mostlydev/nit/releases/tag/vX.Y.Z`
- Site: `https://mostlydev.github.io/nit/`

### Optional: enrich the release notes

The workflow publishes bare notes (`Nit vX.Y.Z`). For a notable release, replace
them with synthesized highlights grouped by theme — not a raw commit list:

```bash
gh release edit vX.Y.Z --notes "$(cat <<'EOF'
## Highlights

- **Feature** — what changed and why it matters.

## Fixes

- Fix: brief description.
EOF
)"
```

## Edge cases

- **No commits since the last tag** — nothing to release; tell the user.
- **Dirty working tree** — warn before starting; don't mix unrelated changes
  into the bump commit.
- **Local clippy passes, CI clippy fails** — CI uses current stable with
  `-D warnings`. `rustup update stable && cargo clippy --all-targets -- -D warnings`
  before tagging.
- **`cargo build --locked` fails after the bump** — the `Cargo.lock` nit entry
  was not bumped; update it to match `Cargo.toml`.
- **Push rejected (remote ahead)** — `git pull --rebase origin master`, then
  re-push master before tagging.
- **Tag points at the wrong commit (e.g. after a rebase)** — delete and re-push
  the tag; if the workflow already created a partial release, delete it first:
  ```bash
  gh release delete vX.Y.Z --yes
  git push origin :refs/tags/vX.Y.Z
  git tag -d vX.Y.Z && git tag vX.Y.Z && git push origin vX.Y.Z
  ```
- **Release came out as draft/not-latest** — the workflow publishes a normal
  public release and GitHub auto-marks the newest as Latest. If you ever create
  one by hand, pass `--latest`.
