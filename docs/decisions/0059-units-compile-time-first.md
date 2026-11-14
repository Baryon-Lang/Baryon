# 0059. Physical units are checked at compile time first, with a run-time unit later

Status: Accepted
Date: 2026-11-13
Roadmap: D.20, 4.2.3

## Context

The design plan names units checked at compile time and erased to plain numbers as a Phase 3
candidate: a mistake such as adding metres to seconds is caught before the program runs, and the
numbers cost nothing extra. Astronomy also reads units from files, such as a FITS header's
`BUNIT`, which no compile-time check can know; AstroPy's `Quantity` carries its unit at run time
for that.

## Decision

Both, in order. Units are checked at compile time and erased first. A run-time unit for values
read from files, like AstroPy's `Quantity`, follows for AstroBaryon (row 4.2.3). Collins chose
this on 2026-11-13. Units build on operators on user types
([decision 0058](0058-operators-are-dunder-methods.md)).

## Consequences

- Unit mistakes in code are compile errors, at no run-time cost.
- Astronomy code can still check a unit read from a file, at the cost of carrying it at run time
  only where it is used.
- Two forms must convert into each other cleanly; the record that designs the run-time unit says
  how.
