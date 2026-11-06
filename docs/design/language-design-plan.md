# Building a New Compiled Language: Technical Plan

Nov 4, 2026 · Collins Simpson

## Summary and recommended approach

The language exists to write physics simulations and mathematical models, the work done today in Fortran, C++, and Rust, with the syntax and simplicity of Python. Every design choice below is judged against that: does it make a molecular dynamics code, a PDE solver, or a Monte Carlo model faster to write, faster to run, or harder to get wrong. Interoperability with other languages is deliberately deferred to Phase 4 so that nothing in the core is shaped by someone else's ABI.

Build the MVP as a Rust-hosted compiler with a query-based (demand-driven) front end, an owned mid-level IR, and LLVM as the first code generator; add a fast debug backend (Cranelift) in Phase 2 so compile speed and runtime speed are never traded against each other. The language itself should take Rust's ownership and RAII, Fortran's array semantics and no-alias guarantees, and Python's syntax wholesale (def, indentation, f-strings, comprehensions, match), departing from Python only where speed or safety leaves no choice, and drop lifetime annotations entirely by limiting references in the MVP.

The single biggest risk is scope. A Rust-grade borrow checker, a C-grade optimizer, and a Python-grade syntax each took their original teams years. The plan below reaches a usable MVP (compile and run numeric programs, run a benchmark suite, open in VS Code with diagnostics and completion) in roughly 12 to 14 person-months by making three deliberate cuts:

1. References cannot be stored in data structures in the MVP ("second-class references"). This removes lifetime inference, the hardest part of Rust, while keeping memory safety.
2. No user-defined macros, no trait objects, no async until after the MVP. Each one roughly doubles front-end complexity.
3. The IDE is a language server, not a standalone editor. A standalone IDE is a Phase 4 decision, and it may never be the right call.

Compiler speed is treated as a product requirement with numeric targets (see Compiler architecture), not an optimization pass to do later. The architectural choices that make a compiler fast (parallel per-module front end, arena allocation, incremental query caching, a decidable type system) cannot be retrofitted; they are made in Phase 0.

## Design goals, non-goals, and positioning

The target niche is numeric, scientific, and analytics code where the author today writes C or Fortran for the kernels and Python (or IDL/GDL) for loading, exploring, and plotting the data around them. The language must serve both halves: a compiled kernel language in the Fortran and Carbon tradition, and an interactive data language in the Python and GDL tradition, with no boundary between them. The language must beat C on array code (by exploiting no-alias semantics the way Fortran does), match Rust on memory safety for the common case, and read like Python.

| Goal | Measurable target for MVP | Taken from |
| --- | --- | --- |
| Runtime speed on numeric kernels | Within 5% of gfortran -O3 and clang++ -O3 on a physics kernel suite: N-body, Lennard-Jones molecular dynamics, 3D heat-equation stencil, conjugate gradient on a sparse matrix, RK4 on a stiff ODE system, Monte Carlo integration, dense matmul, FFT, measured on Apple M-series (NEON, 128-bit vectors); re-measured on AVX-512 hardware in Phase 2 | Fortran |
| Memory safety without GC | No use-after-free or data race in safe code; no runtime cost beyond bounds checks (removable in release) | Rust |
| Readability | A Python programmer reads a 200-line program and explains it without the manual | Python |
| Compile speed, debug build | 100k lines/sec end to end on an 8-core laptop; incremental rebuild of a one-function edit under 200 ms | Zig, Go |
| Compile speed, release build | Within 1.5x of clang -O2 wall time on the same program | C |
| Interop | Not an MVP goal. User-facing interop with C, C++, and Python is Phase 4; the standard library's own calls into libm, BLAS, and LAPACK are internal and invisible | C |

A seventh goal, added for the analytics half, does not fit the MVP gate but shapes what the MVP must not preclude: a data scientist loads a CSV or HDF5 file, filters and groups it, fits a model, and plots the result in under 30 lines, in a REPL, with the same language and the same compiler that produced the kernel. Phase 3 is sized to that goal.

Non-goals for the MVP, each with the reason:

- Garbage collection or reference counting as the default memory model. It conflicts with predictable latency and with Fortran-class performance.
- Lifetime annotations. The reference model in the next section avoids them.
- Macros, metaprogramming, and compile-time code execution beyond constant folding. They cost front-end speed and tooling simplicity, which are both primary goals.
- Async, coroutines, green threads. Parallelism in the MVP is data parallelism (parallel loops) plus plain OS threads.
- A standalone IDE. The LSP server gives 90% of the value in 20% of the work.
- Self-hosting. Write the compiler in Rust; revisit self-hosting only once the language is stable, since a bootstrapped compiler locks the language against breaking changes.
- User-facing foreign function interface of any kind (C, C++, Python). Deferred to Phase 4 so the core types, calling convention, and array layout are designed for simulation code, not for compatibility. The standard library may call libm and BLAS internally; that mechanism is not exposed.

Where each reference language falls short, and what this one does differently:

