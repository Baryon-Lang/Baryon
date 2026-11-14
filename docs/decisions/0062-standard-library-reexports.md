# 0062. A standard-library module re-exports a name with `from x import name as name`

Status: Accepted
Date: 2026-11-14
Roadmap: 3.18.16

## Context

Every standard-library module is one file, `<std>/name.byn`
([decision 0038](0038-standard-library-in-baryon.md)). `CONTRIBUTING.md` asks for a file past
about 500 lines to be split by responsibility, and some modules are far past that. `special`
(roadmap 3.18) holds about 1,750 lines once the Bessel functions are in, and about 3,900 once
the rest of 3.18 is. `linalg` is about 1,060 lines, which is why idea I.50 asks how a module
could span files.

The library already has internal files, which a program cannot import: `byplot` draws through
`figure`, `artist` and the rest ([decision 0046](0046-plotting.md)), and the compiler calls
`frame_joins` for a frame's `merge`. A public module can call into an internal file, but it
cannot pass the internal file's names on to programs. The resolver never follows a name a
library file imports, so that `from math import sqrt` in `special` does not make `special.sqrt`
a name of the standard library. Each library name has one home, the file that defines it.

A program's own modules have no such limit: as in Python, a name one module imports can be
imported from it again.

## Options

### A. The re-export idiom, in standard-library files only (chosen)

An import in a standard-library file written as `from special_gamma import gamma as gamma`, a
member imported under its own name with `as`, makes that name a name of the importing module.
Every other import stays the file's own, as before.

```python
# <std>/special.byn
from special_gamma import gamma as gamma, gammaln as gammaln
from special_error import erf as erf
```

- It is Python's own spelling. Type checkers read `import x as x` and `from m import x as x` as
  an explicit re-export ([PEP 484](https://peps.python.org/pep-0484/#stub-files)), and a reader
  of Python knows it.
- A library file's ordinary imports never leak: `from math import sqrt` in `special_gamma`,
  or `from figure import Figure` in `byplot`, adds nothing to the module a program sees.
- One name still has one home, the file that defines it, so errors, panics and symbols name the
  file the code is in.
- Cost: a public module lists every name it re-exports once more, one `as` per name.

### B. Keep each module in one file

- No new rule.
- `special` reaches about 3,900 lines, eight times the guide, and `linalg` stays past it too.

### C. A wrapper in the public module for each function

```python
def gamma(x: f64) -> f64:
    """..."""
    return special_gamma.gamma(x)
```

- No new rule.
- About 110 wrappers for `special`, each with a second copy of its documentation, which drifts.

## Decision

**A** (Collins, 2026-11-14, card 34). In a standard-library file, and only there, an import of
the form `from module import name as name` re-exports `name`: a program importing the file's
module finds the definition the import names, wherever that is. Only this form counts: a plain
`from module import name`, an `as` with another name, and `import module as module` stay the
file's own. A name starting with `_` is never re-exported.

## Consequences

- `special` becomes one public file of re-exports and internal files of a family each:
  `special_gamma`, `special_beta`, `special_error`, `special_bessel_jy`, `special_bessel_ik`
  and the shared `special_common`. The reference's
  [Files under `special`](../reference/special-functions.md#files-under-special) lists them.
- The same rule covers idea I.50's `linalg`: moving `LinAlgError` and the LU functions into an
  internal file with a re-export resolves, type-checks and runs `linalg`'s tests. Only the file
  named in a panic's location and in a function's symbol changes.
- A program's own modules are unchanged.
- Re-exports chain, up to the resolver's limit of 16 imports, so a circle of re-exports stops
  the search instead of looping.
