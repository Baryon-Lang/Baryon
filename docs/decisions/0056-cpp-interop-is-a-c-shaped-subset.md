# 0056. C++ interop is a C-shaped subset

Status: Accepted
Date: 2026-11-13
Roadmap: D.5, 4.1

## Context

Row 4.1 planned full C++ interop: importing headers, instantiating templates and mapping
exceptions at the boundary, 12 weeks or more. Most numerical C++ libraries also expose a C API,
and the project puts native Baryon libraries first and all interop after AstroBaryon. The choice
was between full interop, a C-shaped subset, and none.

## Decision

C++ interop is a C-shaped subset: Baryon calls `extern "C"` functions and plain structs from C++
libraries, reusing the C interop. No templates and no exceptions cross the boundary. It still
comes after AstroBaryon, as all interop does. Collins chose this on 2026-11-13.

## Consequences

- Row 4.1 shrinks from 12 weeks or more to a few weeks on top of C interop.
- A C++ library with no C API needs a thin `extern "C"` wrapper written in C++.
- Full interop can be added later under a new record without breaking code that uses the subset.