- Rust: lifetimes and the trait solver are the main source of its learning curve and its compile times. Dropping both for the MVP is the central bet.
- C: aliasing rules defeat vectorization; `restrict` is rarely used. Here, references never alias by construction, so the optimizer always knows.
- Fortran: strong array model, weak everything else (strings, generics, modules, tooling). Keep the array model, replace the rest.
- Python: readable but dynamically typed and slow. Keep the syntax itself, not just its shape; make every type static, with Python's own annotation syntax on signatures and inference inside bodies.
- GDL/IDL: the array-first data language astronomers and geoscientists use for analysis; whole-array operations, broadcasting, and built-in plotting and file formats are its strengths, interpretation speed and a dated type system its weaknesses. Keep the array-first style and the batteries; compile it.
- Carbon: Google's C++ successor, which shows that a new low-level language is only adopted if it interoperates with the existing codebase. Take the lesson for Phase 4: when interop arrives, it must be first-class, not bolted on. Until then the language stands on its own.

## Language design

The design decisions below are the ones that shape the compiler. Each is a recommendation with the trade-off stated; the open ones are listed in the last section.

### Syntax

The rule: if Python has a spelling for it, use Python's spelling. Significant indentation, a trailing colon, `#` comments, `def`, `elif`, `and`/`or`/`not`, `True`/`False`/`None`, `for x in range(n)`, `print()`, f-strings, `import x` and `from x import y`, snake\_case names, `a[i, j]` indexing and `a[1:3]` slicing. The lexer emits INDENT and DEDENT tokens so the parser stays a plain recursive-descent parser. A Python programmer should be able to read a file and guess every construct; the only new things to learn are the type annotations, which use Python's own `x: int` and `-> float` syntax, and the handful of keywords below that Python has no equivalent for.

A sketch of what a kernel looks like:

```markdown
def saxpy(a: f64, x: Array[f64], y: inout Array[f64]):
    for i in range(len(x)):
        y[i] = a * x[i] + y[i]

def mean(x: Array[f64]) -> f64:
    total = 0.0
    for v in x:
        total += v
    return total / len(x)

def main():
    x = Array.fill(1_000_000, 1.0)
    y = Array.fill(1_000_000, 2.0)
    saxpy(2.0, x, y)
    print(f"y[0] = {y[0]}, mean = {mean(y):.3f}")
```

What is kept from Python, and the few places the language has to depart from it to get Rust's speed and safety:

| Construct | Python | This language | Why it differs, if it does |
| --- | --- | --- | --- |
| Function | `def f(x: int) -> int:` | same | Annotations are required on signatures, optional inside bodies |
| Local variable | `x = 1` | same; mutable by default, type inferred | No `let`/`var`; Python users expect assignment to just work |
| Constant | `X = 1` by convention | `const X = 1` | Needed for compile-time values (array sizes, SIMD widths) |
| Record type | `@dataclass class P:` | `struct P:` with fields `x: f64` | Value semantics, not reference semantics; `class` would mislead Python users about copying |
| Methods | `def m(self):` inside the class | same, inside the `struct` |  |
| Mutating a parameter | implicit (lists mutate) | `y: inout Array[f64]` at the signature, `saxpy(2.0, x, y)` at the call | The one unavoidable departure: the exclusivity check needs to know which arguments can change |
| Optional | `x: int \| None`, `if x is None:` | same spelling; `T \| None` is `Option[T]` | `is None` compiles to a match, no runtime cost |
| Errors | exceptions | `Result[T, E]` returned, `?` to propagate, `raise` only for unrecoverable panics | Exceptions cost speed and hide control flow; `raise` keeps the word for panics |
| Pattern matching | `match x: case ...` | same, exhaustive | Compiler rejects a missing case |
| Loops | `for`, `while`, `range`, `enumerate`, `zip` | same, plus `parallel for` | `range` is zero-cost, not a list |
| Comprehensions | `[f(v) for v in xs]` | same, produce an `Array` and fuse into one loop |  |
| Generics | `def f[T](x: T)` (3.12) | same | Monomorphized |
| Modules | `import numpy as np` | same, no `__init__.py`, directory is package |  |
| Strings | `str`, f-strings | `str` (UTF-8, owned), f-strings |  |
| Integers | arbitrary precision | `int` is `i64`, `float` is `f64`; sized types `i32`, `u8`, `f32` exist | Fixed width is what makes it fast; `int` and `float` keep the Python names as the defaults |
| Memory | garbage collected | ownership, moves, RAII, invisible in most code | Values are moved on assignment; the compiler says when a moved value is reused |
| Unsafe | not applicable | `unsafe:` block |  |

The two things a Python programmer will trip on, in order: that assigning a struct or array to a new name moves it (use `.copy()` for a copy, as in NumPy), and that `inout` must be written at the signature. Both produce a one-line error with a fix-it in the IDE. Everything else should read as Python with types.

### Type system

- Primitives with explicit widths: `i8..i64`, `u8..u64`, `f32`, `f64`, `bool`, `char`. No implicit widening between numeric types; `as` converts.
- `struct` with value semantics by default, `enum` with payloads (sum types) and exhaustive `match`.
- Generics with monomorphization (one copy of the code per concrete type, as in Rust and C++ templates). Constraints are expressed as `trait` bounds, but the MVP supports only static dispatch. Trait objects (dynamic dispatch) come in Phase 3.
- Local type inference only (Hindley-Milner inside a function body, nothing across functions). This is decidable, fast, and gives good error messages.
- No implicit nulls. `Option[T]` and `Result[T, E]` as in Rust, with `?` for early return.
- Tuples, fixed-size arrays `[T; N]`, slices, and a built-in multidimensional `Array[T]` type (see Numerics).

### Memory model and references

Values are owned, moved by default, and dropped at scope exit (RAII). Copy is opt-in per type. This is the Rust model minus lifetimes.

