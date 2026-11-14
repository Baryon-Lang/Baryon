# 0061. AstroBaryon ships in the standard distribution until a package registry exists

Status: Accepted
Date: 2026-11-13
Roadmap: D.23, 4.2

## Context

AstroBaryon (row 4.2) is the astronomy library. It could ship inside Baryon's standard
distribution, or as a separate package. A separate package needs a package registry and an
installer, which do not exist, and building them first would delay AstroBaryon.

## Decision

AstroBaryon ships in the standard distribution. It moves to its own package once a package
registry exists. Collins chose this on 2026-11-13.

## Consequences

- Every install carries AstroBaryon until the move, and installing it needs no extra step.
- Its modules are named like the rest of the standard library, so a user's file cannot reuse
  their names while it ships inside.
- The move to a package is planned work when a registry arrives, and keeps the modules' names.
