# Nit: Change Groups, Pins, Checkout, Governance

## Status

Converged v1/v2 design after adversarial review with Claude and Codex.

The product name is **Nit**: a small Git-native layer for knitting
multiple independent repositories into one understandable workspace. The old
`legit_design.md` governance plan is not discarded, but it is no longer v1.
Governance becomes v2 on top of the v1 primitives.

The v1 goal is:

> Make multi-repo Git work feel like one coherent workspace while preserving
> ordinary independent Git repositories underneath.

The operator answered the remaining design questions:

- Pinning is core, not a minority add-on.
- The command name is `nit`.
- This file should hold the v1 plan plus the v2 governance path.

---

## The Reframe

The original design focused on agent governance: Aims, Attempts, Eval Results,
and Collapse Manifests. The simpler pressure underneath is more general:

> A real change often spans several colocated repositories. Today the user or
> agent has to commit child repos, update a parent pointer or manifest, commit
> the parent, push in the right order, and then mentally remember that those
> commits belonged together.

Nit v1 addresses that directly.

It does **not** create a new VCS. Each member remains a normal Git repository.
Nit adds shared workspace metadata, commit grouping, reproducible pins, safe
cross-repo checkout, and cross-repo views.

---

## Core Model

V1 has two co-primary primitives:

1. **Change**: what commits belong together.
2. **Pin**: what exact repository state is reproducible.

They solve different problems and neither replaces the other.

| Primitive | Question answered | Source of truth | Typical command |
|---|---|---|---|
| Change | Which commits are one logical change? | Commit trailers in member repos | `nit commit`, `nit change show` |
| Pin | Which exact commits should this workspace materialize? | Committed pin artifact in the root/control repo | `nit pin`, `nit checkout` |

A Change is about causality and review. A Pin is about reproducible checkout,
release, CI, deployment, and handoff.

Earlier drafts tried to make pins derived caches. That breaks once pinning is
core. A pin can be derived from one Change, but it can also represent:

- a baseline or release with no new Change
- the union of several Changes
- a hotfix selection from one repo plus older known-good commits in others
- a rollback to a previous coherent state
- an ad-hoc current workspace snapshot

So the settled rule is:

> Changes are source of truth for grouping. Pins are source of truth for
> materialization.

Crucially, neither is source of truth for the **code**. Git still owns the bytes.
A Change is a set of trailers on real Git commits; a Pin is a resolved
`{node -> commit}` *selection* plus provenance. Nit records what belongs together
and what to reproduce; it never stores code outside Git. This preserves the core
invariant: every member stays a normal Git repository.

---

## Workspace Roster

A workspace has a shared roster of member repositories.

Default behavior:

- The roster is a committed, shared artifact.
- It lives in a designated root member repo when one exists.
- If no natural root exists, `nit init` can create a tiny workspace-control repo.
- Local-only rosters are allowed only as explicit ad-hoc mode.

This follows from the shared-log requirement. A cross-repo graph is retrievable
only if everyone knows which repositories to scan.

Root designation (the mechanism, distinct from the create-vs-local-only policy
in Decisions (Locked)):

- `nit init` runs in a directory. If that directory **is itself a Git repo**, it
  becomes the root member and hosts `.nit/`.
- If it **contains** repos but is not one (e.g. sibling repos `app/` and
  `service/` under a plain `workspace/` dir), there is **no natural root**: `nit
  init` either creates a tiny workspace-control repo at that directory or drops to
  explicit local-only mode.
- The root is a normal member too: it can carry its own code commits and Changes.
  Its double duty (control metadata + member code) is handled by keeping pin/roster
  commits **separate from code commits** (see Pin Creation).

Roster entries contain:

```yaml
nodes:
  - id: app
    path: .
    remotes:
      - git@github.com:example/app.git
  - id: sdk
    path: vendor/sdk
    remotes:
      - git@github.com:example/sdk.git
    local_excludes:
      - ancestor: app
        path: vendor/sdk
```

The roster changes rarely: `nit adopt`, `nit ignore`, path moves, and optional
dependency hints. It does **not** change for every logical code change.

This is the important split:

- membership changes are roster commits
- code changes are ordinary Git commits with Change trailers
- reproducible snapshots are pin commits

---

## Effort Check Against Submodules

Nit only wins if the common publish path is shorter and less stateful than
ordinary submodules. The comparison is:

With submodules, a three-repo publish usually means:

```text
git -C sdk add ... && git -C sdk commit ...
git -C docs add ... && git -C docs commit ...
git add app-files...
git commit ...
git add vendor/sdk docs          # gitlink pointer updates
git commit -m "Update submodules"
git -C sdk push
git -C docs push
git push                         # parent last
```

The exact commands vary, but the work is always the same: commit children,
remember to update the parent pointers, commit the parent, and push in an order
that does not publish dangling references.

Nit's regular publish path must be:

```text
nit add -A                                   # or explicit paths
nit land -m "Publish webhook retry update"   # name is OPTIONAL
nit push
```

`nit land` is the important ergonomic command. It plans and performs the whole
publish transaction:

1. commit staged changes in each touched member repo with one `Nit-Change-Id`
2. create a Pin for the resulting three-repo state
3. commit the Pin in the root/control repo
4. leave `nit push` to publish member commits first and the Pin last

The Pin **name is optional**: `nit land -m "..."` auto-generates a pin id;
`nit land <name> -m "..."` names it for a release. This matters: submodules make
you name nothing on a publish, so forcing a pin name on every `land` would *add*
ceremony Git does not have. The basic publish must be exactly three verbs:
`add`, `land`, `push`.

When to use which:

- **`nit land`** - "publish this workspace state" for a dependency-coupled
  workspace (the submodule-replacement case). The Pin is the reproducible record,
  exactly what a submodule superproject commit gives you, so a Pin per publish
  here is *not* churn, it is the point.
