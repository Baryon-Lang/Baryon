# 0023. The language is named Baryon, its files end in `.byn`, and its tool is `baryon`

Status: Accepted
Date: 2026-11-07
Roadmap: D.6

## Context

The project started under the working name NPL ("new programming language"), with source files
ending in `.npl` and a command-line tool called `npl`. Roadmap item D.6 left the final name open,
and the VS Code extension (1.17) waits on it, because an extension registers a language id and a
file extension that are awkward to change once people install it.

A name is cheap to change today and expensive later. Every example, test, document, symbol and
editor integration carries it, and outside the repository so do package names, search results
and people's memories. The name had to:

1. **Say what the language is for.** Its users are physicists, mathematicians and data
   scientists, so a physics or astronomy name tells them at once who it is for.
2. **Be short and plain.** Like Carbon: one obvious spelling and one pronunciation.
3. **Not collide.** No existing programming language, compiler or developer tool with the name,
   and no well-known product in scientific computing, HPC or data science.
4. **Have its package names free.** `baryon-lang` is free on both crates.io and PyPI, the form
   Carbon uses (`carbon-lang`). The bare `baryon` is free on PyPI. On crates.io it belongs to a
   small 3D engine last released in 2021.

About 200 candidates were checked on 2026-11-07: elements, particles, quasiparticles,
astronomers, stars, moons and Greek and Latin sky words. Most were taken by a language, a
library in the same field or a trademark (for example Triton, Fermi, Kepler, Astra, Solaris,
Metis, Hyperon). The shortlist is in the project's naming document. Collins chose Baryon over
Casimir.

## Decision

- The language is **Baryon**. A baryon is three quarks bound into one particle, as the language
  binds Python's readability, Rust's safety and Fortran's speed into one. The name comes from
  the Greek *barys*, "heavy", which fits heavy computation. Nearly all the visible mass in the
  universe is baryonic.
- Source files end in **`.byn`**. It is short, it reads as the name, and no common format uses
  it.
- The command-line tool is **`baryon`**: `baryon check`, `baryon build`, `baryon run`,
  `baryon doc` and `baryon lsp`.
- Names inside the compiler follow: the runtime's C entry points are `baryon_rt_*`, and the
  backend's private globals start with `baryon.`.
- Every reference to the old name changes, including in accepted decision records and finished
  roadmap rows, so a reader never meets two names for one language. Only this record's context
  names NPL, to say what was renamed.
- The repository is `Baryon-Lang/Baryon`. Links to the old `csimpson1964/New-Programming-Language`
  address redirect, but the tree uses the new one.

## Consequences

- One pull request renames everything at once, so there is never a mix of the old and new names in
  the tree. Open branches that touch the same lines need a merge of `main` after it lands.
- Files ending in the old extension are no longer packages' sources: `baryon check` and
  `baryon build` look for `.byn` only. Nothing outside this repository uses the language yet, so nothing breaks.
- The VS Code extension (1.17) is unblocked and registers the language id `baryon` for `.byn`.
- The `Baryon-Lang` GitHub organization now holds the repository. Before the name is announced, a
  domain and a formal trademark search still need checking. The checks above were web searches and registry lookups only.