References in the MVP are second-class: a function parameter may be `ref T` (shared, read-only) or `inout T` (exclusive, mutable), and a reference can be passed on to another call, but it cannot be stored in a struct, returned, or captured. With that restriction, every reference is lexically scoped to a call and the compiler needs no lifetime annotations and no lifetime inference. Exclusivity (one `inout` or many `ref`, never both) is checked per call site by a simple path-overlap analysis. This is the model Swift used before `~Copyable` and the one Hylo is built on; it covers the large majority of numeric and systems code.

What it does not cover, and the escape hatches:

- Data structures that hold references (an iterator over a slice, a view into a buffer). MVP answer: index-based views (`Array` slices carry offset and stride, not pointers), and `unsafe` raw pointers for the rest. Phase 3 answer: a limited lifetime system or explicit regions, decided after seeing what real programs need.
- Shared mutable state across threads. MVP answer: `Mutex[T]` and atomics in the standard library, plus a `parallel for` whose body may only capture `ref` or disjoint `inout` slices, checked statically.

`unsafe` blocks allow raw pointer arithmetic, calls to C, and unchecked indexing. The rule is the same as Rust: unsafe code is the author's responsibility, but it must be marked.

### Numerics (the Fortran part)

- `Array[T, N]` is a first-class N-dimensional array with shape and strides known to the compiler. Layout is column-major by default (BLAS and LAPACK compatible) with a `row_major` attribute for C interop.
- Array expressions: `c = a * b + 2.0` on whole arrays compiles to a single fused loop (no temporaries), because the compiler sees the expression tree.
- No aliasing between `inout` parameters is guaranteed by the reference rules, so every loop over distinct arrays is vectorizable without runtime checks.
- `parallel for` and `simd for` loop attributes; the latter asserts no loop-carried dependence.
- Fast-math is per-scope: `with fastmath:` inside a block, never a global compiler flag that silently changes results.
- Bounds checks on in debug, off in release, overridable per array access with `[i]!`.
- Integer overflow traps in debug, wraps in release unless the type is `i64.checked`.

### Physics simulation and mathematical modeling (the core purpose)

This is what the language is for, so these are language features and compiler guarantees, not library conveniences. Each item names what Fortran, C++, or Rust users do today and what replaces it.

| Need in simulation code | Today | Here, from the MVP |
| --- | --- | --- |
| Small fixed-size vectors and matrices (positions, forces, stress tensors) | hand-written `Vec3` structs in C++, `real(8), dimension(3)` in Fortran | `Array[f64, (3,)]` and `Array[f64, (3, 3)]` with the shape in the type; stored inline, no heap, operations unrolled by the compiler |
| Large grids and particle arrays | heap arrays, manual indexing | `Array[f64, (N, M, L)]` with runtime shape, column-major, strides known; `a[i, j, k]` bounds-checked in debug, free in release |
| Complex numbers | `std::complex`, Fortran `complex` | `complex` and `complex64` as primitives with literal syntax `1 + 2j`, SIMD-friendly layout |
| Whole-array expressions without temporaries | Fortran array syntax, Eigen expression templates | array expressions fuse into one loop nest at the MIR level, guaranteed, with a compiler remark if fusion fails |
| Stencils | explicit loops with halo handling | `stencil` loop form: `for cell in grid.interior(halo=1): out[cell] = ...` with neighbor access `grid[cell + (1, 0, 0)]`, bounds proven once |
| No aliasing between arrays | Fortran assumes it; C needs `restrict`; Rust proves it with borrows | proven by the reference rules; every loop gets `noalias` |
| Vectorization | compiler hopes, programmer checks assembly | `simd for` asserts independence; the IDE shows which loops vectorized |
| Shared-memory parallelism | OpenMP pragmas | `parallel for` with static checking that the body only writes disjoint slices |
| Reductions in parallel | OpenMP `reduction(+:x)` | `parallel for ... reduce(total: +)`, deterministic order option |
| Floating-point control | `-ffast-math` globally, `-O3` surprises | `with fastmath:` per block; otherwise strict IEEE; `fma()` explicit; reproducible results are the default |
| Random numbers | per-library RNGs | `random` module with counter-based generators (Philox) so parallel streams are reproducible from one seed |
| Units and dimensions | comments, or Boost.Units | Phase 3: `Quantity[f64, m/s]` checked at compile time, zero cost at runtime |
| Sparse matrices and iterative solvers | PETSc, Eigen, hand-rolled CSR | Phase 3: `SparseMatrix` (CSR, COO), conjugate gradient, GMRES in `linalg` |
| ODE and PDE time stepping | hand-written RK4, library calls | Phase 3: `ode` module with explicit and implicit steppers that accept a user function and inline it |
| Automatic differentiation | Stan, Enzyme, hand-coded Jacobians | Phase 4 candidate: forward and reverse mode on the MIR; the MIR design must not preclude it |
| Multi-node parallelism | MPI | Phase 4: distributed arrays; the shape-and-stride design is what makes it possible later |
| GPU | CUDA, OpenACC | Phase 4: `parallel for` offload through an LLVM GPU target; same loop form, different backend |

The MVP rows are language and compiler work and are in the Phase 1 estimates. The Phase 3 and 4 rows are libraries or backends on top of a core designed to accept them; the design rule is that none of them should need a new language feature.

### Modules and interop