- **`nit commit`** (no pin) - WIP, incremental work, or loosely-coupled repos
  where you do not need a recorded cross-repo state. This is why auto-pin stays
  off: `land` is the explicit "I want a record" verb, `commit` is the everyday
  one. `nit commit` + `nit pin` remain available as the decomposed form.

If a user regularly has to run child commit, root metadata commit, and ordered
push by hand, Nit has failed its purpose.

### Minimal three-repo publish example

Workspace:

```text
product/
  .nit/          # root/control metadata
  app/           # member repo
  sdk/           # member repo
  docs/          # member repo
```

One-time setup from three existing remotes:

```console
$ mkdir product && cd product
$ nit init --control --remote git@github.com:example/product-workspace.git
created control repo at product/  (.nit/ created, origin set)

$ git clone git@github.com:example/app.git app
$ git clone git@github.com:example/sdk.git sdk
$ git clone git@github.com:example/docs.git docs
$ nit adopt app sdk docs
adopted app, sdk, docs  (3 roster entries)
committed roster update to control repo  (1 commit)

$ nit pin baseline
pinned baseline -> PIN-20260603-p0
  app   1111111
  sdk   2222222
  docs  3333333
committed pin to control repo  (b45e1b0)

$ nit push
preflight ok (3 members, 1 pin)
  app      already up to date
  sdk      already up to date
  docs     already up to date
  control  pushed roster commits + baseline pin
done
```

After editing all three repos:

```console
$ nit status
Workspace product
Current pin: baseline (working tree differs: app, sdk, docs)

Unstaged
  app  M src/webhooks.ts
  sdk  M src/retry.ts
  docs M retry.md

$ nit add app/src/webhooks.ts sdk/src/retry.ts docs/retry.md
staged: app(1), sdk(1), docs(1)

$ nit land -m "Publish webhook retry update"
plan:
  Change NCH-20260603-k9d2
    app   commit staged changes
    sdk   commit staged changes
    docs  commit staged changes
  Pin PIN-20260603-p4  (unnamed; add `nit land <name>` to label a release)
    app   <new app commit>
    sdk   <new sdk commit>
    docs  <new docs commit>
proceed? yes

created Change NCH-20260603-k9d2
  app   a1b2c3d  Publish webhook retry update
  sdk   d4e5f6a  Publish webhook retry update
  docs  f7a8b9c  Publish webhook retry update
created Pin PIN-20260603-p4
committed pin to control repo  (c0ffee1)

$ nit push
preflight ok (3 members, 1 pin)
  app      pushed a1b2c3d
  sdk      pushed d4e5f6a
  docs     pushed f7a8b9c
  control  pushed PIN-20260603-p4
done
```

This is still the same logical ordering Git requires, but the user-visible work
is reduced to staging, landing, and pushing. Nit carries the pointer/pin update,
metadata commit, and push-order bookkeeping.

### Being honest: where submodules are comparable or ahead

Nit is not a free win everywhere, and pretending otherwise would be a sales pitch.

- **Against a submodule expert using `git submodule foreach`**, the command-count
  win is modest (3 vs ~4). The durable advantage is footgun removal: no push
  ordering to remember, no detached-HEAD commit loss, no manual gitlink bump; not
  raw keystrokes.
- **Reconstruction must stay one command.** `git clone --recursive` hydrates a
  whole submodule tree in one step. So `nit clone <url>` **auto-hydrates** members
  from the roster by default (and `--pin <p>` also materializes them). If
  reconstruction took three separate steps instead, submodules would be *less*
  effort: the one-command clone is a hard requirement, not a nicety.
- **For truly unrelated repos** that just share a directory, both Nit and a
  submodule superproject are pure overhead. Use either only when the repos are
  actually related; otherwise plain per-repo Git is correct.

The verdict: for a *related* multi-repo workspace, Nit's basic publish is fewer
steps and removes the submodule footguns; the heavy machinery (checkout,
worktrees, review artifacts) is opt-in and never taxes the basic
`add`/`land`/`push` path.

---

## Workspace Construction And Examples

The model is only useful if the first-hour workflows are boring. These examples
are part of the design surface: they show where Nit must avoid surprising Git
users.

### 1. Existing root repo with an existing nested repo

Starting state:

```text
app/              # normal Git repo
  .git/
  vendor/sdk/     # normal nested Git repo, not a submodule
    .git/
```

Walkthrough (real session; note that **Nit commits its own metadata**; the user
never hand-stages `.nit/`):

```console
$ cd app
$ nit init
initialized Nit workspace  (root: app, created .nit/)
discovered 1 nested repo not yet adopted:
  vendor/sdk   -> run `nit adopt vendor/sdk` to manage it

$ nit adopt vendor/sdk --id sdk
adopted sdk  (vendor/sdk)
  + roster entry: sdk
  + local exclude in app: vendor/sdk
committed roster update to app  (a1b2c3d)

$ nit status
Workspace app   root: app
Current pin: none
  app    clean   on main
  sdk    clean   on main
nothing staged; workspace has no pin yet

$ nit pin baseline
pinned baseline -> PIN-20260603-p0
  app  def456
  sdk  abc123
committed pin to app  (e4f5a6b)

$ nit push
preflight ok (2 repos, 1 pin)
  sdk   already up to date
  app   pushed 2 commits  (roster + pin)
done
```

Required behavior:

- `nit adopt` records `sdk` in the shared roster, writes the local excludes so
  `app` does not see `vendor/sdk` as untracked, and **commits the roster update
  itself** (use `--no-commit` to batch several adoptions into one roster commit).
- Roster and pin commits are plain metadata commits in the root/control repo;
  they do not carry a `Nit-Change-Id` and are never folded into a code Change.
- `nit doctor` can reapply the excludes on another machine from the shared roster.
- `nit pin baseline` creates the first reproducible snapshot and commits it.

The user should not have to stage a child gitlink, edit a manifest, or hand-run
`git add .nit/...`. If a step in these walkthroughs would make the user reach for
raw `git` to manage Nit metadata, that is a faceplant and the command owns it
instead.

