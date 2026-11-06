# Decision records

A decision record captures one choice that is hard to reverse or that affects more than one crate:
what was decided, why, and what it costs. Records are never edited after acceptance except to
change their status; a new record supersedes an old one.

## Format

Name the file `NNNN-short-title.md`, numbered in order, and use this outline:

```markdown
# NNNN. Title

Status: Proposed | Accepted | Superseded by NNNN
Date: YYYY-MM-DD

## Context

The problem and the forces at play.

## Decision

What we will do.

## Consequences

What becomes easier, what becomes harder, and what we give up.
```

## Index

| Record | Status |
| --- | --- |
| [0001. Modular crate architecture with enforced dependency rules](0001-modular-crate-architecture.md) | Accepted |
| [0002. The roadmap is the source of truth for project management](0002-roadmap-source-of-truth.md) | Accepted |
| [0003. A C++ and Fortran baseline suite in `bench/`, run by a Rust harness](0003-benchmark-suite.md) | Accepted |
