# Workflow Invariants Audit

This audit captures the issue #2 failure class: a workflow check used a local or
optimistic proxy where the real invariant was distributed, uncertain, or
reference-dependent. The rule is to avoid treating "not on this local ref" or
"could not inspect" as "safe to proceed".

| Workflow | User Promise | Real Invariant | Risky Proxy Found | Status |
| --- | --- | --- | --- | --- |
| `nit push` with historical pins | Root metadata is published only after member commits are reconstructable. | Every committed Pin references member commits reachable from local `HEAD` or published `origin/*` refs after member pushes. | Pin reachability checked only against current local `HEAD`, so historical side-branch pins blocked later root pushes. | Fixed by accepting current `HEAD` or `refs/remotes/origin/*`, while still rejecting local-only or rewritten-away commits. |
| Transparent upkeep / `nit doctor` / `nit ignore` | Local `.git/info/exclude` repair is non-destructive. | Nit must read the existing exclude file before appending entries. | `read_to_string(...).unwrap_or_default()` treated unreadable or non-UTF-8 excludes as empty, then rewrote them. | Fixed by propagating non-`NotFound` read errors and never writing after an incomplete read. |
| `nit skills install` / `nit skills uninstall` | Managed skill installs are refreshed, but foreign or unreadable targets are not clobbered. | The target ownership marker and managed `SKILL.md` must be inspected accurately before replacement. | Marker read failures collapsed to foreign, and managed `SKILL.md` read failures collapsed to stale. | Fixed by propagating non-`NotFound` read errors. Missing marker remains foreign; missing managed `SKILL.md` remains self-healing stale. |
| `nit review <pin>` | Review output explains why a pinned member cannot be shown. | A missing local object is a local availability problem, not proof the commit does not exist remotely. | The command printed only `commit not available locally`. | Hardened message with the member path, commit context, and explicit `nit checkout` / `git fetch` remediation. No automatic fetch in the display command. |
| `nit change diff` | Show diffs for a reconstructed Change. | Change commits are discovered from local refs before showing them. | No confirmed gap: commits come from `git log --all`, so `git show` receives local object ids. | Left unchanged. |
| `nit pr` / `nit pr open` | Create or adopt ordinary PRs without duplicates or partial preflight creation. | Selected Change must be on the branch, the pushed head must match local `HEAD`, and the base ref must be known. | No issue #2 class bug found. The PR path already fetches base refs and checks remote head equality. | No change in this batch. |
| `nit status` | Report root/member state clearly. | Ignore Nit-owned metadata reliably across Git status formats. | Non-`-z` porcelain parsing can miscount unusual rename/copy lines involving `.nit/`. | Follow-up polish issue candidate. |
| `nit log` | Show a stable unified timeline of Changes and Pins. | Equal timestamps need deterministic ordering. | Entries with identical timestamps have no secondary sort key. | Follow-up polish issue candidate. |
| `nit push` branch hints | A pin blocked on an unpublished side branch should name or publish the satisfying branch. | If `branch_hint` is the intended publication ref, the push path should make the remediation obvious. | Current push holds root for commits reachable only from a local non-current branch, but does not push or name the branch. | Follow-up behavior issue candidate. |

## Batch Boundary

This batch includes the confirmed safety fixes and the actionable `review`
diagnostic. It does not add network behavior to read-only display commands, and
it does not expand the push verb to publish branch hints. Those changes are
separate operator/product decisions because they alter remote ref behavior.