### 2. Existing sibling repos with no natural root

Starting state:

```text
workspace/
  app/.git/
  sdk/.git/
  docs/.git/
```

There is no root member repo because `workspace/` itself is not a Git repo.

Workflow with a shared control repo:

```console
$ cd workspace
$ nit init --control --remote git@github.com:example/product-workspace.git
created control repo at workspace/  (.nit/ created, origin set)
discovered 3 nested repos not yet adopted: app, sdk, docs

$ nit adopt app sdk docs
adopted app, sdk, docs  (3 roster entries)
committed roster update to control repo  (1 commit)

$ nit pin baseline
pinned baseline -> PIN-20260603-p0  (app ..., sdk ..., docs ...)

$ nit push
  app/sdk/docs  already up to date
  control       pushed 2 commits  (roster + pin)
done
```

Required behavior:

- `nit init --control [--remote <url>]` creates a tiny Git repo at `workspace/`
  to hold `.nit/`, and records the remote if given (no raw `git remote add`).
- `nit adopt` accepts multiple paths and commits one roster update for the batch.
- The control repo is only workspace metadata unless the user puts files there.
- Pin and roster commits live in the control repo.
- Member repos keep their own remotes and histories.

Local-only mode is explicit:

```text
nit init --local
```

Local-only mode is useful for personal scratch assemblies, but it cannot provide
the shared retrievable graph by itself.

### 3. Fresh machine from a shared workspace

Starting point: a teammate has only the root/control repo URL.

Workflow - one command, matching `git clone --recursive`:

```console
$ nit clone git@github.com:example/product-workspace.git product --pin baseline
cloned control repo  (3 members in roster)
hydrating members...
  app   cloned
  sdk   cloned
  docs  cloned
applied local excludes
checked out pin 'baseline'  (app 1111111, sdk 2222222, docs 3333333)
ready: cd product
```

Required behavior:

- **`nit clone` auto-hydrates by default**: it clones the root/control repo,
  reads `.nit/roster.yaml`, clones any missing members to their roster paths,
  configures remotes, and applies local excludes, all in one command. This is a
  hard requirement: `git clone --recursive` is one step, so reconstruction must
  be too.
- `--pin <p>` additionally materializes members to that pin's commits with the
  same safe rules as `nit checkout` (refuse unsafe resets or dirty worktrees).
- Without `--pin`, members land on their default branches. There is no separate
  `hydrate` verb: re-running `nit checkout <pin>` clones any member added later,
  and `nit doctor` repairs a partial workspace.
- If a member remote is missing or a pinned commit is unreachable, Nit reports the
  exact node and remote instead of silently producing a partial workspace.

This closes the construction gap: a Pin is only useful if a fresh machine can
materialize it from the shared roster in a single command.

### 4. Add a new repo to an existing workspace

There is **no `nit new` verb**. Make the repo with the Git verb you already know,
then adopt it. This keeps the surface git-native and avoids a `nit new` /
`adopt --init` mode flag:

```console
$ git init plugins/payments          # or: git clone <url> plugins/payments
$ nit adopt plugins/payments --id payments
adopted payments  (plugins/payments)
  + local exclude in app: plugins/payments
committed roster update to control repo  (1 commit)
```

Required behavior:

- `nit adopt` works on any repo already on disk, however it was created (`git
  init`, `git clone`, copied in). It adds the roster entry, applies ancestor
  excludes, and commits the roster update itself.
- The roster commit is separate from any code commit in the new member.
- `nit status` shows the new member as present but unpinned until a Pin records
  its commit.

### 5. Add a member from a remote that is not cloned yet

Same rule: clone with Git, then adopt. No `adopt --clone` mode flag.

```console
$ git clone git@github.com:example/sdk.git vendor/sdk
$ nit adopt vendor/sdk --id sdk
adopted sdk  (vendor/sdk)
committed roster update to control repo  (1 commit)
```

Required behavior:

- `git clone` puts the repo on disk with its remote already configured; `nit
  adopt` records the path (and reads the remote from the clone) into the roster
  and applies ancestor excludes.
- It refuses if `vendor/sdk` is already tracked as normal files in the root.

### 6. Convert an existing submodule

Starting state:

```text
app/
  .gitmodules
  vendor/sdk     # gitlink/submodule
```

Workflow:

```text
nit import-submodule vendor/sdk --id sdk
nit status
nit pin baseline
```

Required behavior:

- Nit detects the existing gitlink and `.gitmodules` entry.
- It presents a transaction plan before changing anything.
- The plan removes the submodule metadata from the root, makes `vendor/sdk` a
  normal member repo, adds it to the roster, and applies local excludes.
- It refuses if the submodule worktree is dirty or detached in a way that would
  strand commits.

This command is needed because "just refuse tracked gitlinks" is correct for
safety but not enough for common migrations.

### 7. Normal cross-repo publish

Walkthrough (run from anywhere inside the workspace; `nit` finds the root by
walking up to `.nit/`, just like `git` finds `.git`):

```console
$ # edited files in app and sdk
$ nit add sdk/src/retry.ts app/src/webhooks.ts app/tests/webhooks.ts
staged: sdk(1), app(2)

$ nit status
Workspace app   root: app
Current pin: baseline (working tree differs: app, sdk)
Staged for next Change
  app  M src/webhooks.ts
       M tests/webhooks.ts
  sdk  M src/retry.ts

$ nit land -m "Add webhook retry support"
plan:
  Change NCH-20260603-abc123
    sdk  commit staged changes
    app  commit staged changes
  Pin PIN-20260603-p1  (unnamed)
    sdk  <new sdk commit>
    app  <new app commit>
proceed? yes

created Change NCH-20260603-abc123
  sdk  abc123  Add webhook retry support
  app  def456  Add webhook retry support
created Pin PIN-20260603-p1
committed pin to control repo  (c0ffee1)

$ nit push
preflight ok
  sdk   pushed 1 commit
  app   pushed 1 commit
  app   pushed pin commit (PIN-20260603-p1)   # pin lands after members
done
```

