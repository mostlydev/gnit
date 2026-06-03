# Legit v1 - Superseded Plan

> Superseded. This draft is retained for context. The current authoritative
> design is [../nit-design.md](../nit-design.md).

# Legit v1 - Converged Plan: Ergonomic Multi-Repo Git

## Status

Converged design proposal from an adversarial Claude/Codex working session (6 rounds).
Supersedes the framing of `legit_design.md` for v1 scope. The governance model in
`legit_design.md` is **not discarded** — it is deferred to v2 and kept forward-compatible
(see §9). This document is the v1 plan; `legit_design.md` remains the v2 north star.

---

## 1. The Reframe

`legit_design.md` described a governance protocol (Aims, Attempts, Eval Results, Collapse
Manifests) for the multi-attempt agent code loop. The operator's pressure-test reframed the
**v1 surface** around a simpler, more universal pain:

> Working across several independent-but-colocated Git repos (nested and/or bind-mounted)
> means hand-cranking the dance: commit in child → stage the parent's gitlink/manifest →
> commit in parent. For one logical change spanning N repos, this is N×3 steps of bookkeeping,
> no unified view, no unified rollback, and no record that the commits belong together.

Legit v1 makes **one logical change across N repos** feel like one operation, for humans and
agents alike — without forcing those repos to merge their lifecycles.

This is **not** "another `mr`/`meta`/`repo forall`." Fanning a command across repos is
commoditized. The defensible, unfilled gap is: **treating a cross-repo change as a first-class
unit**, and **killing the parent-pointer bookkeeping** that submodules make painful.

---

## 2. The Core Insight (the whole elegance)

The old design's pain came from **conflating two things that change on completely different
clocks**. Split them and the dance vanishes:

| Concern | What it is | Changes when | Stored as |
|---|---|---|---|
| **Membership roster** | which repos are in the workspace | rarely — on `adopt`/`ignore` | one sidecar file: `{node-id, path, remote-alias}[]` |
| **Change grouping** | which commits form one logical change | per commit | a `Change-Id` trailer on ordinary Git commits |

The old "parent stages a manifest **every change**" pain existed only because membership and
grouping were fused into one per-change parent commit. Separate them and:

- There is **no parent commit per change.** Each repo just commits its own work, tagged with a
  shared `Change-Id`.
- The **only genuinely new durable artifacts** are: (a) one workspace roster file, which changes
  rarely, and (b) a trailer on commits. Everything else — unified status, log, diff, the
  cross-repo graph — is a **projection** computed by scanning the member repos for `Change-Id`
  trailers.
- **Git remains the source of truth for everything**, including the change graph. The roster is
  a membership list, not a history.

> The irreducible primitive is the `Change-Id`. The lock, the graph, the unified views are all
> projections of `Change-Id`-tagged commits across the roster.

This is literally the `Aim-Id`/`Attempt-Id` trailer from `legit_design.md` doing all the load-
bearing work, with the governance edifice peeled off.

### 2.1 The projection rule (how a `Change-Id` resolves)

"The commit carrying `Change-Id` X" is underdefined — a repo may have several commits with X,
X on multiple branches, or orphaned copies after a rebase. The projection is defined precisely:

> Within each member repo, **and within a selected ref scope** (default: the member's current
> branch/HEAD), find the **maximal reachable commits** carrying `Change-Id` X.
> - **exactly one tip** → that is the repo's *participant tip* for X
> - **zero** → the repo is *absent* from X
> - **more than one tip** (divergent branches both carry X) → X is **ambiguous** in that repo
>   until the caller selects a ref/tip or pins a lock.

`legit show X` and `legit log` **must report ambiguity explicitly** — they never silently pick a
tip by timestamp. `--all-refs` widens the scope beyond the current branch. This rule is what
makes the graph well-defined; everything downstream (show, diff, lock, revert) consumes it.

### 2.2 The roster is shared by default

Because the operator wants a **retrievable, shared** history/log graph, the graph must be
retrievable by *anyone with the repos* — which means everyone must know **which repos to scan**.
A local-only roster makes the graph personal, not shared. Therefore:

- **Default: the roster is a committed, shared artifact.** It lives committed in a designated
  **root member repo** when one exists (the common case — you usually have a main repo).
