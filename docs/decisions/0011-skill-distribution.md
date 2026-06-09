# 0011: Skill Distribution

## Status

Accepted. Implementation in progress for v0.6.0.

## Context

Agent harnesses (Claude Code, Codex, OpenCode, Grok) load "skills" — small
instruction bundles — from a per-harness directory. A skill that teaches a
harness how to drive Nit lets agents use the change/pin/checkout/push workflow
correctly instead of guessing at raw Git.

Talking Stick solves the same problem with `tt install <harness>`, which links
its bundled `skills/talking-stick/` into each harness's skills directory. That
works because `tt` is distributed as an npm package whose files, including the
skill source, are present on disk for every install.

Nit is different. It ships as a single binary: `install.sh` and the release
tarball move only the `nit` executable, so there is no on-disk `skills/`
directory for an installed user to link against. A design that symlinks harness
directories straight at a repository path would only work for developers with a
checkout.

## Decision

Nit gains a `nit skills` command group that installs a bundled agent skill into
supported harnesses. The skill source of truth lives at `skills/nit/SKILL.md`
in the repository and is compiled into the binary with `include_str!`, so every
build carries its own copy.

- `nit skills install [<harness>...] [--all] [--copy|--link] [--print]`
- `nit skills uninstall [<harness>...] [--all] [--print]`
- `nit skills list` reports per-harness state (linked, copied, absent, stale).

### Managed source

On install, Nit first materializes a **managed source** from the embedded
content at `<data>/skills/nit/SKILL.md`, where `<data>` is the first of
`$NIT_DATA_DIR`, `$XDG_DATA_HOME/nit`, or `~/.local/share/nit`. If the embedded
content differs from what is on disk, the managed source is refreshed. This is
the single point of truth on the user's machine.

### Link vs copy

- `--link` (default) symlinks each harness skill directory to the managed
  source. One `nit skills install` after a `nit update` refreshes every linked
  harness at once. This keeps Nit consistent with Talking Stick's link-default
  while still working for binary-only installs, because the symlink target is a
  stable Nit-owned location rather than a repository path.
- `--copy` writes an independent copy of the skill into each harness directory.
  Use it when symlinks are undesirable.

### Supported harnesses and paths

| Harness | Skill directory | Aliases |
| --- | --- | --- |
| Claude Code | `~/.claude/skills/nit` | `claude`, `claude-code` |
| Codex | `~/.codex/skills/nit` | `codex` |
| OpenCode | `~/.opencode/skills/nit` | `opencode` |
| Grok | `${GROK_HOME:-~/.grok}/skills/nit` | `grok`, `grok-build` |

Gemini is deferred: its skills are registry-managed through `gemini skills
link`, which needs the Gemini CLI and a different mechanism.

### Resolution, detection, and safety

- All roots are environment-overridable (`HOME`, `XDG_DATA_HOME`, `NIT_DATA_DIR`,
  `GROK_HOME`) so the behavior is testable in a sandboxed home.
- `--all` targets only **detected** harnesses (those whose base config
  directory exists) and reports the rest as skipped.
- An explicitly named harness is always installed; its skill directory parents
  are created as needed. If the harness's base directory did not already exist,
  the run says so rather than failing.
- Nit never silently clobbers. If a harness skill directory already exists and
  is not a symlink Nit owns (a real directory, or a symlink pointing elsewhere),
  the install reports it and leaves it untouched unless `--force` is given.
- `--print` makes no changes and prints the planned actions per harness.
- `nit skills uninstall` removes the harness entry. The managed source is left
  in place; it is small and harmless, and a later install reuses it.

### Explicit, never automatic

Installing a skill is always an explicit `nit skills install`. Nit does not
auto-install or silently re-align skills on ordinary commands, consistent with
the project rule that Nit changes the user's machine only when asked.

## Consequences

- Installed binary users, not just developers, can run `nit skills install`.
- A single `nit update` plus `nit skills install` refreshes every linked
  harness from one managed source.
- The skill content rides inside the binary, so it is always version-matched to
  the running Nit and needs no separate download.
- Grok's session-hook machinery is intentionally not reproduced; that is Talking
  Stick identity wiring, not skill distribution.

## Rejected Alternative

Symlink harness directories directly at a repository or package `skills/` path,
as `tt` does. Rejected because Nit's release tarball ships only the `nit`
binary, so that path does not exist for installed users. The managed-source
indirection keeps the link-default ergonomics without assuming a checkout.

Inverting the default to `--copy` and offering `--link` only inside a developer
checkout was also considered. Rejected because the managed source lets `--link`
stay the default for everyone, which gives the refresh-all-at-once benefit that
makes linking worthwhile.