Important default:

- `nit land -m "message"` is the canonical publish path: one planned operation
  for member commits plus the reproducible Pin. `nit land <name> -m "message"`
  is the named form for releases or human handles.
- `nit commit` alone does not automatically create a Pin. It reports pin drift
  so the user knows the workspace has unpinned changes.
- `nit pin` is still a deliberate reproducibility act. The decomposed form is:
  `nit commit -m "message"` followed by `nit pin <name>`.
- The decomposed form is for WIP, review staging, or releases that intentionally
  collect several Changes into one Pin. It is not the normal publish UX.

### 8. Release pin from several unrelated changes

Workflow:

```text
nit log
nit pin release-2026-06-03
nit diff previous-release release-2026-06-03
nit review release-2026-06-03
nit push
```

Required behavior:

- A Pin may reference zero, one, or many Change-Ids as provenance.
- `nit diff <pinA> <pinB>` shows what changed between two reproducible states
  even when no single Change produced the new state.
- `nit review <pin>` produces the review artifact for the release.

### 9. Hotfix worktree from a production Pin

Workflow:

```console
$ nit worktree add ../hotfix --pin production
materialized workspace at ../hotfix from pin 'production'
  app  detached @ def456  (worktree)
  sdk  detached @ abc123  (worktree)
  root control worktree on branch nit/wt/hotfix

$ cd ../hotfix
$ # edit only app
$ nit add app/src/patch.ts
staged: app(1)
$ nit commit -m "Patch production webhook timeout"
created Change NCH-20260603-hf01
  app  fed210  Patch production webhook timeout
$ nit pin production-hotfix
pinned production-hotfix -> PIN-20260603-p9
$ nit push
  app   pushed 1 commit
  control pushed pin commit
done
```

Required behavior:

- `nit worktree add` creates a separate materialized workspace rooted at
  `../hotfix`, isolated from the original's working directories.
- It uses Git worktrees for member repos when possible and falls back to clones
  when necessary.
- Worktree branch collisions are handled, not hit: a member is materialized
  **detached at the pinned commit** (or on a fresh `nit/wt/<name>` branch), so it
  never trips Git's "branch already checked out in another worktree" error when
  the same branch is live in the original workspace.
- The root/control repo is also a worktree, on its own `nit/wt/<name>` branch, so
  pins/changes created here are ordinary commits that push independently.
- It materializes the requested Pin with the same safe rules as `nit checkout`.

This is the agent/human isolation workflow: make a new workspace from a Pin,
work there, create a Change, then create a new Pin, with no branch explosion in
the shared repos.

### 10. Agent attempt without branch explosion

Workflow:

```text
nit worktree add ../attempt-a --pin baseline
cd ../attempt-a
# harness creates per-touched-repo branches lazily
nit commit -m "Attempt retry implementation"
nit pin attempt-a-ready
nit review attempt-a-ready
```

Required behavior:

- Nit does not create same-named branches in every repo.
- The harness may create branches only in repos it edits, for example
  `nit/NCH-20260603-abc123/app`.
- The Pin captures the exact candidate state for review or v2 governance.

### 11. Recovery: partial push and a dangling pin

Partial push: `sdk` lands but `app` is rejected mid-operation:

```console
$ nit push
preflight ok  (2 repos, 1 pin)
  sdk   pushed 1 commit
  app   ! rejected (non-fast-forward): remote has newer commits
  pin   HELD BACK (members incomplete)
partial landing: sdk pushed; app failed; pin not published.
resolve app, then run: nit push --resume

$ # integrate origin/main into app, then:
$ nit push --resume
  app   pushed 1 commit
  app   pushed pin commit
done
```

Nit pushed members first, and when `app` failed it **held the pin back** instead
of publishing a pin that references an unpushed commit. `--resume` re-preflights
and continues from the unfinished set; it never re-pushes what already landed.

Dangling pin: a member rebased away a pinned commit:

```console
$ nit checkout release-2026-05
  app   ok @ def456
  sdk   ! pinned commit abc123 is unreachable (history rewritten?)
checkout stopped: pin 'release-2026-05' is dangling in sdk. run `nit doctor`.

$ nit doctor
checking pins against member histories...
  pin release-2026-05: sdk@abc123 unreachable on any known remote
  -> likely rebased or garbage-collected
this pin cannot be materialized as-is. options:
  - re-pin from the surviving SHA:   nit pin --change <id> release-2026-05b
  - recover abc123 from a reflog/backup remote and re-run nit checkout
  - record the pin as superseded (keeps history honest; not reproducible)
```

Recovery never invents evidence. `nit doctor` explains the break and offers
explicit actions; it does not silently repoint a pin to a different commit.

### UX checks these examples force

- There must be a one-command fresh-machine path: `nit clone --pin` auto-hydrates
  members and materializes the pin, matching `git clone --recursive`; never three
  separate steps.
- New members use plain `git init`/`git clone` then `nit adopt` - no `nit new`
  verb and no `adopt --init`/`--clone` mode flags (keeps the surface git-native
  without confusing arguments).
- There must be a submodule migration path: `nit import-submodule` (its own verb,
  not a flag on adopt, because it is migration surgery, not adoption).
- There must be an isolated workspace path: `nit worktree add`.
- There must be an explicit staging helper: `nit add`.
- `nit commit` must remain staged-only and must not auto-pin by default.
- Pins must push after member commits.
- `nit status` must always show dirty repos, missing repos, pin drift, unpushed
  participants, ambiguous Change projections, and discovered-but-unadopted repos.
- **Nit owns its metadata commits.** `adopt`, `import-submodule`, `pin`, and
  label moves commit their own `.nit/` changes (with `--no-commit` to batch).
  If a walkthrough makes the user hand-run `git add .nit/...`, that is a faceplant.