- **No natural root?** `legit init` either creates a tiny dedicated **workspace-control repo**
  (the manifest-repo pattern) or drops to **local-only** mode — but local-only is an explicit
  escape hatch for ad-hoc assemblies, not the default.
- Sharing has an honest cost: the roster needs a home, and `adopt`/`ignore` become **rare,
  reviewable roster commits**. This does **not** break the roster-vs-grouping split: roster
  commits happen on membership change (rare), never per logical change (which stays trailer-only).

---

## 3. Two Modes — and why this split IS the design

The single most important thing to keep explicit: **default Legit does NOT solve reproducible
superproject pinning.** That capability has real, unavoidable costs (it is submodule-class), so
it lives behind an explicit opt-in. Users who never need it never pay for it.

### Default mode — Incidental Workspace (the common path)

The repos are simply colocated and you want to operate on them as one change.

- Workspace = a **set** of member repos in a roster sidecar. Branches stay **per repo**.
- **No parent pointer.** A parent repo (if any) does not record child commits. The child↔parent
  relationship is filesystem layout, not a versioned dependency.
- Therefore: **no manifest bump per change, no materialize-on-checkout, no detached-HEAD
  question, no child-before-parent push ordering** — there is nothing in a parent to keep in
  sync, so none of the submodule failure classes can occur.
- Bind mounts and symlinked repos are a **non-event**: deduped by repo identity (see §5), and no
  parent tree ever absorbs mounted files.

### Opt-in mode — Pinned / Superproject Workspace (the minority path)

You need a parent repo to pin **exact** child versions, reproducibly (deploy superproject, CI
that must check out a coherent cross-repo state).

- `legit lock` serializes the current `{node → commit}` set into a lock artifact.
- `legit sync` materializes a lock: reconciles each member's working tree to its pinned commit.
- **This is where the submodule-class costs live, by construction**, and only here:
  reproducible checkout, the materialization/detached-HEAD question (§6-G), and fragmented host-
  native review (the parent shows a lock diff, not child code; §6-H).

The elegance is the **honesty of this split**: incidental mode is cheap because it deliberately
does not promise reproducible pinning; lock mode promises it and charges for it.

---

## 4. Objects (v1 data model)

Four small things. Three are derived/optional.

1. **Workspace roster** (durable, rarely-changing). A **committed, shared** sidecar (§2.2) under
   `.legit/`, listing members as `{node-id, path, remote-alias[], local-excludes}`. `node-id` is
   a generated logical slot id (§5); `local-excludes` records which child paths each ancestor must
   exclude so any machine can re-apply them (§6-A, §6-K). This is the **only** Legit file required
   in default mode. Local-only mode is an explicit escape hatch.

2. **Change** (a *projection*, not a stored object). A `Change-Id` value that groups commits,
   resolved by the projection rule (§2.1). Intent is **not** a source-less record: it lives in
   **trailers** on the change's first commit in each participating repo —
   `Change-Id` (required), and **optional, unenforced** `Change-Goal`, `Change-Actor`,
   `Change-Session` reserved for v2 governance. (Richer/structured fields like multi-glob
   `scope` are awkward as single-line trailers and defer to v2 + local cache; v1 does not invent
   storage for them.)

3. **Lock** (opt-in, derived). A serialized `{node → commit (+branch hint)}` snapshot — a
   **resolved** projection of a change or of current workspace state, used for
   reproducibility/CI/handoff. It is *not* the source of truth for grouping, and deriving it may
   **require explicit tip selection** when a repo's tips are ambiguous (§2.1). It pins SHAs, so it
   breaks on SHA rewrite **by design** — which is exactly why the trailer is primary and the lock
   is opt-in.

4. **Journal/cache** (optional, local). Acceleration + recovery evidence (discovery cache, push
   outcomes, partial-landing state). **Never** a source of truth; rebuildable.

---

## 5. Node Identity (resolves the round-4 contradiction)

A node's identity is a **workspace-manifest slot id**, generated at `legit init`/`adopt` and
stored **in the roster**, not in repo-local committed metadata.

- **Path is location, not identity.** Moving/renaming a repo preserves its node-id.
- **Remote is evidence, not identity.** The same remote may appear as two nodes; two clones of
  one remote are two nodes unless explicitly declared mirrors of one logical slot.