- One file is one module; directories are packages. No header files, no forward declarations.
- No foreign function interface before Phase 4. The compiler has an internal, unexposed way to call C symbols so the standard library can reach libm, BLAS, and LAPACK; it is not part of the language.
- Build is driven by a single manifest file; the compiler is also the build tool (as in Go, Zig, and Cargo).

### Data science and analytics (the Python and GDL part)

The compiled core above is what makes analytics fast; this subsection is what makes it convenient. Everything here is library and tooling on top of the language, not new language features, which is why it can follow the MVP without reopening the compiler.

- Broadcasting: elementwise operations between arrays of compatible shape follow NumPy's rules and fuse into one loop. `a + b[:, None]` means what a NumPy user expects, and compiles to a single nested loop with no temporary.
- Reductions and axis arguments: `x.sum(axis=0)`, `x.mean()`, `x.argmax()`, with the axis resolved at compile time so the loop nest is fixed.
- `DataFrame`: a column-oriented table whose columns are typed arrays, with `filter`, `group_by`, `join`, `sort`, and `select`. Columns are known types, so a `group_by(...).mean()` compiles to a typed loop rather than a dynamic dispatch per cell.
- Missing data: `float | None` columns are stored as a value array plus a validity bitmap (the Arrow layout), so `None` costs one bit, not a boxed object.
- File formats: CSV, Parquet, HDF5, NetCDF, and FITS readers and writers, because those are what the GDL and Python users already have on disk. Arrow in memory means Parquet is nearly free.
- Plotting: a `plot` module with the GDL and matplotlib verbs (`plot`, `scatter`, `imshow`, `contour`, `hist`) rendering to PNG, SVG, or an interactive window.
- Interactive use: a REPL that compiles each input with the Cranelift backend and keeps live values across inputs, and a Jupyter kernel on top of it so notebooks work from day one of Phase 3. A compiled language needs the REPL to feel like Python; without it the analytics half is not credible.
- Python interop, Phase 4 and not before: call Python from the language for the libraries that will never be rewritten (scikit-learn, specific plotting), and expose language functions to Python as native extensions. Arrays cross the boundary with zero copy through the buffer protocol.

None of this is in the MVP, but two MVP decisions protect it: array layout and strides are a first-class, stable part of the `Array` type so that NumPy and Arrow buffers can be viewed without copying, and `None` on a value type is a real `Option` from the start, not a sentinel.

## Compiler architecture

The compiler is one Rust binary that does parsing, checking, optimization, code generation, linking, and build orchestration, with every stage after parsing exposed as cached queries so that the language server and the batch compiler share one engine.

_(Diagram in the live doc: compiler pipeline, 12 stages, 2 backends. Flow: lex+parse → resolve names → type check → reference check → lower to MIR → monomorphize → MIR passes → pick backend → Cranelift (debug) or LLVM (release) → link with lld. Everything through MIR lowering runs inside the incremental query engine, which the language server also reads.)_

Everything above the backend split runs inside the cached query engine, so the language server answers from the same results the batch compiler produces; only MIR lowering and below differ between a debug and a release build.

### Implementation language and dependencies

Write the compiler in Rust. The reasons are practical: memory safety in a program that manipulates deeply shared trees all day, `rayon` for parallelism, and mature crates for the pieces that are not worth re-inventing. Dependencies to take, and the ones to refuse:

| Component | Decision | Why |
| --- | --- | --- |
| Lexer and parser | Hand-written | Generated parsers give worse errors and worse recovery; a hand-written recursive-descent parser is 2 to 3 weeks of work and is the fastest option |
| Incremental query engine | `salsa` crate (what rust-analyzer uses) or a thin clone of it | Memoized, dependency-tracked queries are what make the IDE fast and incremental builds correct |
| Release backend | LLVM via `inkwell` | Best optimizer available; vectorizer, LTO, PGO, every target |
| Debug backend | Cranelift (Phase 2) | Generates code 10 to 20x faster than LLVM; this is how Rust got fast debug builds |
| Linker | `lld` bundled, `mold` optional | Linking is often half of a debug build's wall time with the system linker |
| Target triples, object writing | LLVM and Cranelift provide these | No reason to own them |
| String interning, arenas | `bumpalo`, own interner | Core to compile speed; must be owned and tuned |

### Front end

Lexer: a single pass producing a flat token array with byte offsets, no heap allocation per token. Indentation tracked with a stack. Target: 50 MB/s single-threaded.

Parser: recursive descent with Pratt parsing for expressions. Produces a concrete syntax tree stored as a flat arena (node = 16 bytes: kind, start, len, first-child index) rather than a tree of boxed nodes. The CST keeps all tokens including comments and whitespace, so the formatter and the IDE work from the same tree. Error recovery is at statement and declaration boundaries, so one typo yields one error. Every file parses independently and in parallel.

Name resolution and type checking are queries keyed on a declaration id: `signature_of(fn_id)`, `type_check(fn_id)`, `layout_of(struct_id)`. Because signatures are mandatory, checking function A never needs function B's body, only its signature, which is the property that makes whole-program checking embarrassingly parallel. A change inside one function body invalidates exactly the queries that depend on that body, which is just its own `type_check` and the code generated from it.

The reference and exclusivity check is one query per function on the typed IR, a flow-insensitive overlap check over access paths. It is deliberately simpler than Rust's borrow checker (no NLL, no two-phase borrows) because second-class references make those unnecessary.

### Middle end: the IR