- Every command resolves the workspace from the current directory upward, so the
  user is never forced to `cd` back to the root.
- `nit worktree add` must dodge Git's "branch already checked out" error by
  materializing members detached or on a fresh `nit/wt/<name>` branch.

---

## Staging And Status UX

Staging must be ergonomic without making `nit commit` dangerous.

`nit add` is an explicit path-based fan-out wrapper:

```text
nit add app/src/webhooks.ts sdk/src/retry.ts
nit add --repo sdk src/retry.ts tests/retry.ts
nit add -A                # explicit: stage all tracked-modified files, every member
nit add .                 # explicit: stage everything under the current directory
```

Rules:

- `nit add` maps workspace-relative paths to the member repo that owns them.
- `nit add --repo <id>` interprets paths relative to that member.
- `nit add -A` / `nit add .` are the **explicit** bulk-stage forms for the common
  daily flow (mirrors `git add -A` / `git add .`). They are an explicit user act,
  so they are not the footgun; the footgun is an *implicit* transitive stage.
- It refuses paths inside unadopted nested repos.
- It refuses ambiguous paths caused by duplicate mounts or unresolved roster
  entries.

`nit commit` remains staged-only and **never implicitly stages**: there is no
transitive `nit commit -a` that auto-stages across the workspace. Bulk staging is
always an explicit `nit add -A`/`.` first. `nit add` improves daily ergonomics
without turning commit into a surprise workspace-wide `git commit -a`.

`nit status` must be grouped for scanning. A large workspace cannot be a flat
wall of Git status output. The shape should be:

```text
Workspace product
Current pin: baseline (drifted: app, sdk)

Staged for next Change
  app  M src/webhooks.ts
       M tests/webhooks.ts
  sdk  M src/retry.ts

Unstaged
  docs M README.md

Missing or not checked out
  worker missing locally; run `nit checkout <pin>` to clone + materialize it

Discovered (not adopted)
  tools/scratch   run `nit adopt tools/scratch` or `nit ignore tools/scratch`

Pins
  baseline pushed
  webhook-retry-ready local only
```

Required status properties:

- group by repo, not by raw path
- show the current Pin and drift first
- separate staged, unstaged, untracked, missing, and pushed state
- surface discovered-but-unadopted nested repos with an adopt/ignore hint
- show suggested next commands only for actionable problems
- keep clean repos collapsed by default, with `--verbose` to expand
- resolve the workspace from the current directory upward (like `git`), so every
  command works from any member subdirectory, not just the root

---

## Node Identity

A node id is a workspace slot id, not a global repo identity.

- Path is location, not identity.
- Remote URL is evidence, not identity.
- Two clones of the same remote may be two separate nodes.
- The same physical repo may appear through symlinks or bind mounts; discovery
  deduplicates it by canonical git directory and filesystem identity.
- Re-cloning the same shared roster preserves node ids because the ids live in
  the roster.

Before adoption, a discovered repo has only a temporary fingerprint:

- canonical worktree path
- git common-dir
- filesystem device/inode where available
- remote aliases

Nit refuses graph cycles. If A nests B and B bind-mounts back to A, traversal
stops with an error instead of looping.

---

## Change

A Change is a group of ordinary Git commits across member repos.

Implementation trailer:

```text
Nit-Change-Id: NCH-20260603-abc123
```

This document says "Change-Id" generically, but the implementation should use a
Nit-specific trailer to avoid colliding with Gerrit-style `Change-Id`.

Optional v1 trailers:

```text
Nit-Change-Goal: Add retry handling for webhook delivery.
Nit-Actor: codex
Nit-Session: harness:...
```

Structured scope, eval evidence, and governance policy are v2.

### Change Creation

`nit commit -m "message"`:

- scans the shared roster
- finds member repos with staged changes
- refuses untracked child repos that should be adopted or ignored
- commits staged changes in each touched member repo
- stamps each commit with the same `Nit-Change-Id`
- never stages automatically across repos
- never applies `git commit -a` transitively
- reports pin drift after the commit

`nit commit --change <id> -m "message"` appends new commits to an existing
Change. This is how an agent or human accretes one logical change over several
commit rounds.

### Change Projection

A repo may contain multiple commits with the same Change-Id, especially after
incremental work, rebases, cherry-picks, or divergent branches. Projection must
be deterministic.

Within each member repo and selected ref scope:

- zero maximal reachable commits with the Change-Id means the repo is absent
- one maximal reachable commit means that commit is the participant tip
- more than one maximal reachable commit means the Change is ambiguous in that
  repo

`nit change show <id>` must report ambiguity. It must not silently choose by
timestamp. A caller can narrow the ref scope or create a Pin after selecting
tips.

### Change Views

Required v1 views:

```text
nit change status <id>
nit change show <id>
nit change diff <id>
nit change log [<id>]
nit change revert <id>
```

`nit change revert <id>` is non-destructive. It runs per-repo `git revert` for
resolved participant tips and creates a new Change with:

```text
Nit-Reverts-Change-Id: <id>
```

It never cross-repo resets by default.

---

## Pin

A Pin is an identified, committed, reproducible cross-repo snapshot. It may also
have a human-friendly name or label.

It records exact member commits:

```yaml
id: PIN-20260603-abc123
name: webhook-retry-ready
created_at: 2026-06-03T05:00:00Z
nodes:
  app:
    commit: def456...
    branch_hint: feature/webhook-retry
  sdk:
    commit: abc123...
    branch_hint: main
provenance:
  changes:
    - NCH-20260603-abc123
```

Pins live in the root/control repo under a Nit metadata directory, for example:

```text
.nit/
  roster.yaml
  pins/
    PIN-20260603-abc123.yaml
```

The pin commit is the workspace-level reproducible state. This is intentional.
When pinning is core, the parent/control commit is not a footgun; it is the
record the user asked for. The footgun was doing it manually and pretending a
child pointer update was ordinary Git ergonomics.

