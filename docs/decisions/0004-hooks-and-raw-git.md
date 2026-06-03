# 0004: Hooks And Raw Git

## Status

Accepted.

## Decision

Nit does not install Git hooks by default.

Raw Git usage is allowed. Nit reconstructs workspace state from Git history,
Change trailers, the roster, and Pin artifacts. `nit status`, `nit checkout`,
`nit doctor`, and `nit push` surface and repair drift.

`nit hooks install` is an opt-in convenience. `nit hooks install --strict` is an
opt-in mode for managed agent environments. CI or server-side checks are the
authoritative enforcement boundary.

## Rationale

Local hooks are bypassable and can conflict with existing hook managers. They
are useful safety rails, not the trust boundary.