Lower the typed tree to a mid-level IR (MIR) in static single assignment form with explicit control flow, explicit drops, and explicit bounds checks. This is where the language-specific optimizations live, before anything is handed to a backend:

- Array expression fusion: an expression tree over arrays becomes one loop nest.
- Drop elision and move forwarding.
- Bounds-check hoisting out of loops when the index range is provably within the shape.
- Inlining of small generic instantiations after monomorphization.
- Attaching `noalias`, `nonnull`, and `dereferenceable` metadata derived from the reference rules, so LLVM gets the same information Fortran compilers get for free.

Monomorphization happens at this layer. To control code bloat and compile time, instantiations are deduplicated by their post-lowering MIR hash, so `Vec[i32]` and `Vec[u32]` can share code when layout and operations are identical.

### Back ends

Release: MIR lowers to LLVM IR one function at a time, inside a `rayon` parallel map, into one LLVM module per source module. The default is `-O2` with the loop vectorizer on; `-O3` adds aggressive inlining and unrolling; `--lto` runs thin LTO. Fast-math flags are emitted only inside `with fastmath:` scopes.

Debug (Phase 2): the same MIR lowers to Cranelift. No optimization beyond register allocation. This is the backend the language server's "run tests" button and the edit-compile-run loop use.

The two backends are the reason the MIR must be rich enough to carry all semantic information; neither backend should ever need to look at the syntax tree.

### Compile-speed targets and how each is met

| Target | Mechanism |
| --- | --- |
| Parse at 1M lines/sec across 8 cores | Flat arena CST, no per-node allocation, one file per task |
| Type check at 300k lines/sec across 8 cores | Per-function queries, mandatory signatures, local inference only, no trait-object resolution in the MVP |
| Debug build 100k lines/sec end to end | Cranelift backend, parallel codegen, bundled `lld` |
| Incremental one-function edit under 200 ms | Salsa-style query invalidation; only the edited function re-checks and re-codegens; object files patched, not rebuilt |
| Release build within 1.5x of clang -O2 | Fewer, larger LLVM modules; dedup of monomorphized instances; `-O2` default not `-O3` |
| Cold start of the language server under 1 s on 100k lines | Parse and resolve only; type check lazily on open files |

Two design rules protect these numbers. First, no stage may be global: anything that needs the whole program (dead code elimination, LTO) is a release-only pass that runs after per-module work. Second, every compiler run emits timing per query in `--timings` from the first week, and the benchmark suite in CI fails if any number regresses more than 10%.

### Diagnostics

Error messages are a product feature, not an afterthought. Every diagnostic carries a primary span, zero or more secondary spans with labels, and a suggested fix where one exists. The IDE applies the fix with one click. The reference checker in particular must explain exclusivity violations in terms of the two access paths that overlap, not in terms of compiler internals.

## Runtime, standard library, and tooling

The runtime is as small as C's: no garbage collector, no scheduler, no reflection. A statically linked binary of `main` printing a number should be under 100 KB.

Runtime components, all written in the language itself once the MVP compiles, in Rust or C before then:

- Allocator shim over the system `malloc` with an override hook; arena and pool allocators in the standard library.
- Panic handler with backtraces in debug builds; abort in release.
- Bounds-check and overflow-trap intrinsics.
- Thread pool for `parallel for`, sized to hardware threads, started lazily on first use.

Standard library scope for the MVP, kept deliberately narrow so the compiler team is not also a library team:

| Module | Contents |
| --- | --- |
| `core` | integers, floats, bool, char, Option, Result, tuples, slices, iteration protocol |
| `array` | N-dimensional `Array`, views, elementwise ops, reductions, `matmul` via Apple Accelerate on macOS, OpenBLAS elsewhere |
| `collections` | `Vec`, `HashMap`, `String` (UTF-8) |
| `io` | stdin, stdout, files, `print` with format strings |
| `math` | libm bindings, `sqrt`, `exp`, trig, `fma` |
| `thread` | `spawn`, `join`, `Mutex`, atomics |
| `ffi` | C type aliases, raw pointers, `CString` |

Added in Phase 3 for the analytics half:

| Module | Contents |
| --- | --- |
| `array` (extended) | broadcasting, axis reductions, `linalg` (solve, eig, svd via LAPACK), `fft` |
| `data` | `DataFrame`, `Series`, group\_by, join, Arrow memory layout |
| `io.formats` | CSV, Parquet, HDF5, NetCDF, FITS |
| `plot` | plot, scatter, imshow, contour, hist; PNG, SVG, interactive window |
| `stats` | descriptive statistics, distributions, least squares, curve fitting |
| `py` | Phase 4 only: call Python, expose to Python, zero-copy array exchange |
| `repl` | the interactive compiler loop and the Jupyter kernel |

Tooling that ships with the compiler binary (one executable, subcommands, as in Go and Zig):

- `build`, `run`, `test` with a built-in test runner (`#[test]` functions).
- `fmt`, a formatter driven from the CST so it is lossless and fast. It exists from the MVP because a Python-style language without an enforced formatter fragments immediately.
- `check`, type-check only, which is what editors call on save.
- `bench`, which runs functions marked `#[bench]` and reports against a stored baseline.
- `doc`, HTML from doc comments, Phase 3.
- Package manifest and lockfile; a registry is Phase 4 and may be unnecessary if git dependencies suffice.