### Pin Creation

`nit pin <name>`:

- resolves the current workspace state
- writes a pin artifact in the root/control repo
- commits that artifact
- records any selected Change-Ids as provenance

`nit pin --change <id> <name>`:

- resolves a Change using the projection rule
- refuses ambiguous participant tips unless the caller selects them
- writes and commits the pin

`nit commit --pin [<name>] -m "message"` or `nit land [<name>] -m "message"`:

- commits staged member repo changes with one Change-Id
- creates a Pin for the resulting workspace state
- commits the pin artifact
- presents the whole transaction plan before mutating

This automates the old child-commit, parent-stage, parent-commit sequence. It
does not pretend the sequence is logically unnecessary.

Auto-pin is **off by default**, even though pinning is core. Pinning every
`nit commit` would reintroduce per-change control-repo churn and proliferate
meaningless pins; a Pin is a deliberate act (`nit pin` / `nit land`). A policy
(e.g. `nit.autopin = release-branches | agents`) can opt specific contexts into
pin-on-land. In the root/control repo, the **pin artifact commit is kept separate
from any code commit** so the root's history does not entangle reproducible-state
metadata with ordinary code.

### Pin Identity

Pins should be immutable by id. A human-friendly name may point to a pin id, but
moving a name requires an explicit update. This keeps `nit checkout PIN-...`
reproducible while allowing labels such as `staging` or `latest-good` if a team
wants them.

Recommended label semantics (now locked): pin **ids** are
immutable; **labels** are movable pointers, but every move is **recorded as a
commit** in the root/control repo, so the history of what `staging` meant is
auditable. `nit checkout <id>` is reproducible; `nit checkout <label>` resolves a mutable
pointer and **warns** that it is doing so. This gives both reproducible release
ids and convenient moving labels without conflating them.

---

## Checkout

`nit checkout <pin>` materializes a Pin. It reuses Git's `checkout` mental model
(make my working state match this), extended across repos. (`nit sync` is kept
only as an alias for users who find it clearer; `checkout` is canonical.)

Safe default behavior:

- **clone any missing roster members** from their recorded remotes (this is why
  there is no separate `hydrate` verb: `nit clone` auto-hydrates, and `checkout`
  clones-missing as part of materializing)
- fetch required member refs when configured to do so
- verify every pinned commit is reachable or explain what is missing
- if the pinned commit is the tip of the branch hint, a local branch, or a
  safely materializable remote branch, check out or fast-forward that branch
- if no branch can represent the pinned commit safely, detach HEAD and warn
  clearly instead of hiding the state
- never discard local changes by default

Exact mode:

```text
nit checkout <pin> --exact
```

`--exact` may detach HEAD, reset uncommitted work, and clean untracked files, but
it must not secretly rewind an existing branch ref to an older pin. Detached HEAD
is an explicit materialization mode, not a surprise.

Plain `git checkout` of the root/control repo can change the visible pin files,
but it cannot materialize child repos. Nit owns cross-repo materialization. That
is the honest submodule-class cost of core pinning.

---

## Workspace Log (the shared graph)

The operator's "retrievable shared history/log graph across repositories" is a
headline v1 capability, not just an internal projection. It is realized by one
top-level command:

```text
nit log
```

`nit log` renders a single **interleaved workspace timeline** reconstructed by
scanning member repos (per the roster) for `Nit-Change-Id` trailers and reading
the pin artifacts in the root/control repo. Each entry is either:

- a **Change** (its participant commits across members, grouped), or
- a **Pin** (a reproducible snapshot marker, with provenance).

Because it is a projection over Git + roster, the graph is retrievable on **any**
machine that has the member repos (or can fetch them); there is no separate
database to lose. `nit change log` and `nit pin list` remain the per-axis views;
`nit log` is the unified one. Where a member is missing locally, `nit log` says so
rather than silently dropping entries.

---

## Branching

Branches remain per repository. Nit does not invent a workspace branch namespace
in v1.

Rules:

- `nit commit` creates commits only in repos with staged changes.
- It does not create empty same-named branches in every member.
- A Pin records exact commits and optional branch hints.
- `nit checkout` uses branch hints only for safe materialization.
- A named Pin can act like a cross-repo release tag, not like a branch.

This avoids branch explosions while preserving reproducible workspace state.

Agents may use deterministic branch names per touched repo, for example:

```text
nit/<change-id>/<node-id>
```

That is a harness convention, not a v1 data-model requirement.

---

## Push And Partial Landing

Multi-repo push is not atomic.

`nit push` must make non-atomicity visible and recoverable:

- preflight all participating repos first
- verify remote access and fast-forward policy
- show the push plan
- push member commits before any Pin that references them
- push the root/control pin commit last
- report exactly what landed
- support `nit push --resume`

Default Change-only pushes may use stable roster order. Pin pushes have a hard
structural ordering requirement: member commits first, pin commit last. Optional
dependency hints in the roster can refine ordering, but v1 should not infer
semantic dependencies.

`nit status --pin <pin>` and `nit change status <id>` show whether each
participant is:

- local only
- pushed
- missing locally
- missing remotely
- ambiguous
- drifted from the selected Pin

---

## Review Surface

Host review is fragmented by default because the root/control repo only shows
roster and pin file diffs. V1 must provide a combined CLI review view.

Required:

```text
nit show <change>
nit show <pin>
nit diff <change>
nit diff <pin>
nit diff <pinA> <pinB>
nit review <change|pin>
```

`nit diff <pinA> <pinB>` is the cross-repo "what changed between two reproducible
states" view (e.g. release N vs release N+1, or a hotfix pin vs its baseline). It
is essential for reviewing **hand-assembled pins** (a selection no single Change
produced), which cannot be reviewed as one Change.

`nit review` produces a review artifact with per-repo diffs, selected commits,
pin contents, and provenance. Web/host integration can come later, but v1 must
make cross-repo review possible without manually opening every repo.

