---
layout: home

hero:
  name: Gnit
  text: Multi-repo Git.
  tagline: "Commit, pin, and check out one change across independent repos — and teach your agents to do the same. Why? Because submodules suck."
  actions:
    - theme: brand
      text: Quickstart
      link: /guide/quickstart
    - theme: alt
      text: Gnit for agents
      link: /guide/agents
    - theme: alt
      text: Why Gnit
      link: "#why-gnit"
    - theme: alt
      text: GitHub
      link: https://github.com/mostlydev/gnit

features:
  - title: Keep your repos
    details: Members stay ordinary Git repos — own remotes, own branches, own history. Gnit never rewrites them. Walk away and you still have plain Git.
  - title: One Change-Id
    details: gnit commit stamps one id across every repo a change touches. Add, commit, push. The commits know they belong together.
  - title: Pins, not pointers
    details: gnit land snapshots exact commits across repos; gnit checkout reproduces any state. No gitlink to bump, no .gitmodules to babysit.
  - title: Teach your agents once
    details: gnit skills install puts the workflow into Claude Code, Codex, OpenCode, and Grok; gnit init leaves a guidance block in AGENTS.md. Agents drive cross-repo change instead of guessing at raw Git.
  - title: Built for shared workspaces
    details: Advisory locking keeps concurrent gnit invocations — yours and your agents' — from racing on workspace state. Multiple agents in one workspace is the expected topology, not an edge case.
  - title: Submodules go home
    details: No hidden parent-pointer bump. No "I committed but the parent didn't update." No recursive clone roulette. The footguns are exposed or gone.
---

## Why Gnit

You have a change that touches three repos. With submodules that means: commit each
child, remember to bump the parent pointer, commit the parent, push in the right
order or publish a dangling reference, and pray nobody is on a detached HEAD.

With Gnit:

```sh
gnit add -A
gnit land -m "Ship the new field"
gnit push
```

One Change-Id ties the commits together. One Pin records the exact state. The repos
stay independent — Gnit just stops making you do the bookkeeping by hand.

> `git submodules go home` — scrawled on the wall of every monorepo migration.

Honest about the trade: against a submodule expert the keystroke win is modest. The
durable win is the footguns Gnit deletes — push ordering, detached-HEAD commit loss,
gitlink bumps — and grouping that shows up in `gnit change log` instead of living in your head.

## Why agents

The tools that tried to fix multi-repo before — meta, Gitslave, mu-repo — died of
the same disease: every collaborator had to adopt them or the workflow fell apart.
Agents change that math. An agent harness reads `AGENTS.md`, installs a skill once,
and then *reliably* follows the workflow on every invocation — no onboarding, no
forgetting, no "I just used plain git, sorry."

Gnit leans into it: the skill is bundled in the binary, `gnit skills install --all`
teaches every harness on the machine, and the workspace guidance block makes any
agent that lands in the repo discover the workflow on its own. Humans get the same
commands; nobody is forced to adopt anything. See
[Gnit for agent harnesses](/guide/agents).