Testing the compiler itself needs four suites from the first month: a parser corpus with expected CSTs, a type-checker corpus with expected diagnostics (snapshot tests), an execution corpus (compile, run, compare stdout), and the numeric benchmark suite with stored baselines for both runtime and compile time. Fuzzing the parser with a grammar-based generator should start as soon as the parser accepts the full grammar; parsers that have never been fuzzed crash in the IDE on half-typed code.

## IDE and developer experience

Build a language server first, a VS Code extension second, and decide on a standalone IDE only after the language has users. The language server is the compiler's query engine behind the Language Server Protocol, so every feature is free once the compiler query exists; a standalone IDE is a multi-year product with none of that leverage.

### What the language server provides, by phase

| Feature | Phase | Compiler query it uses |
| --- | --- | --- |
| Syntax highlighting (semantic tokens) | MVP | parse |
| Diagnostics as you type | MVP | type\_check, reference\_check |
| Go to definition, find references | MVP | resolve |
| Hover with inferred type | MVP | type\_of(expr) |
| Completion of names and members | MVP | resolve, type\_of |
| One-click fixes from diagnostics | MVP | diagnostics carry edits |
| Format on save | MVP | fmt over CST |
| Rename across files | Phase 2 | find references |
| Inlay hints (inferred types, parameter names) | Phase 2 | type\_of |
| Run and debug a test from the gutter | Phase 2 | Cranelift build + DAP |
| Show vectorization and inlining decisions inline | Phase 3 | MIR pass remarks |
| Show which loops fused, with estimated cost | Phase 3 | fusion pass |
| Shape and bounds hints for arrays | Phase 3 | shape inference |

The Phase 3 rows are where this language's IDE can be better than Rust's or Python's: because the compiler knows array shapes and aliasing statically, the editor can tell the author which loops vectorized and why one did not, which is the question every numeric programmer asks and today answers by reading assembly.

### What speeds up development, in order of payoff

1. Sub-second diagnostics. The query engine with mandatory signatures makes this possible; re-checking one edited function is milliseconds.
2. A fast debug backend with a run button. Edit, run, see output in under a second for a 50k-line project is the loop that makes a compiled language feel like Python.
3. Debug info that works. DWARF emitted from the MVP so `lldb` and the VS Code debugger show the author's variables, not mangled names. This is often postponed and should not be.
4. A tree-sitter grammar, written in the first month, so GitHub, Neovim, Zed and Helix highlight the language before the language server exists.
5. Error messages with fixes, as described under Diagnostics.

### Standalone IDE: when and whether

Revisit after Phase 3 with real users. The case for one is weak unless the language needs an interaction that editors cannot host: a notebook-style REPL for numeric exploration, live plots of arrays, or a visual profiler tied to source. If that case appears, the cheapest route is a VS Code fork or a Zed extension, not a new editor. Budget nothing for it in the MVP.

## MVP definition and phased roadmap

The MVP is done when a physicist can write a 2,000-line simulation (a Lennard-Jones molecular dynamics code and a 3D heat-equation solver are the reference programs) in the language, build it in both modes, run it within 5% of the Fortran version, and edit it in VS Code with live diagnostics. Nothing else is in the MVP.

_(Diagram in the live doc: roadmap. Phase 0 Spec, 6 weeks, gate: programs frozen → Phase 1 MVP, 5 to 6 months, gate: sim runs within 5% of Fortran → Phase 2 Speed, 2 to 3 months, gate: speed table met → Phase 3 Optimize, 4 to 6 months, gate: 3 external users → Phase 4 Ecosystem, ongoing.)_

Phase 1 is the accent because it is the only phase where scope is fixed; every later phase is sized by what the gate before it revealed.

Phase 0, Specification (6 weeks). Write the language reference as a short document: grammar in EBNF, type rules, reference rules, array semantics, C ABI. Write 20 example programs in the language before any compiler exists and iterate on the syntax until they read well. Build the benchmark suite in C and Fortran so there is a baseline to beat. Gate: the 20 programs are frozen and the reference check rules are written as a decision procedure on paper.

Phase 1, MVP compiler (5 to 6 months). Lexer, parser, CST, query engine, name resolution, type checker, reference checker, MIR, LLVM backend, `lld`, the `core` and `array` modules, `build`/`run`/`test`/`fmt`, DWARF, the language server with the MVP rows, the VS Code extension. Gate: the MVP definition above, plus all four test suites green in CI.

Phase 2, Compile speed and loop (2 to 3 months). Linux x86-64 target (ELF, lld, AVX validation of the vectorizer), Cranelift backend, incremental object patching, parallel codegen tuning, `--timings`, compile-speed targets met and enforced in CI, rename and inlay hints, test runner in the gutter. Gate: every number in the compile-speed table met on the reference project.

Phase 3, Optimization and expressiveness (4 to 6 months). MIR fusion and bounds hoisting, noalias metadata audit against the vectorizer, thin LTO, PGO, trait objects, a decision on first-class references or regions based on real programs, `doc`, the IDE's optimization-remark views, and the analytics track: broadcasting, REPL and Jupyter kernel, DataFrame, file formats, plotting, Python interop. Gate: within 5% of gfortran on all kernels, the 30-line load-group-fit-plot script runs in the REPL, and three external users have written something real.

Phase 4, Ecosystem and interop (ongoing). All foreign function interfaces (C, C++, Python) land here and nowhere earlier; automatic differentiation, distributed arrays, and GPU offload are the other candidates. Package registry if needed, Windows and Linux ARM (Graviton, SVE) targets, self-hosting decision, standalone IDE decision, async decision.