---

## Local Excludes And Adoption

Nested repos must not be accidentally staged into ancestor repos.

`nit adopt <path>`:

- adds the repo to the shared roster
- writes local `.git/info/exclude` entries in every managed ancestor that would
  otherwise see the child path as untracked
- records those required local excludes in the shared roster so `nit doctor`
  can reapply them on another machine
- refuses if the child path is already tracked in an ancestor

If the child is already tracked, Nit explains the case:

- tracked normal files
- existing submodule/gitlink
- existing Nit pin relationship

It never silently excludes tracked content.

---

## Raw Git Safety And Hooks

A natural worry: if Nit links repos together, won't a stray raw `git` command
desync the workspace? Should Nit inject Git hooks to prevent that?

The answer is that **this is mostly covered by the protocol design, not by
hooks.** Nit is reconstruct-not-enforce: Git is the source of truth, and Nit
state is a projection rebuilt from trailers + the roster + pin artifacts. So raw
Git is never catastrophic, only (at worst) untidy until the next `nit` command
reconciles it:

| Raw action | Effect | How Nit copes |
|---|---|---|
| `git commit` with no `Nit-Change-Id` | commit exists, ungrouped | `nit status`/`nit log` show it as ungrouped; `nit doctor` can stamp a trailer with explicit user action |
| `git checkout` / `reset` in a member | working tree drifts from the pin | `nit status` reports drift; `nit checkout` re-materializes |
| `git push` of a member | just pushes | fine; if a control pin commit is later pushed before a member, `nit doctor` reports a dangling pin |
| `git rebase` that drops a trailer | Change link lost | already covered; `nit doctor`/cache repair, no magic claim |

Because nothing here corrupts irrecoverable state, **hooks are a convenience, not
a safety requirement.** And auto-installing hooks is invasive (it fights
`core.hooksPath`, husky, pre-commit) and is bypassable, so per the original design
**local hooks are never the trust boundary.** The tiered policy:

- **Default: no hooks.** Rely on reconstruction plus `nit status`/`nit doctor` to
  surface and repair drift.
- **Opt-in convenience - `nit hooks install`:** a `prepare-commit-msg` that stamps
  `Nit-Change-Id` even on a raw `git commit` (so grouping survives), plus a
  `pre-push` warning about pin ordering. It must **chain** existing hooks, never
  clobber them.
- **Opt-in strict - `nit hooks install --strict` / `nit.strict`:** refuse raw
  desyncing operations. Sensible for managed autonomous agents that control their
  environment; off for humans.
- **Real enforcement is CI / a server-side gate (v2 governance)** - the only
  trustworthy boundary, exactly as in the original design.

So: protection comes from the projection model first, optional local hooks
second, and authoritative gates only at v2. Humans are never forced into a hook
regime to use Nit safely.

---

## CLI Surface

Commands use the product name:

```text
# setup / construction  (new members: use plain `git init`/`git clone`, then `nit adopt`)
nit init
nit init --control [--remote <workspace-url>]
nit init --local
nit clone <workspace-url> [path] [--pin <pin>]   # clones + auto-hydrates members; --pin also materializes
nit adopt <path>... [--id <id>] [--no-commit]    # adopt repo(s) already on disk
nit ignore <path>
nit import-submodule <path> [--id <id>]          # migrate a submodule to a member
nit hooks install [--strict]                     # opt-in; default is no hooks
nit doctor

nit status
nit status --pin <pin>

nit log

nit add <path>...
nit add -A | .                                   # explicit bulk stage across members
nit add --repo <id> <path>...
nit commit -m <msg>                               # everyday: group staged changes, no pin
nit commit --change <id> -m <msg>                 # append to an existing Change
nit land [<name>] -m <msg>                        # CANONICAL publish: commit + pin (name optional)
nit commit --pin [<name>] -m <msg>                # equivalent of land; scriptable form, not taught first

nit show <change|pin>
nit diff <change|pin>
nit diff <pinA> <pinB>

nit change status <id>
nit change show <id>
nit change diff <id>
nit change log [<id>]
nit change revert <id>

nit pin <name>
nit pin --change <id> <name>
nit pin show <pin>
nit pin diff <pin>
nit pin diff <pinA> <pinB>
nit pin list

nit checkout <pin>
nit checkout <pin> --exact

nit worktree add <path> --pin <pin>

nit push
nit push --resume

nit review <change|pin>
```

Recursion is a workspace default for Nit commands. `--local` or `--repo-only`
is the escape hatch.

Verb policy:

- read commands recurse across the roster
- local write commands operate only on staged changes and explicit pin files
- push commands preflight and show a plan
- destructive materialization requires `checkout --exact` plus confirmation/policy

---

## Failure Modes

| Failure mode | V1 behavior |
|---|---|
| Nested repo appears as untracked in parent | `nit adopt` writes local excludes; `nit doctor` reapplies them from roster. |
| Child path already tracked by parent | Refuse and explain; do not silently exclude. |
| Bind mount or symlink loop | Deduplicate by canonical repo identity; refuse cycles. |
| Fresh machine lacks member repos | `nit clone` auto-hydrates members from roster remotes; re-running `nit checkout <pin>` clones any member added later; `nit doctor` repairs a partial workspace and reports unreachable pins. |
| Existing submodule needs migration | `nit import-submodule` plans removal of gitlink metadata and adoption as a normal member; refuses dirty or stranded submodule states. |
| Partial commit | Report which repos committed; allow continue or non-destructive revert. |
| Partial push | Report exact landing state; resume later. |
| Pin references unpushed member commits | `nit push` pushes members first and pin commit last. |
| Pin commit pushed but member push failed | This should be prevented by ordering; if remote state contradicts, `nit doctor` reports a broken pin. |
| Pin references an unreachable member commit | `nit checkout` stops; `nit doctor` reports a dangling pin and offers explicit recovery or supersede actions without repointing silently. |
| Plain Git commit without Nit trailer | It remains normal Git; `nit status` shows ungrouped commits. `nit doctor` may attach/recover only with explicit user action. |
| Rebase preserves trailers | Change projection still works. |
| Rebase drops trailers | Change link is lost unless cache/doctor can repair; no magic claim. |
| Change has divergent tips | Views report ambiguity; caller narrows ref scope or pins selected commits. |
| Plain Git checkout of root changes pin files | Child repos do not move until `nit checkout`; status reports drift. |
| Dirty member during checkout | Refuse unless exact/destructive policy permits. |

