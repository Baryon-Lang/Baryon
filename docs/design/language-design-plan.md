# Building a New Compiled Language: Technical Plan

First written Nov 4, 2026 by Collins Simpson. Brought up to date with decision records 0004 to
0016 on Nov 6, 2026, and with records 0017 to 0023 on Nov 8, 2026.

This plan explains why the language and its compiler are shaped the way they are. It is not where
work is tracked:

- [`ROADMAP.md`](../../ROADMAP.md) is the source of truth for every work item, its status, its
  owner and its estimate. This plan names roadmap items by ID and never repeats their status.
- The [decision records](../decisions/README.md) settle the hard-to-reverse choices. Where this
  plan and a decision record differ, the record wins.
- The [language reference](../reference/README.md) defines what a program may say and what it
  means, in detail.

## Summary and recommended approach

The language exists to write physics simulations and mathematical models, the work done today in
Fortran, C++ and Rust, with the syntax and simplicity of Python. Every design choice is judged
against that: does it make a molecular dynamics code, a PDE solver or a Monte Carlo model faster
to write, faster to run, or harder to get wrong? Interoperability with other languages waits for
Phase 4 so that nothing in the core is shaped by someone else's ABI.

The compiler is written in Rust. Its front end is query-based (demand-driven) on the `salsa`
crate, it lowers to an IR of its own, and LLVM is its first code generator; a fast debug backend
(Cranelift) follows in Phase 2 so compile speed and runtime speed are never traded against each
other.

The language takes Rust's ownership and RAII, Fortran's array semantics and no-alias guarantees,
and Python's syntax exactly: every source file is valid Python 3.12, read by an existing Rust
parser for Python ([0004](../decisions/0004-python-syntax.md)). It departs from Python's
semantics only where speed or safety leaves no choice, and it has no lifetime annotations because
references are limited in the MVP.

The single biggest risk is scope. A Rust-grade borrow checker, a C-grade optimizer and a
Python-grade syntax each took their original teams years. The plan reaches a usable MVP by making
four deliberate cuts:

1. **Python's grammar, not a new one.** No hand-written lexer or parser, no tree-sitter grammar,
   no formatter: Python's existing tools already parse, highlight and format the files
   ([0004](../decisions/0004-python-syntax.md)).
2. **Second-class references.** A reference cannot be stored in a data structure, returned or
   captured in the MVP. This removes lifetime inference, the hardest part of Rust, while keeping
   memory safety ([0006](../decisions/0006-parameter-passing.md)).
3. **No macros, trait objects, async or class inheritance** in the MVP. Each roughly doubles
   front-end complexity ([0015](../decisions/0015-no-class-inheritance.md) rules out inheritance
   for good, not only for the MVP).
4. **The IDE is a language server**, not a standalone editor. A standalone IDE is a Phase 4
   decision, and it may never be the right call.

