---
layout: home

hero:
  name: Nit
  text: Multi-repo Git.
  tagline: "Commit, pin, and check out one change across independent repos. The punchline: no submodules."
  actions:
    - theme: brand
      text: Quickstart
      link: /guide/quickstart
    - theme: alt
      text: Why Nit
      link: /guide/design
    - theme: alt
      text: GitHub
      link: https://github.com/mostlydev/nit

features:
  - title: Keep your repos
    details: Members stay ordinary Git repos — own remotes, own branches, own history. Nit never rewrites them. Walk away and you still have plain Git.
  - title: One Change-Id
    details: nit commit stamps one id across every repo a change touches. Add, commit, push. The commits know they belong together.
  - title: Pins, not pointers
    details: nit land snapshots exact commits across repos; nit checkout reproduces any state. No gitlink to bump, no .gitmodules to babysit.
  - title: Submodules go home
    details: No hidden parent-pointer bump. No "I committed but the parent didn't update." No recursive clone roulette. The footguns are exposed or gone.
---

## Why Nit

You have a change that touches three repos. With submodules that means: commit each
child, remember to bump the parent pointer, commit the parent, push in the right
order or publish a dangling reference, and pray nobody is on a detached HEAD.

With Nit:

```sh
nit add -A
nit land -m "Ship the new field"
nit push
```

One Change-Id ties the commits together. One Pin records the exact state. The repos
stay independent — Nit just stops making you do the bookkeeping by hand.

> `git submodules go home` — scrawled on the wall of every monorepo migration.

Honest about the trade: against a submodule expert the keystroke win is modest. The
durable win is the footguns Nit deletes — push ordering, detached-HEAD commit loss,
gitlink bumps — and grouping that shows up in `nit change log` instead of living in your head.