---

## Implementation Plan

### Phase 1: Roster And Discovery

- Implement `nit init`, `nit clone` (clones + auto-hydrates members), `nit adopt`,
  `nit ignore`, and `nit import-submodule`. New members are plain `git init`/`git
  clone` followed by `nit adopt` (no `nit new` verb).
- Store shared roster in `.nit/roster.yaml`.
- Make metadata commands commit their own `.nit/` changes by default, with
  `--no-commit` only for batching planned roster edits.
- Deduplicate worktrees, symlinks, bind mounts, and cycles.
- Apply and repair local excludes.
- Implement `nit status`.

### Phase 2: Change Grouping

- Implement Change-Id generation and trailers.
- Implement `nit add` as explicit cross-repo staging.
- Implement `nit commit` over staged member changes.
- Implement Change projection, ambiguity detection, and views.
- Implement `nit change revert`.

### Phase 3: Pins

- Implement pin files under `.nit/pins/`.
- Implement `nit pin`, `nit pin --change`, and immutable pin ids (auto-generated;
  name optional).
- Implement `nit land [<name>]` and `nit commit --pin` transaction planning.
- Report pin drift in status.

### Phase 4: Checkout

- Implement safe `nit checkout`.
- Implement `nit checkout --exact` with confirmation/policy.
- Implement `nit worktree add <path> --pin <pin>` for isolated materialized
  workspaces, including detached or fresh `nit/wt/<name>` branch handling to
  avoid Git worktree branch-collision errors.
- Add fetch/reachability checks.
- Add dirty-worktree protections.

### Phase 5: Push And Review

- Implement push preflight, ordered push, partial landing reports, and resume.
- Implement `nit show`, `nit diff`, and `nit review` for Change and Pin.
- Add `nit doctor` recovery for trailers, pins, dangling pins, excludes, and
  remote drift.
- Implement opt-in `nit hooks install [--strict]` (trailer-stamp + pin-order
  warning; chains existing hooks). No hooks are installed by default.

---

## V2: Governance On Top

The old governance plan becomes v2.

V2 objects:

- **Aim**: a task contract with goal, scope, invariants, and eval policy.
- **Attempt**: one candidate Change, usually on per-repo branches.
- **Evidence**: evals, reviews, waivers, and logs tied to exact Change and Pin
  state.
- **Collapse**: the decision that selects one Attempt and lands or pins it.

Mapping:

- A v2 Attempt references one or more `Nit-Change-Id`s.
- A v2 Collapse usually creates or selects a Pin.
- Evals bind to exact member commits and/or a Pin.
- Scope enforcement and selectors are not in v1.

This keeps v1 small while preserving the path to agent governance:

> V1 makes cross-repo Git understandable. V2 decides which cross-repo change is
> safe enough to land.

---

## Non-Goals For V1

- No automatic semantic merge across repos.
- No new merkle DAG or replacement for Git history.
- No hidden destructive checkout.
- No required eval gates.
- No scope enforcement.
- No multi-attempt selector.
- No hosted web review integration beyond CLI / `nit review` output.

---

## Decisions (Locked)

Decided by the operator, plus the Claude/Codex convergence the operator delegated:

1. **Product name:** `nit`.
2. **Trailer spelling:** `Nit-Change-Id` (avoids colliding with Gerrit's
   `Change-Id`); "Change-Id" is shorthand only in prose.
3. **No-natural-root default:** `nit init --control` creates a tiny workspace
   control repo; `--local` is the explicit ad-hoc escape hatch.
4. **Pin labels:** ids are immutable; labels are movable pointers whose moves are
   recorded as commits; `nit checkout <label>` warns it is resolving a mutable
   pointer.
5. **Metadata commits:** `nit` commits its own `.nit/` changes by default
   (`--no-commit` to batch); users never hand-stage metadata.
6. **Hooks:** none installed by default. `nit hooks install [--strict]` is opt-in;
   safety comes from the reconstruct-not-enforce model and CI/server gates (v2).

### Verb design (operator: reuse Git verbs; do not add confusing arguments)

The surface is deliberately mostly Git verbs you already know, plus a few verbs
that name genuinely new concepts. No verb is folded into a confusing mode-flag.

| Operation | Verb | Rationale |
|---|---|---|
| init / clone / add / commit / push / status / log / diff / show / checkout / revert / worktree add | same as Git | direct mirrors; zero new mental load |
| publish a recorded workspace state | `nit land` | names a distinct compound op (commit + pin); kept as a verb precisely because hiding it in a forgettable `commit --pin` flag would silently downgrade the operation's meaning |
| register an existing repo as a member | `nit adopt` | `add` is taken by staging; new members are plain `git init`/`git clone` + `adopt`, with no `nit new` verb and no `adopt --init`/`--clone` mode flags |
| migrate a submodule | `nit import-submodule` | migration surgery, not adoption; a self-documenting verb beats a scary flag on `adopt` |
| reproducible cross-repo snapshot | `nit pin` | an artifact with a commit vector + provenance, not a mere pointer (so not `tag`) |
| combined cross-repo review artifact | `nit review` | renamed from `bundle` to avoid colliding with `git bundle` |
| diagnose / repair | `nit doctor` | idiomatic CLI diagnostic |

`nit sync` is retained only as an alias for `nit checkout`. There is no `nit
hydrate` (clone auto-hydrates; checkout clones-missing) and no `nit new`.
