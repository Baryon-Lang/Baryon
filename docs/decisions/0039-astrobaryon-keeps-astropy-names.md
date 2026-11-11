# 0039. AstroBaryon keeps AstroPy's names

Status: Accepted
Date: 2026-11-11
Roadmap: D.24

## Context

AstroBaryon (roadmap 4.2) is Baryon's astronomy library: AstroPy's capability written natively
in Baryon, designed from the astronomy standards rather than translated from AstroPy's Python
([AstroBaryon roadmap](../design/astrobaryon-roadmap.md)). Collins made documentation and an
easy move from AstroPy its first priority.

Two ways to name its API were open:

1. Keep AstroPy's module, class, function and argument names wherever the behavior is the same,
   such as `Time`, `SkyCoord`, `u.deg` and `Table.read`.
2. Design names fresh from the standards, and map every AstroPy call to its new name in the
   transition guide.

Baryon already chose Python's names where it builds Python's behavior: decision
[0036](0036-files-and-processes.md) uses Python's `open`, `os` and `subprocess` names for its
native file and process library.

## Decision

Collins chose option 1 on 2026-11-11.

- AstroBaryon's modules, classes, functions and arguments take AstroPy's names wherever their
  behavior is the same.
- Where Baryon's types force a difference, such as units checked at compile time, frames that are
  values rather than subclasses (no inheritance, [decision 0015](0015-no-class-inheritance.md)),
  or errors as values, the name stays and the transition guide (roadmap 4.2.11) records the
  difference.
- A name keeps AstroPy's meaning or is not used: a call that behaves differently enough to
  surprise an AstroPy user gets a different name, also listed in the guide.
- The internals follow the standards (ERFA, the FITS and WCS papers, the IVOA formats), not
  AstroPy's Python.

## Consequences

- An AstroPy user reads AstroBaryon code without a dictionary, and most of the transition guide
  is a list of differences rather than a list of renames.
- AstroPy's names carry its history, including names a fresh design would not pick. AstroBaryon
  accepts that cost for familiarity.
- Every AstroBaryon row must check its names against AstroPy's and list each difference in the
  guide before it is `Done`. The guide's coverage list (4.2.11) makes a missing entry visible.
- A Baryon language rule that blocks an AstroPy name or call shape, such as operators on user
  types before D.19 is decided, shows up as a recorded difference, not a silent rename.