Compile speed is a product requirement with numeric targets (see
[Compile-speed targets](#compile-speed-targets-and-how-each-is-met)), not an optimization pass for
later. The choices that make a compiler fast (parallel per-module front end, flat arenas,
incremental query caching, a decidable type system) cannot be retrofitted, so they are made from
the first crate.

## Where the project stands

As of Nov 6, 2026, the repository foundation is in place: ground rules, the roadmap and its
check, the crate architecture and its check, CI, the dependency policy, and the C++ and Fortran
baseline benchmark suite. The language reference is at version 0 and five example programs are
written. The compiler has started: the front end parses source files into a flat syntax tree,
rejects the Python constructs the language does not support, loads a package into the incremental
query database, resolves every name through scopes, modules and imports, and reports all of it
through `baryon check`. Type representation (roadmap 1.6) is next on the critical path.

The [roadmap](../../ROADMAP.md) has the item-by-item status.

## Design goals, non-goals and positioning

The target niche is numeric, scientific and analytics code where the author today writes C or
Fortran for the kernels and Python (or IDL/GDL) for loading, exploring and plotting the data
around them. The language serves both halves: a compiled kernel language in the Fortran tradition,
and an interactive data language in the Python and GDL tradition, with no boundary between them.
It must beat C on array code (by exploiting no-alias semantics the way Fortran does), match Rust
on memory safety for the common case, and read like Python.

| Goal | Measurable target for the MVP | Taken from |
| --- | --- | --- |
| Runtime speed on numeric kernels | Within 5% of `gfortran -O3` and `clang++ -O3` on the physics kernel suite in [`bench/`](../../bench/README.md) (N-body, Lennard-Jones MD, 3D heat stencil, CG on a sparse matrix, RK4, Monte Carlo, matmul, FFT), measured on Apple M-series; re-measured on x86-64 with AVX in Phase 2 | Fortran |
| Memory safety without a garbage collector | No use-after-free or data race in safe code. Bounds checks stay on in release builds and must cost under 2% ([0010](../decisions/0010-bounds-checks.md)) | Rust |
| Readability | A Python programmer reads a 200-line program and explains it without the manual | Python |
| Compile speed, debug build | 100k lines/sec end to end on an 8-core laptop; incremental rebuild of a one-function edit under 200 ms | Zig, Go |
| Compile speed, release build | Within 1.5x of `clang -O2` wall time on the same program | C |
| Interop | Not an MVP goal. User-facing interop with C, C++ and Python is Phase 4; the standard library's own calls into libm, BLAS and LAPACK are internal and invisible ([0012](../decisions/0012-no-ffi-in-mvp.md)) | C |

A seventh goal, for the analytics half, does not fit the MVP gate but shapes what the MVP must not
preclude: a data scientist loads a CSV or HDF5 file, filters and groups it, fits a model and plots
the result in under 30 lines, in a REPL, with the same language and compiler that produced the
kernel. Phase 3 is sized to that goal.

Non-goals for the MVP, each with the reason:

- **Garbage collection or reference counting as the default memory model.** It conflicts with
  predictable latency and with Fortran-class performance.
- **Lifetime annotations.** The reference model below avoids them.
- **Macros, metaprogramming and compile-time code execution** beyond constant folding. They cost
  front-end speed and tooling simplicity, which are both primary goals.
- **Async, coroutines and green threads.** Parallelism in the MVP is data parallelism (`prange`)
  plus plain OS threads.
- **Class inheritance**, ever. Classes are structs with methods; shared behavior comes from traits
  and shared data from composition ([0015](../decisions/0015-no-class-inheritance.md)).
- **A standalone IDE.** The language server gives most of the value for a fraction of the work.
- **Self-hosting.** Revisit only once the language is stable, since a bootstrapped compiler locks
  the language against breaking changes.
- **A user-facing foreign function interface** of any kind before Phase 4
  ([0012](../decisions/0012-no-ffi-in-mvp.md)).

Where each reference language falls short, and what this one does differently:

- **Rust:** lifetimes and the trait solver are the main source of its learning curve and its
  compile times. Dropping both for the MVP is the central bet.
- **C:** aliasing rules defeat vectorization, and `restrict` is rarely used. Here, references
  never alias by construction, so the optimizer always knows.
- **Fortran:** a strong array model and weak everything else (strings, generics, modules,
  tooling). Keep the array model, replace the rest.
- **Python:** readable, but dynamically typed and slow. Keep the syntax itself, not just its
  shape; make every type static, with Python's own annotations on signatures and inference inside
  bodies.
- **GDL/IDL:** the array-first data language of astronomers and geoscientists. Keep the
  array-first style and the batteries; compile it.
- **Carbon:** shows that a new low-level language is adopted only if it interoperates with
  existing code. Take the lesson for Phase 4: when interop arrives, it is first-class, not bolted
  on.

Idea [I.10](../../ROADMAP.md#ideas) proposes a fuller comparison with Julia, Mojo, Numba,
Pythran, Codon and Chapel.

## Language design

This section gives the shape of the language and the reason for each choice. The
[language reference](../reference/README.md) has the rules.

### Syntax

Every source file is valid Python 3.12 syntax, and the compiler never extends the grammar
([0004](../decisions/0004-python-syntax.md)). If Python has a spelling for something, the
language uses it: significant indentation, `def`, `elif`, `and`/`or`/`not`, f-strings,
comprehensions, `match`, `import`, `a[i, j]` indexing and `a[1:3]` slicing. Where Python has no
spelling, the language uses a name that Python's grammar already accepts:

| Construct | Spelling | Decision |
| --- | --- | --- |
| Value type (struct) | `class Particle(Struct):`; `class Vec3(Struct, Copy):` to copy on assignment | [0004](../decisions/0004-python-syntax.md), [0015](../decisions/0015-no-class-inheritance.md) |
| Compile-time constant | `N: Final = 64` | [0004](../decisions/0004-python-syntax.md) |
| Mutable parameter | `y: InOut[Array[f64, 1]]` | [0006](../decisions/0006-parameter-passing.md) |
| Consuming parameter | `buffer: Owned[Array[f64, 1]]` | [0006](../decisions/0006-parameter-passing.md) |
| Fallible function | `-> Raises[f64, IOError]` | [0005](../decisions/0005-error-handling.md) |
| Parallel loop | `for i in prange(n):` | [0004](../decisions/0004-python-syntax.md) |
| Parallel loop adding into arrays | `for i in prange(n, accumulate=forces):` | [0013](../decisions/0013-parallel-accumulation.md) |
| Vectorized loop | `for i in simd(range(n)):` | [0011](../decisions/0011-checked-simd.md) |
| Unsafe block | `with unsafe:` | [0004](../decisions/0004-python-syntax.md) |
| Unchecked index | `a.unchecked[i]`, only inside `with unsafe:` | [0010](../decisions/0010-bounds-checks.md) |
| Number conversion | `f64(x)`, `i32(n)` | [0014](../decisions/0014-number-conversions-are-calls.md) |
| Test and benchmark | `@test`, `@bench` | [0004](../decisions/0004-python-syntax.md) |
| Struct layout fixed on purpose | `@layout("C")` on a struct | [0017](../decisions/0017-layout-decorator.md) |

A kernel looks like this:

```python
def saxpy(a: f64, x: Array[f64, 1], y: InOut[Array[f64, 1]]):
    for i in prange(len(x)):
        y[i] = a * x[i] + y[i]


def mean(x: Array[f64, 1]) -> f64:
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

Because every file is Python, editors highlight it, Ruff and Black format it, and idea
[I.11](../../ROADMAP.md#ideas) could run the example programs under CPython as a correctness
oracle. The two things a Python programmer will trip on are that assigning a struct or array to a
new name moves it (use `.copy()`, as in NumPy) and that a parameter the function changes must be
marked `InOut`. Both produce a one-line error with a fix. The reference chapter
[Differences from Python](../reference/differences-from-python.md) lists every other surprise.

### Type system

- **Numbers** have explicit widths: `i8` to `i64`, `u8` to `u64`, `f32`, `f64`, `complex64`,
  `complex128`, plus `bool`. `int`, `float` and `complex` are the 64-bit defaults. Numbers mix the
  way Python's do within one width: `/` returns a float, literals take the type their context
  needs, and integers promote to floats. Different widths never mix implicitly; a conversion is a
  call, `f64(x)` ([0014](../decisions/0014-number-conversions-are-calls.md)).
- **Structs** are classes with the base `Struct`. They have value semantics and move on
  assignment unless they also have the base `Copy`. There is no inheritance
  ([0015](../decisions/0015-no-class-inheritance.md)).
- **Enums** are `class Color(Enum):`. Enums whose variants carry data (sum types) wait on decision
  [D.10](../../ROADMAP.md#decisions-needed), the spelling in valid Python syntax.
- **Generics** use Python 3.12's `def f[T](x: T)` and are monomorphized. Constraints are trait
  bounds with static dispatch only in the MVP; trait objects come in Phase 3 (roadmap 3.5). How
  user code declares a trait is decision [D.9](../../ROADMAP.md#decisions-needed); until then only
  the standard library's traits can be bounds.
- **Inference** is local: inside a function body, never across functions. Signatures are
  mandatory, which keeps checking decidable, fast and parallel, and keeps errors local.
- **No implicit nulls.** `T | None` is an optional value that must be checked before use.
- **Errors** are values. A function that can fail says so with `Raises[T, E]`; `raise` returns the
  error to the caller and `try`/`except` branches on it, with no unwinding. Out-of-range indexes
  and other bugs are panics, which cannot be caught ([0005](../decisions/0005-error-handling.md)).

### Memory model and references

Values are owned, moved by default and dropped at scope exit (RAII). Copy is opt-in per type. This
is the Rust model minus lifetimes.

Parameters are read-only borrows by default; `InOut[T]` is an exclusive, mutable borrow and
`Owned[T]` moves the value into the callee ([0006](../decisions/0006-parameter-passing.md)).
Borrows are second-class: one can be passed on to another call, but not stored in a struct,
returned or captured. So every borrow is scoped to a call, and the compiler needs no lifetime
annotations and no lifetime inference. Exclusivity (one `InOut` or many read-only borrows of a
place, never both) is checked per call site by a path-overlap analysis, written as a decision
procedure in [Values and references](../reference/values-and-references.md). This is the model
Swift used before `~Copyable` and the one Hylo is built on.

What it does not cover, and the escape hatches:

- **Data structures that hold references** (an iterator over a slice, a view into a buffer). MVP
  answer: array views carry an offset and strides, not pointers. Phase 3 answer: a limited
  lifetime system or explicit regions, decided after seeing what real programs need (roadmap 3.6).
- **Shared mutable state across threads.** MVP answer: `prange` bodies may only write disjoint
  elements, reduce into scalars, or add into arrays listed in `accumulate=`, all checked
  statically; plus `Mutex[T]` and atomics in the standard library.

### Numerics (the Fortran part)

- **Arrays** come in one family ([0007](../decisions/0007-array-types.md)): `Array[T, rank]` has a
  run-time shape and lives on the heap; `Fixed[T, shape]` has its shape in the type, lives inline
  and is `Copy`. Slices are views with offset and strides.
- **Layout** is row-major by default, as in C and NumPy, with a per-type opt-in to column-major
  ([0008](../decisions/0008-row-major-arrays.md)).
- **Whole-array expressions** such as `c = a * b + 2.0` compile to one fused loop with no
  temporaries (roadmap 3.1).
- **No aliasing** between `InOut` parameters is guaranteed by the reference rules, so every loop
  over distinct arrays is vectorizable without run-time checks.
- **Parallel loops.** `prange` iterations must write disjoint elements. Scalar reductions are
  inferred, and arrays the loop adds into are declared with `accumulate=` and combined from
  private per-chunk buffers ([0013](../decisions/0013-parallel-accumulation.md)). Float reductions
  are reproducible by default: bit-identical on every run and thread count.
- **Vector loops.** `simd(range(n))` is a checked request: if the compiler cannot vectorize the
  loop, compilation fails and says why ([0011](../decisions/0011-checked-simd.md)).
- **Bounds checks** stay on in every build mode; `a.unchecked[i]` inside `with unsafe:` opts out
  ([0010](../decisions/0010-bounds-checks.md)).
- **Floating point** is strict IEEE except inside a `with fastmath:` block, never a global flag.
- **Integer overflow** panics in debug builds and wraps in release builds
  ([0030](../decisions/0030-integer-overflow.md), decision D.11).

### Physics simulation and mathematical modeling (the core purpose)

These are language features and compiler guarantees, not library conveniences.

| Need in simulation code | Today | Here |
| --- | --- | --- |
| Small fixed-size vectors and matrices | hand-written `Vec3` in C++, `real(8), dimension(3)` in Fortran | `Fixed[f64, (3,)]`, `Fixed[f64, (3, 3)]`: inline, no heap, unrolled |
| Large grids and particle arrays | heap arrays, manual indexing | `Array[f64, 3]` with run-time shape and known strides |
| Complex numbers | `std::complex`, Fortran `complex` | `complex128` and `complex64` primitives, literals such as `1 + 2j` |
| Whole-array expressions without temporaries | Fortran array syntax, Eigen expression templates | fused into one loop nest in the IR, with a compiler remark if fusion fails |
| Stencils | explicit loops with halo handling | a `stencil` loop form with bounds proven once (planned; not yet in the reference) |
| No aliasing between arrays | Fortran assumes it; C needs `restrict` | proven by the reference rules; every loop gets `noalias` |
| Vectorization | the compiler hopes, the programmer reads assembly | `simd` loops fail to compile if they do not vectorize |
| Shared-memory parallelism | OpenMP pragmas | `prange` with a static disjoint-write check |
| Parallel reductions | OpenMP `reduction(+:x)` | inferred scalar reductions and `accumulate=` arrays, reproducible by default |
| Random numbers | per-library generators | counter-based generators (Philox), so parallel streams are reproducible from one seed |
| Units and dimensions | comments, Boost.Units | Phase 3 candidate: units checked at compile time, zero cost at run time |
| Sparse matrices and iterative solvers | PETSc, Eigen, hand-rolled CSR | Phase 3: sparse formats, CG and GMRES in `linalg` |
| ODE and PDE time stepping | hand-written RK4 | Phase 3: steppers that inline the user's function |
| Automatic differentiation | Enzyme, hand-coded Jacobians | Phase 4 candidate; the IR must not preclude it |
| Multi-node parallelism | MPI | Phase 4 candidate: distributed arrays, made possible by the shape-and-stride design |
| GPU | CUDA, OpenACC | Phase 4 candidate: `prange` offload through an LLVM GPU target |

The design rule is that none of the Phase 3 and 4 rows should need a new language feature.

### Modules

One file is one module and a directory is a package, with no `__init__` file. A module's top level
holds only imports, `Final` constants, type aliases, functions and classes, so there is no global
mutable state for a parallel loop to race on. A program starts at `main`. A single manifest drives
the build; the compiler is also the build tool, as in Go, Zig and Cargo.

### Data science and analytics (the Python and GDL part)

The compiled core makes analytics fast; this is what makes it convenient. All of it is library and
tooling on top of the language, which is why it can follow the MVP without reopening the compiler.

- **Broadcasting** follows NumPy's rules and fuses into one loop (roadmap 3.9).
- **Axis reductions** such as `x.sum(axis=0)` resolve the axis at compile time so the loop nest is
  fixed.
- **`DataFrame`**: typed columns, `filter`, `group_by`, `join`, `sort`, `select`, compiled to
  typed loops (roadmap 3.13). Missing data uses the Arrow layout: a validity bitmap, so `None`
  costs one bit.
- **File formats**: CSV, Parquet, HDF5, NetCDF and FITS (roadmap 3.14).
- **Plotting** with the GDL and matplotlib verbs (roadmap 3.15).
- **Interactive use**: a REPL on the Cranelift backend that keeps live values, and a Jupyter kernel
  on top of it (roadmap 3.11, 3.12). Without it the analytics half is not credible.
- **Python interop** (roadmap 3.17, Phase 4): call Python libraries that will never be rewritten,
  and exchange arrays with zero copy. Idea [I.5](../../ROADMAP.md#ideas) proposes exporting
  functions to Python earlier, in Phase 2.

Two MVP choices protect this: array layout and strides are a stable part of the array types, so
NumPy and Arrow buffers can be viewed without copying, and `None` on a value type is a real
optional from the start, not a sentinel.

## Compiler architecture

The compiler is one Rust binary, `baryon`, that parses, checks, optimizes, generates code, links and
orchestrates builds. Every stage up to the backends runs as a cached query in one incremental
database, so the language server and the batch compiler share one engine.

Pipeline: parse → reject unsupported constructs → resolve names → type check → reference check →
lower to IR → monomorphize → IR passes → backend (Cranelift for debug, LLVM for release) → link.

### Crates and modularity

Each stage is its own crate, and each crate has a kind that limits what it may depend on
([0001](../decisions/0001-modular-crate-architecture.md)). Implementation crates never depend on
each other; they exchange data only through interface crates, and only application crates such as
the `baryon` driver wire them together. A stage reads another stage's result through the database's
`Db` trait, never through that stage's crate ([0016](../decisions/0016-salsa-query-engine.md)).
So any stage can be replaced without touching the rest. [`architecture.md`](../architecture.md)
lists the crates and `cargo xtask check-architecture` enforces the rules.

### Dependencies

Compiler crates aim for zero external dependencies. Each one taken has a reason, and every
dependency passes the license, advisory and source rules in [`deny.toml`](../../deny.toml).

| Component | Choice | Why |
| --- | --- | --- |
| Parser | Ruff's Python parser (`ruff_python_parser`), pinned to an exact version behind the `parser` crate | The files are Python, so an existing fast, error-tolerant parser does the job; writing one would cost weeks and add nothing ([0004](../decisions/0004-python-syntax.md)) |
| Incremental queries | `salsa`, pinned to an exact version | Memoized, dependency-tracked queries make the IDE fast and incremental builds correct; it is what rust-analyzer uses ([0016](../decisions/0016-salsa-query-engine.md)) |
| Release backend | LLVM, through its textual IR and `clang` ([0020](../decisions/0020-llvm-backend-emits-text.md)) | Best optimizer available: vectorizer, LTO, PGO, every target |
| Debug backend | Cranelift (Phase 2) | Generates code 10 to 20 times faster than LLVM |
| Linker | `lld` bundled | Linking is often half of a debug build's wall time with the system linker |
| Interning and arenas | our own | Core to compile speed, so owned and tuned |

### Front end

The `parser` crate runs Ruff's parser and lowers its tree to the language's own flat syntax tree
in the `syntax` interface crate: 16-byte nodes stored in preorder in one array, not a tree of
boxed nodes. Lowering keeps the rest of the compiler independent of Ruff, so the parser can be
swapped. Syntax errors are rewritten for this language. The `syntax_check` crate then reports
every Python construct the language does not accept, each with a message that says why and what to
write instead.

Name resolution and type checking are queries keyed on a declaration: `signature_of(function)`,
`type_check(function)`, `layout_of(struct)`. Because signatures are mandatory, checking one
function needs only the signatures of the functions it calls, never their bodies. That makes
whole-program checking parallel and means an edit inside one body invalidates only that body's
queries.

The reference and exclusivity check is one query per function on the typed IR, a flow-insensitive
overlap check over access paths. It is deliberately simpler than Rust's borrow checker (no
non-lexical lifetimes, no two-phase borrows) because second-class references make those
unnecessary.

### Middle end: the IR

The typed tree lowers to a mid-level IR with explicit control flow, drops and bounds checks:
basic blocks over typed locals and places, as in Rust's MIR, not static single assignment form,
because the reference rules, `noalias` metadata and drops are all stated over places and the
backends build SSA themselves (decided in [0018](../decisions/0018-mid-level-ir.md)).
Language-specific optimizations live here, before any backend:

- array expression fusion into one loop nest;
- drop elision and move forwarding;
- bounds-check hoisting out of loops when the index range is provably within the shape;
- inlining of small generic instantiations after monomorphization;
- `noalias`, `nonnull` and `dereferenceable` metadata from the reference rules, so LLVM gets what
  Fortran compilers get for free.

Monomorphization happens here: lowering builds each generic body once, and an instance query
substitutes concrete types into it. Whole-array expressions stay one IR node until fusion has
seen them, and `prange` bodies are outlined for the runtime. Instantiations may later be
deduplicated by the hash of their lowered IR to control code size and compile time.

### Back ends

Release: the IR lowers to LLVM IR one function at a time, in parallel, one LLVM module per source
module. The default is `-O2` with the loop vectorizer; `-O3` and thin LTO are opt-in. Fast-math
flags are emitted only inside `with fastmath:` blocks. The MVP targets Apple AArch64 and Mach-O.

Debug (Phase 2): the same IR lowers to Cranelift with no optimization beyond register allocation.
This backend serves the edit-run loop, the REPL and the editor's run-test button.

Neither backend ever looks at the syntax tree, so the IR must carry all semantic information.

### Compile-speed targets and how each is met

| Target | Mechanism |
| --- | --- |
| Parse at 1M lines/sec across 8 cores | Ruff's parser, one file per task, lowered into a flat arena |
| Type check at 300k lines/sec across 8 cores | Per-function queries, mandatory signatures, local inference only, no trait objects in the MVP |
| Debug build 100k lines/sec end to end | Cranelift backend, parallel code generation, bundled `lld` |
| Incremental one-function edit under 200 ms | Query invalidation re-checks and re-generates only the edited function; object files are patched, not rebuilt |
| Release build within 1.5x of `clang -O2` | Fewer, larger LLVM modules; deduplicated instances; `-O2` by default |
| Language server cold start under 1 s on 100k lines | Parse and resolve only; type check open files lazily |

Two rules protect these numbers. No stage may be global: anything that needs the whole program
(dead code elimination, LTO) is a release-only pass after per-module work. And every run reports
time per query, and CI fails if a benchmark regresses more than 10% (roadmap 1.18, 2.3).

### Diagnostics

Error messages are a product feature. Every diagnostic has a primary span, labelled secondary
spans and a suggested fix where one exists, and tests check each one by its full rendered text.
Messages are written for Python programmers: the reference checker explains an exclusivity
violation in terms of the two access paths that overlap, never compiler internals.

## Runtime, standard library and tooling

The runtime is as small as C's: no garbage collector, no scheduler, no reflection. A statically
linked program that prints a number should be under 100 KB. It holds an allocator shim over the
system `malloc`, a panic handler with backtraces in debug builds, bounds-check and overflow-trap
intrinsics, and a thread pool for `prange`, started on first use (roadmap 1.12). It is a Rust
static library that compiled code calls through C functions, and the one crate outside a backend
allowed `unsafe` ([0021](../decisions/0021-runtime-crate-and-unsafe-code.md)).

The MVP standard library is deliberately narrow so the compiler team is not also a library team:

| Module | Contents |
| --- | --- |
| `core` | numbers, `bool`, `str` (a pointer and a length; a literal is never copied, [0022](../decisions/0022-str-values.md)), optionals, tuples, iteration |
| `array` | `Array` and `Fixed`, views, element-wise operations, reductions, `prange` and `simd` support, `matmul` through Accelerate on macOS and OpenBLAS elsewhere |
| `collections` | `list`, `str` (UTF-8), a hash map |
| `io` | standard streams, files, `print` with f-strings |
| `math` | `sqrt`, `exp`, trigonometry, `fma`, over libm |
| `random` | counter-based generators (Philox) |
| `thread` | `spawn`, `join`, `Mutex`, atomics |

There is no `ffi` module ([0012](../decisions/0012-no-ffi-in-mvp.md)). Phase 3 adds `linalg` and
`fft` over LAPACK and FFTW or Accelerate, `data` (DataFrame), file formats, `plot`, `stats` and the
REPL; Phase 4 adds `py` for Python interop.

Tooling ships in the one `baryon` executable, as in Go and Zig: `check`, `build`, `run`, `test`
with a built-in runner for `@test` functions, `doc` for Markdown pages from the documentation
strings, and `lsp`, the language server ([0019](../decisions/0019-language-server-protocol.md)),
all of which exist today as prototypes, and `bench` for `@bench` functions against a stored
baseline, which does not yet. [The `baryon` command](../reference/commands.md) documents each.
There is no `fmt`: Ruff and Black already format the files.

Testing the compiler needs four suites from the start: a parser corpus (exists), a type-checker
corpus with expected diagnostics, an execution corpus that compiles, runs and compares output, and
the benchmark suite with stored baselines for run time and compile time. Fuzzing follows once the
front end accepts the full language (roadmap 1.18).

## IDE and developer experience

A language server first, a VS Code extension second, and a standalone IDE only if real users need
one. The language server is the compiler's query engine behind the Language Server Protocol, so
every feature is nearly free once its query exists. It is `baryon lsp`, in the `baryon` executable,
and speaks the protocol over `serde_json` rather than through a framework
([0019](../decisions/0019-language-server-protocol.md)).

| Feature | Phase | Query it uses |
| --- | --- | --- |
| Highlighting | now | none: editors highlight the files as Python; semantic tokens add types later |
| Diagnostics as you type | MVP | `type_check`, `reference_check` |
| Go to definition, find references | MVP | `resolve` |
| Hover with the inferred type | MVP | `type_of` |
| Completion of names and members | MVP | `resolve`, `type_of` |
| One-click fixes | MVP | diagnostics carry edits |
| Rename, inlay hints (including `InOut` at call sites) | Phase 2 | find references, `type_of` |
| Run and debug a test from the gutter | Phase 2 | Cranelift build and the debug adapter |
| Which loops vectorized, fused or kept bounds checks, and why | Phase 3 | IR pass remarks |

The Phase 3 rows are where this language's IDE can beat Rust's and Python's: the compiler knows
array shapes and aliasing, so the editor can say which loops vectorized and why one did not, the
question every numeric programmer asks and today answers by reading assembly.

In order of payoff, what speeds up development: sub-second diagnostics; a fast debug backend with
a run button; debug info that works from the MVP (DWARF, so `lldb` shows the author's variables);
and error messages with fixes.

A standalone IDE is revisited after Phase 3, and only if the language needs an interaction editors
cannot host, such as live plots of arrays. Even then, a VS Code fork or a Zed extension comes
before a new editor.

## MVP definition and phases

The MVP is done when a physicist can write a 2,000-line simulation (a Lennard-Jones molecular
dynamics code and a 3D heat-equation solver are the reference programs), build it in both modes,
run it within 5% of the Fortran version, and edit it in VS Code with live diagnostics.

| Phase | Scope | Gate |
| --- | --- | --- |
| 0: specification and foundation | Language reference, example programs, baseline benchmarks, repository foundation | The 20 example programs are frozen and the reference rules are written as a decision procedure |
| 1: MVP compiler | Front end, query database, name resolution, types, reference check, IR, LLVM backend, driver, runtime, core and array libraries, language server, VS Code extension | The MVP definition above, with all four test suites green |
| 2: compile speed and the edit loop | Cranelift backend, incremental code generation, speed tuning, Linux x86-64, more editor features | Every number in the compile-speed table met |
| 3: optimization and data science | Fusion, bounds hoisting, vectorizer audit, LTO and PGO, trait objects, first-class references, the analytics track | Within 5% of gfortran on every kernel, the 30-line load, group, fit and plot script runs in the REPL, and three external users have written something real |
| 4: ecosystem and interop | C, C++ and Python interop, automatic differentiation, distributed arrays, GPU offload, more targets | Ongoing |

Phase 1 is the only phase whose scope is fixed; each later phase is sized by what the gate before
it revealed. The work items, their estimates in engineer-weeks and their dependencies are in
[`ROADMAP.md`](../../ROADMAP.md).

The critical path through Phase 1 runs 1.2, 1.4, 1.5, 1.6, 1.7, 1.9, 1.10, 1.11: front end, query
database, name resolution, types, type checker, IR, LLVM backend, driver. The first three are done.
The standard library, language server and execution corpus are off the critical path. Decision
0004 removed the lexer, the tree-sitter grammar and the formatter (1.1, 1.3, 1.15), about four
weeks of work.

The two items most likely to overrun are the type checker (1.7) and the LLVM backend (1.10). For
1.7 the overrun comes from generics and traits; if the schedule slips, ship generics without
trait bounds, since monomorphization does not need them to produce correct code. For 1.10 it comes
from debug info and ABI details; use LLVM's own C ABI lowering rather than writing one.

The estimates assume two to three engineers, one of whom has built a compiler before. A solo
effort roughly doubles each phase and should cut the MVP further, most likely by moving the
language server to Phase 2. Decision [D.1](../../ROADMAP.md#decisions-needed) records the real
team size.

## Risks

| Risk | Likelihood | Impact | Mitigation |
| --- | --- | --- | --- |
| Second-class references prove too restrictive for real programs | Medium | High: forces a lifetime system into Phase 3 | Write the Phase 0 example programs under the restriction and see what breaks; array views cover most array code |
| Files that are literally Python raise expectations of Python semantics (reference types, exceptions that unwind, duck typing, inheritance) | High | Medium | Messages written for Python programmers; the [Differences from Python](../reference/differences-from-python.md) chapter; the example programs tested on Python users |
| The parser dependency changes or disappears | Medium | Low | Ruff's parser is pinned to an exact version behind the `parser` crate and lowered to our own tree, so a replacement touches one crate |
| `salsa`'s API changes between versions | Medium | Low | Pinned to an exact version, upgraded on purpose; only the driver implements queries on it |
| LLVM compile time dominates and release builds miss the 1.5x target | High | Medium | Deduplicated instances, `-O2` by default, fewer larger modules, thin LTO only on request |
| Type checker scope creep (traits, associated types, higher-kinded types) | High | High: Phase 1 slips months | Freeze the trait feature set with decision D.9; static dispatch only; no associated types in the MVP |
| Bounds checks in release cost more than the 5% budget allows | Medium | Medium | Bounds-check hoisting moves into Phase 1 if any kernel loses more than 2% ([0010](../decisions/0010-bounds-checks.md)) |
| No users after the MVP | Medium | High | Pick one concrete workload as the Phase 3 gate (decision D.2) |
| Two backends drift in semantics | Medium | Medium | The execution corpus runs under both; any output difference fails CI |
| Debug info and ABI edge cases on each platform | High | Medium | macOS on Apple silicon only for the MVP; Linux x86-64 in Phase 2; Windows in Phase 4 |

Where this plan pushes back on the brief:

- "Readable like Python" and "low-level like Rust" conflict at one point: pointers and lifetimes.
  The plan removes lifetimes from the MVP, so some systems programs Rust can express will need
  `unsafe` here. That trade-off cannot be designed away.
- "Mixing Fortran with C and Python" implies three mental models. The plan takes one thing from
  each and refuses the rest, because a language that is three languages is not readable.
- A top-tier compiler and a standalone IDE together exceed a small team's budget. The plan funds
  the compiler and gets the IDE through the language server.

## Decisions

### Settled

| Choice | Record |
| --- | --- |
| Crates have kinds with enforced dependency rules | [0001](../decisions/0001-modular-crate-architecture.md) |
| `ROADMAP.md` is the source of truth for project management | [0002](../decisions/0002-roadmap-source-of-truth.md) |
| C++ and Fortran baseline suite in `bench/` | [0003](../decisions/0003-benchmark-suite.md) |
| Every source file is valid Python 3.12, read by an existing Rust parser | [0004](../decisions/0004-python-syntax.md) |
| Errors are `Raises`, `raise` and `try`, compiled to error returns; panics cannot be caught | [0005](../decisions/0005-error-handling.md) |
| Parameters are read-only borrows unless `InOut` or `Owned`; no call-site marker for now | [0006](../decisions/0006-parameter-passing.md) |
| One array family: `Array` on the heap, `Fixed` inline | [0007](../decisions/0007-array-types.md) |
| Arrays are row-major by default | [0008](../decisions/0008-row-major-arrays.md) |
| Numbers mix the way Python's do; conversions are calls such as `f64(x)` | [0014](../decisions/0014-number-conversions-are-calls.md), superseding [0009](../decisions/0009-number-mixing.md) |
| Bounds checks stay on in release builds | [0010](../decisions/0010-bounds-checks.md) |
| `simd` loops fail to compile if they do not vectorize | [0011](../decisions/0011-checked-simd.md) |
| No foreign function interface before Phase 4 | [0012](../decisions/0012-no-ffi-in-mvp.md) |
| Parallel loops add into shared arrays they list in `accumulate=` | [0013](../decisions/0013-parallel-accumulation.md) |
| Classes have no inheritance | [0015](../decisions/0015-no-class-inheritance.md) |
| Incremental queries run on `salsa` | [0016](../decisions/0016-salsa-query-engine.md) |
| `@layout("C")` pins a struct's declared layout and silences the padding warning | [0017](../decisions/0017-layout-decorator.md) |
| The mid-level IR is basic blocks over typed locals and places | [0018](../decisions/0018-mid-level-ir.md) |
| The language server is `baryon lsp`, speaking the protocol over `serde_json` | [0019](../decisions/0019-language-server-protocol.md) |
| The first LLVM backend writes textual IR and hands it to `clang` | [0020](../decisions/0020-llvm-backend-emits-text.md) |
| The runtime is one Rust crate of its own kind, allowed `unsafe` | [0021](../decisions/0021-runtime-crate-and-unsafe-code.md) |
| A `str` is a pointer and a length, and a literal is never copied | [0022](../decisions/0022-str-values.md) |
| The language is Baryon: files end in `.byn`, the tool is `baryon` | [0023](../decisions/0023-language-name-baryon.md) |

Platform for the MVP: macOS on Apple silicon, because the development machines are Macs; Linux
x86-64 is the first Phase 2 target, so vectorization is validated on AVX as well as NEON.

### Open

Each open decision is a row in the roadmap's
[Decisions needed](../../ROADMAP.md#decisions-needed) table, which tracks its status.

| ID | Question | What waits on it |
| --- | --- | --- |
| D.1 | Team size and hours per week | Every estimate |
| D.2 | Target workload for the Phase 3 gate (Lennard-Jones MD is suggested) | The Phase 3 gate |
| D.3 | Mark `InOut` at the call site as well as the signature | Revisited after the example programs |
| D.5 | C++ interop: full, a C-shaped subset, or dropped | Roadmap 4.1, the largest single item |
| D.8 | License: Apache-2.0 alone, or dual Apache-2.0 and MIT | Outside contributions |
| D.9 | How user code declares a trait and implements it: structurally like Python's `Protocol`, or by naming it like Rust | User-defined trait bounds in the type checker (1.7) |
| D.10 | How to spell an enum whose variants carry data, in valid Python syntax | Sum types and exhaustive `match` (1.7) |
| D.11 | Integer overflow in release builds: wrap or panic. Decided: wrap ([0030](../decisions/0030-integer-overflow.md)) | The runtime's overflow traps (1.12) and the reference's arithmetic rules |