Assumed team: two to three engineers, one of whom has written a compiler before. A solo effort roughly doubles the calendar time of each phase and should cut the MVP scope further, most likely by dropping the language server to Phase 2.

## Work breakdown structure

Estimates are engineer-weeks for someone who has built at least one compiler component before, and assume the Rust crate choices above. Totals: Phase 0 about 6 weeks, Phase 1 about 46 to 54 weeks of effort (5 to 6 calendar months with two to three people), Phase 2 about 12 weeks, Phase 3 about 24 weeks.

| ID | Work package | Phase | Weeks | Depends on | Output |
| --- | --- | --- | --- | --- | --- |
| 0.1 | Language reference: grammar (EBNF), type rules, reference rules, array semantics | 0 | 3 |  | reference.md |
| 0.2 | 20 example programs, iterated for readability | 0 | 2 | 0.1 | examples/ |
| 0.3 | Physics kernel suite in Fortran and C++ with harness: N-body, LJ molecular dynamics, 3D stencil, CG on sparse, RK4, Monte Carlo, matmul, FFT | 0 | 1 |  | bench/ |
| 0.4 | Repo, CI, test harness skeleton, `--timings` plumbing | 0 | 1 |  | CI green on empty compiler |
| 1.1 | Lexer with INDENT/DEDENT, token arena | 1 | 1 | 0.1 | lexer crate, fuzz target |
| 1.2 | Parser, flat CST, error recovery, parser corpus | 1 | 3 | 1.1 | parser crate |
| 1.3 | Tree-sitter grammar | 1 | 1 | 0.1 | highlighting in editors |
| 1.4 | Query engine integration (salsa), file and module database | 1 | 2 | 1.2 | incremental skeleton |
| 1.5 | Name resolution, modules, imports | 1 | 2 | 1.4 | resolve queries |
| 1.6 | Type representation, interning, layout computation, compile-time shape parameters, \`complex\` primitives | 1 | 2 | 1.5 | types crate |
| 1.7 | Type checker: local inference, generics, traits with static dispatch, enums, match | 1 | 6 | 1.6 | type\_check query, diagnostic corpus |
| 1.8 | Reference and exclusivity checker | 1 | 3 | 1.7 | reference\_check query |
| 1.9 | MIR definition, lowering from typed tree, drops, monomorphization | 1 | 5 | 1.7 | mir crate |
| 1.10 | LLVM backend via inkwell, noalias metadata, DWARF, Apple AArch64 ABI, Mach-O output | 1 | 6 | 1.9 | object files |
| 1.11 | Driver: `build`, `run`, `test`, manifest, Mach-O linking via ld64 or lld | 1 | 3 | 1.10 | one binary |
| 1.12 | Runtime: allocator, panic, traps, thread pool | 1 | 2 | 1.10 | runtime lib |
| 1.13 | `core` and `collections` modules | 1 | 3 | 1.11 | stdlib |
| 1.14 | `array` module: fixed-shape inline arrays, runtime-shape arrays, views, stencil and simd-for loop forms, parallel-for with reduce, Philox RNG, fusion-ready ops, internal BLAS binding | 1 | 5 | 1.13 | stdlib |
| 1.15 | `fmt` from CST | 1 | 2 | 1.2 | formatter |
| 1.16 | Language server: diagnostics, goto, hover, completion, fixes | 1 | 4 | 1.8 | lsp binary |
| 1.17 | VS Code extension | 1 | 1 | 1.16 | marketplace package |
| 1.18 | Execution corpus, benchmark baselines, fuzzing in CI | 1 | 2 | 1.11 | CI gates |
| 2.1 | Cranelift backend from MIR | 2 | 4 | 1.9 | debug backend |
| 2.2 | Incremental codegen and object patching | 2 | 3 | 2.1 | sub-second rebuilds |
| 2.3 | Compile-speed profiling and tuning to targets | 2 | 3 | 2.2 | speed table met |
| 2.4 | Rename, inlay hints, test runner in gutter, DAP hookup | 2 | 2 | 1.16 | IDE Phase 2 rows |
| 3.1 | Array expression fusion pass | 3 | 4 | 1.14 | fused loops |
| 3.2 | Bounds-check hoisting, shape inference | 3 | 3 | 3.1 | fewer checks |
| 3.3 | Vectorizer audit against gfortran on each kernel | 3 | 3 | 3.1 | 5% gate |
| 3.4 | Thin LTO, PGO, instantiation dedup | 3 | 3 | 1.10 | release build options |
| 3.5 | Trait objects, dynamic dispatch | 3 | 4 | 1.7 | expressiveness |
| 3.6 | First-class references or regions: design and prototype | 3 | 4 | 1.8 | decision document |
| 3.7 | Optimization remarks in the IDE | 3 | 2 | 3.1 | IDE Phase 3 rows |
| 3.8 | `doc` tool | 3 | 1 | 1.2 | HTML docs |

Added for the analytics half, all Phase 3 unless noted; the first two run in parallel with the optimization work above since they are library, not compiler:

| ID | Work package | Phase | Weeks | Depends on | Output |
| --- | --- | --- | --- | --- | --- |
| 3.9 | Broadcasting and axis reductions, fused with 3.1 | 3 | 3 | 3.1 | NumPy-style array ops |
| 3.10 | `linalg` and `fft` over LAPACK and FFTW/Accelerate | 3 | 2 | 1.14 | solvers |
| 3.11 | REPL on Cranelift with persistent values | 3 | 4 | 2.1 | interactive loop |
| 3.12 | Jupyter kernel | 3 | 1 | 3.11 | notebooks |
| 3.13 | Arrow layout, `DataFrame`, group\_by, join | 3 | 5 | 3.9 | data module |
| 3.14 | CSV, Parquet, HDF5, NetCDF, FITS | 3 | 3 | 3.13 | io.formats |
| 3.15 | `plot` module | 3 | 4 | 3.9 | plotting |
| 3.16 | `stats` module | 3 | 2 | 3.9 | statistics |
| 3.17 | Python interop, zero-copy arrays | 4 | 3 | 1.10 | py module |
| 4.1 | C++ interop (Carbon-style): header import, templates by instantiation, exceptions at the boundary | 4 | 12+ | 3.5 | C++ bindings |

This adds about 27 weeks to Phase 3, which becomes 7 to 9 calendar months with a third engineer on the library track. If the team is two, the REPL (3.11) and DataFrame (3.13) come first and plotting and formats slip to Phase 4.

The critical path through Phase 1 is 1.1 to 1.2 to 1.4 to 1.5 to 1.6 to 1.7 to 1.9 to 1.10 to 1.11, about 28 weeks of effort that cannot be parallelized much beyond two people. The language server (1.16), the standard library (1.13, 1.14), the formatter (1.15), and the tree-sitter grammar (1.3) are off the critical path and are what a third engineer takes.

The two packages most likely to overrun are 1.7 (type checker) and 1.10 (LLVM backend). For 1.7 the overrun comes from generics and traits; the mitigation is to ship the MVP with generics but without trait bounds if the schedule slips, since monomorphization does not need them to produce correct code. For 1.10 the overrun comes from debug info and ABI details; the mitigation is to use LLVM's own C ABI lowering rather than writing one.

## Risks, open questions, and decisions needed

### Risks

| Risk | Likelihood | Impact | Mitigation |
| --- | --- | --- | --- |
| Second-class references prove too restrictive for real programs | Medium | High: forces a lifetime system into Phase 3 | Write the 20 Phase 0 programs with the restriction and see what breaks; index-based views cover most array code |
| Python-identical syntax raises expectations of Python semantics (reference types, exceptions, duck typing) that the language does not have | Medium | Low | Error messages written for Python programmers ("this value was moved on line 12; use .copy()"); a one-page "differences from Python" in the docs; the 20 Phase 0 programs tested on Python users for surprise |
| LLVM compile time dominates and release builds miss the 1.5x target | High | Medium | Dedup instances, `-O2` default, fewer larger modules, thin LTO only on request |
| Type checker scope creep (traits, associated types, HKTs) | High | High: Phase 1 slips months | Freeze the trait feature set in Phase 0; static dispatch only; no associated types in MVP |
| No users after MVP | Medium | High | Pick one concrete workload (a specific simulation or solver) as the Phase 3 gate, not a generic benchmark |
| Two backends drift in semantics | Medium | Medium | Execution corpus runs under both; any output diff is a CI failure |
| Debug info and ABI edge cases on each platform | High | Medium | macOS ARM (Apple M-series) only for MVP, since the dev machines are Macs; Linux x86-64 in Phase 2; Windows in Phase 4 |

### Where this plan pushes back on the brief

- "Readability at Python's level" and "low-level like Rust" conflict at exactly one point: pointers and lifetimes. The plan resolves it by removing lifetimes from the MVP, which means some systems programs Rust can express will need `unsafe` here. That is the trade-off; it cannot be designed away.
- "Mixing Fortran with C and Python" implies three mental models. The plan takes one thing from each and refuses the rest, because a language that is three languages is not readable.
- A top-tier compiler and a standalone IDE together exceed a small team's budget. The plan funds the compiler and gets the IDE through LSP; the standalone IDE is explicitly deferred.

### Decisions needed from you

1. Team size and who is on it. The estimates assume two to three engineers; one changes the MVP scope.
2. Target workload. Which concrete program (a solver, a simulation, a signal-processing pipeline) is the thing this language must run well? It sets the benchmark suite and the Phase 3 gate.
3. Platforms for the MVP. Decided: macOS ARM first, because developing on a Mac and targeting Linux x86 would mean cross-compiling or containers from day one. Mach-O output and Apple's AArch64 ABI add about 2 weeks to 1.10 and 1.11, already reflected in the estimates. Linux x86-64 is the first Phase 2 target so vectorization is validated on AVX as well as NEON.
4. Settled: Python syntax as closely as possible, with the departures listed in the syntax table. Still open within that: whether \`inout\` is written at the call site as well as the signature (Swift requires \`&y\` at the call; it makes mutation visible but adds noise). The plan says signature only.
5. Column-major versus row-major default for arrays. Column-major favors BLAS and Fortran interop; row-major favors C and NumPy interop. The plan says column-major with a per-type attribute.
6. C++ interop. Now in the plan as 4.1 because the Carbon-like positioning implies it, but it is the single largest package in the document (12+ weeks, likely more). Confirm it is worth that before Phase 4, or scope it to a C-shaped subset of C++ (no templates, no exceptions).
7. Name of the language and the file extension, which is needed before the tree-sitter grammar and the VS Code extension.