- A standalone repo does **not** carry a global node-id; identity belongs to the workspace
  assembly. The same repo can be a node in two different people's workspaces with different ids.
- The cross-machine graph resolves nodes by an **alias bundle** (remote URLs + git-common-dir +
  current path), not by id. Re-cloning the same workspace roster preserves node-ids because they
  live in the roster.
- For **discovery before adoption**, identity is a temporary fingerprint: git-common-dir/inode +
  remote aliases + current path — used only to dedupe (bind mounts, symlinks, worktrees) so one
  physical repo is never committed twice.

---

## 6. Failure Modes (and where each one is handled)

| Failure mode | Handling |
|---|---|
| **A. Untracked child dirs in an ancestor** | `legit adopt R` excludes R's path from **every managed ancestor** that would otherwise see it as untracked, via **local** excludes (`.git/info/exclude`), never a committed `.gitignore`. Default commit **never** stages child files into a parent. |
| **B. Child already tracked in an ancestor** | Refuse + explain whether it is tracked files, a gitlink/submodule, or an existing lock-mode relationship. Do not silently exclude. |
| **C. Partial commit** (one member's commit fails, e.g. hook rejects) | Local and reversible. Report exactly which members committed; offer to continue or reset the group. |
| **D. Partial push** (one remote fails) | **Default mode:** no *structural* ordering requirement — no pointer can dangle. Preflight all (access + fast-forward), push the attempted set, **report partial landing**, support resume. **But** semantic/CI races still exist (app's CI may run before sdk's push is visible), so default push uses a **stable order** and may honor **optional dependency hints** declared in the roster — unordered is not claimed as a virtue. **Lock mode** is the only mode with a *hard structural* ordering requirement (members before the lock commit). |
| **E. Partial landing is OBSERVABLE** | `legit status --change X` shows, per member, whether X is local-only vs pushed. The old design's "make non-atomicity visible & recoverable" goal **falls out for free** from `Change-Id`. |
| **F. Rebase / cherry-pick** | `Change-Id` is a commit-message trailer, so it **survives ordinary rebase/cherry-pick** that preserve messages — where a SHA-pinning lock would break. **No magic claim:** a human dropping the trailer or a bad squash loses the link; hooks/templates/`doctor` protect and repair from the local cache. |
| **G. Materialization / detached HEAD** | **Lock mode only.** Lock pins a SHA and may record a branch hint. `legit sync` default: fast-forward/checkout only when safe and on the hinted branch; if the pinned commit would require rewind/detach, **stop and show the exact action**; `legit sync --exact` detaches/resets only with confirmation/policy. Detached HEAD is an explicit mode, never a surprise. |
| **H. Fragmented host review** | **Lock mode** parent PRs show a lock/YAML diff, not child code. Reviewing the change as a unit needs `legit show <change>` / `legit diff <change>` to render the combined diff (the platform cannot). v1 renders a combined **CLI** diff; web/review integration defers. In **default mode** there is no parent PR to fragment — each member's change is reviewed in its own repo, unified via `legit show`. |
| **I. Cycles** | A nests B which bind-mounts back to A: visited-set keyed on canonical identity; refuse, do not loop. |
| **J. Stale view after plain `git commit` in a member** | `legit status` reconstructs from live repo HEADs + trailers, so a plain `git commit` simply shows up; `legit doctor` reconciles cache drift. |
| **K. Fresh checkout of a shared workspace** | Local excludes (`.git/info/exclude`) are **not** committed, so a teammate who clones the workspace would see child dirs as untracked. `legit init`/`adopt` record the excludes in the **shared roster** (§4-1); `legit doctor` (or an implicit sync-roster on first run) **re-applies** them locally on each machine. Without this, every teammate's ancestor repos show child dirs as untracked. |

---

## 7. CLI Surface (v1)

Default-mode happy path is the top four. Everything else is observational or opt-in.

```text
legit init                      # establish a workspace roster here
legit adopt <path>              # add a discovered repo to the managed set (writes roster + local excludes)
legit ignore <path>             # explicitly exclude a discovered repo
legit status [--change <id>]    # recursive by default; grouped; shows partial-landing
legit commit -m <msg>           # commit each managed repo's STAGED changes, stamp one Change-Id
legit commit -m <msg> --change <id>   # append to an existing change
legit log [--change <id>]       # projection: per-repo history grouped by Change-Id
legit show <change>             # combined cross-repo diff/summary for one change (CLI render); reports ambiguity (§2.1)
legit diff [--change <id>]      # projection
legit revert <change>           # per-repo `git revert` of participant tips; stamps a NEW Change-Id
                                #   carrying `Reverts-Change-Id: <change>`. Non-destructive (never reset).
legit push [--resume]           # preflight all; push attempted set; report partial landing
                                #   default: STABLE order (+ optional roster dep hints). lock mode: structural ordering.

# opt-in superproject / reproducibility mode:
legit lock <change|current>     # serialize resolved {node -> commit (+branch hint)} (may need tip selection, §2.1)
legit sync [--exact]            # materialize a lock to working trees (see §6-G)

legit doctor                    # reconcile cache/journal with live Git + trailers; repair links;
                                #   re-apply local excludes from the shared roster (§6-K)
```

Recursion is **tiered by verb, not a global `--recursive`**:
- **read** (`status`/`log`/`diff`/`show`): always recurse the roster. `--repo-only`/`--local` suppresses.
- **local write** (`commit`): recurse over members with **staged** changes by default. Respects
  staging — **never** transitively applies `-a`/auto-stage. Members with nothing staged are
  skipped silently.
- **outward** (`push`): high-friction. Preflight + show plan; interactive confirms, agents need
  `--yes`/policy. Never a silent outward mutation.
- A config policy (e.g. `legit.recurse.push`) lets managed agents/power users opt into more while
  casual users keep guardrails.

---

## 8. Change-Id Lifecycle

- A fresh `Change-Id` is generated per `legit commit` by default.
- `legit commit --change <X>` **appends** to an existing change, so an agent (or human) can
  accrete one logical cross-repo change over several commits across several repos.
- (Deferred convenience, not v1 source of truth: a `legit start`/active-change pointer so you
  don't pass `--change` each time. v1 does not require it.)

---

## 9. Forward-Compatibility (governance deferred, not designed-out)

v1 enforces **no** governance. It only **reserves the seams** so v2 does not require a schema
migration:

- Intent lives in **commit trailers** (§4-2), not a source-less record: optional, unenforced
  `Change-Goal`/`Change-Actor`/`Change-Session` on the change's first commit per repo. Structured
  scope defers to v2.
- A future **Aim** (`legit_design.md`) = a contract that one or more **competing `Change-Id`s**
  attempt to satisfy; **selection** picks one; **Collapse** is the landing/finalization act.
- **Attempt** maps onto a `Change-Id` (one candidate cross-repo change); evals, gates, scope
  enforcement, and the collapse selector are all v2.

So the path is: v1 ships the ergonomic base (roster + `Change-Id` + projections + fan-out), and
v2 layers governance on top **without** re-modeling the journal.

---

## 10. What v1 Explicitly Does NOT Do

- No governance enforcement: no required evals, no gates, no scope blocking, no collapse selector.
- No multi-attempt selection/ranking.
- No reproducible superproject pinning **unless** the user opts into lock/sync.
- No new cross-repo VCS / merkle DAG. The graph is a projection of trailers; Git is the truth.
- No web/host review integration (combined diff is CLI-only in v1).
- No automatic synthesized merges.

---

## 11. Open Questions for the Operator

Two prior questions are now **decided in the design** (no longer open): the roster is **shared
by default** (§2.2, forced by the shared-graph requirement), and intent is stored in **trailers**
(§4-2). What remains genuinely for the operator:

1. **Mode emphasis.** Is your primary picture **incidental** (a few repos in a dir, "let me
   commit/push/see them as one change") or do you genuinely need **lock/superproject pinning**
   for reproducible cross-repo checkout? v1 defaults to incidental and makes pinning opt-in; if
   pinning is actually your core need, we invest more in `sync` up front.
2. **Naming.** Keep `legit`? Keep `Change-Id` as the trailer name, or align it now with the v2
   `Aim-Id`/`Attempt-Id` vocabulary to avoid a later rename?
3. **Replace vs coexist.** Should this plan **edit** `legit_design.md` (reframing it as the v2
   north star with v1 carved out), or remain a separate companion document?
