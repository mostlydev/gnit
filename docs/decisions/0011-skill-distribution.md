# 0011: Skill Distribution

## Status

Accepted. Implementation in progress for v0.6.0.

## Context

Agent harnesses (Claude Code, Codex, OpenCode, Grok) load "skills" — small
instruction bundles — from a per-harness directory. A skill that teaches a
harness how to drive Gnit lets agents use the change/pin/checkout/push workflow
correctly instead of guessing at raw Git.

Talking Stick solves the same problem with `tt install <harness>`, which links
its bundled `skills/talking-stick/` into each harness's skills directory. That
works because `tt` is distributed as an npm package whose files, including the
skill source, are present on disk for every install.

Gnit is different. It ships as a single binary: `install.sh` and the release
tarball move only the `gnit` executable, so there is no on-disk `skills/`
directory for an installed user to link against. A design that symlinks harness
directories straight at a repository path would only work for developers with a
checkout.

## Decision

Gnit gains a `gnit skills` command group that installs a bundled agent skill into
supported harnesses. The skill source of truth lives at `skills/gnit/SKILL.md`
in the repository and is compiled into the binary with `include_str!`, so every
build carries its own copy.

- `gnit skills install [<harness>...] [--all] [--copy|--link] [--print]`
- `gnit skills uninstall [<harness>...] [--all] [--print]`
- `gnit skills list` reports per-harness state (linked, copied, absent, stale).

### Managed source

On install, Gnit first materializes a **managed source** from the embedded
content at `<data>/skills/gnit/SKILL.md`, where `<data>` is the first of
`$GNIT_DATA_DIR`, `$XDG_DATA_HOME/gnit`, or `~/.local/share/gnit`. If the embedded
content differs from what is on disk, the managed source is refreshed. This is
the single point of truth on the user's machine.

### Link vs copy

- `--link` (default) symlinks each harness skill directory to the managed
  source. One `gnit skills install` after a `gnit update` refreshes every linked
  harness at once. This keeps Gnit consistent with Talking Stick's link-default
  while still working for binary-only installs, because the symlink target is a
  stable Gnit-owned location rather than a repository path.
- `--copy` writes an independent copy of the skill into each harness directory.
  Use it when symlinks are undesirable.

### Supported harnesses and paths

| Harness | Skill directory | Aliases |
| --- | --- | --- |
| Claude Code | `~/.claude/skills/gnit` | `claude`, `claude-code` |
| Codex | `~/.codex/skills/gnit` | `codex` |
| OpenCode | `~/.opencode/skills/gnit` | `opencode` |
| Grok | `${GROK_HOME:-~/.grok}/skills/gnit` | `grok`, `grok-build` |

Gemini is deferred: its skills are registry-managed through `gemini skills
link`, which needs the Gemini CLI and a different mechanism.

### Resolution, detection, and safety

- All roots are environment-overridable (`HOME`, `XDG_DATA_HOME`, `GNIT_DATA_DIR`,
  `GROK_HOME`) so the behavior is testable in a sandboxed home.
- `--all` targets only **detected** harnesses (those whose base config
  directory exists) and reports the rest as skipped.
- An explicitly named harness is always installed; its skill directory parents
  are created as needed. If the harness's base directory did not already exist,
  the run says so rather than failing.
- Gnit never silently clobbers. If a harness skill directory already exists and
  is not a symlink Gnit owns (a real directory, or a symlink pointing elsewhere),
  the install reports it and leaves it untouched unless `--force` is given.
- `--print` makes no changes and prints the planned actions per harness.
- `gnit skills uninstall` removes the harness entry. The managed source is left
  in place; it is small and harmless, and a later install reuses it.

### Explicit, never automatic

Installing a skill is always an explicit `gnit skills install`. Gnit does not
auto-install or silently re-align skills on ordinary commands, consistent with
the project rule that Gnit changes the user's machine only when asked.

## Consequences

- Installed binary users, not just developers, can run `gnit skills install`.
- A single `gnit update` plus `gnit skills install` refreshes every linked
  harness from one managed source.
- The skill content rides inside the binary, so it is always version-matched to
  the running Gnit and needs no separate download.
- Grok's session-hook machinery is intentionally not reproduced; that is Talking
  Stick identity wiring, not skill distribution.

## Rejected Alternative

Symlink harness directories directly at a repository or package `skills/` path,
as `tt` does. Rejected because Gnit's release tarball ships only the `gnit`
binary, so that path does not exist for installed users. The managed-source
indirection keeps the link-default ergonomics without assuming a checkout.

Inverting the default to `--copy` and offering `--link` only inside a developer
checkout was also considered. Rejected because the managed source lets `--link`
stay the default for everyone, which gives the refresh-all-at-once benefit that
makes linking worthwhile.
