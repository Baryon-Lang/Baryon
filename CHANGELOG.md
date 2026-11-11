# Release notes

What each release of Baryon holds, newest first. A release is a tag on `main`; nothing is
published to a package registry yet. Releases are time-based: one every two weeks, cut from what
`main` holds that day. [`ROADMAP.md`](ROADMAP.md#releases) tracks the steps of the release in
progress.

## Unreleased: Linux and macOS, due 2026-11-25

These notes were written for 0.2.0, which was never tagged. They ship with the first time-based
release, whose notes add everything merged since (roadmap R.5).

The second release, and the first to download and use without building the compiler: prebuilt
`baryon` archives for Linux and macOS, programs written the way the reference describes them
without the 0.1.0 workarounds, and the basics of a standard library. `baryon check` still knows the
whole of version 0 of the language, and the compiler now builds most of it.
[What builds today](docs/reference/status.md) lists what it does not build yet. Please
[open an issue](https://github.com/Baryon-Lang/Baryon/issues) for anything wrong, unclear or
missing, with what `baryon --version` prints.

### Platforms

| Platform | Needs |
| --- | --- |
| Linux on x86_64 with glibc 2.35 or newer: Ubuntu 22.04, Debian 12, Fedora 36 and later | `clang` 15 or newer (`sudo apt install clang-15` on Ubuntu 22.04, whose default is 14) |
| macOS 13.5 or newer on Apple silicon | The Command Line Tools 15 or newer, from `xcode-select --install` |

`clang` 15 is the floor because the LLVM backend writes opaque pointers
([decision 0020](docs/decisions/0020-llvm-backend-emits-text.md)). Both platforms are tested in CI
on every change. The release is tested on Ubuntu 22.04 and macOS 15; macOS 13.5 is the oldest
version whose `clang` is new enough, but no older Mac has run this release yet. Intel Macs and
musl Linux such as Alpine have no download and build from source. Windows comes in release 0.4.0.

### Install

The [beginner's guide](docs/guide/README.md#1-install-baryon) shows both ways:

- **Download** `baryon-0.2.0-x86_64-unknown-linux-gnu.tar.gz` or
  `baryon-0.2.0-aarch64-apple-darwin.tar.gz` from the release, with `curl` so macOS does not
  quarantine it, check it against its `.sha256` file, unpack it and put the directory on your
  `PATH`. `baryon` and the runtime library `libruntime.a` travel together in it. Before the
  archives are attached, the release workflow unpacks each on a fresh machine without Rust and
  builds, runs and tests the example programs and the guide's first program with it, in debug and
  `--release`, and checks that a missing `clang` gives the message that says how to install it.
- **Build from source** with `cargo`, as in 0.1.0.

### New in the language

- **Traits** ([decision 0031](docs/decisions/0031-traits-are-protocols.md)): declare one as a
  `Protocol` class, implement it by naming it as a base, and bound a type parameter with it,
  `[T: Body]`; calls resolve when compiling, with no run-time cost. `Embed[T]` fields reach an
  embedded struct's fields and methods through the outer one.
- **Array slices**: `a[i:j]`, `a[i:j:k]` and `a[:, k]` are views that copy nothing. Read them,
  pass them to read-only and `InOut` parameters, use them in whole-array arithmetic, and assign
  into them with `=` and `+=`, including `forces[a:b] += v` in a `prange` loop with
  `accumulate=`.
- **Array methods**: `a.min()`, `a.max()`, `a.dot(b)`, `a.round()`, `a.fill(value)`, and the
  matrix product `@` on rank-1 and rank-2 arrays.
- **Complex numbers**: `complex128` (`complex`) and `complex64`, `2j` literals, arithmetic,
  `z.real`, `z.imag`, `z.conjugate()`, `abs(z)`, `complex(re, im)` and `print`, with CPython's
  results bit for bit. Division by zero gives IEEE infinities and NaNs, as NumPy does, where
  Python raises.
- **Statements**: `for` over arrays, tuples and lists, a `range` step known only at run time,
  `**` and `**=` on integers, and nested unpacking `a, (b, c) = e`. `raise ... from` is rejected
  with a clear error ([decision 0034](docs/decisions/0034-no-error-chaining.md)).
- **Generics**: methods of generic structs, a `Callable` parameter passed on to another call or
  kept in a local, and default parameter values computed at each call
  ([decision 0027](docs/decisions/0027-default-parameter-evaluation.md)).
- **`print` of every value but arrays**: `None`, optional values, tuples, enum members as
  `Phase.GAS`, structs as `Type(field=value, ...)`, and lists and dicts as Python writes them,
  `['a', 'b']` and `{'a': 1}`, with the `sep=` and `end=` keywords. Output is flushed at exit and
  before a panic. Arrays print in 0.3.0, in NumPy's form.
- **Reproducible `simd` sums**: float sums and products in a `simd` loop vectorize in 8 fixed
  lanes, so they run faster and give the same bits on every machine.
- **Integer overflow checks** in debug builds now also cover generic code, `abs` of the minimum,
  whole-array arithmetic, and integer `sum`, `dot` and `@`.

### New in the standard library

- **`math`**: the rest of Python's `math` module for real numbers, such as `tan`, the inverse and
  hyperbolic functions, `atan2`, `hypot`, `log2`, `log10`, `log1p`, `expm1`, `cbrt`, `erf`,
  `gamma`, `lgamma`, `fma`, `floor`, `ceil` and `isnan`.
- **`str`**: `len`, `==`, `!=`, ordering, `in`, and `for c in text`, one character at a time.
- **Lists of any type**, such as `list[str]` and lists of structs, with `in`, `not in`,
  `copy()`, `pop()`, slices `xs[a:b]`, list comprehensions, and `for` over them. Dict
  comprehensions are not built yet.
- **`dict`** ([decision 0035](docs/decisions/0035-dict.md)): Python's dict, with integer, `bool`,
  `str`, enum and tuple keys, insertion order, and a panic on a missing key; float keys are
  rejected.

### Editor support

`baryon lsp` now shows a name's type and documentation on hover, goes to a definition across a
package's modules, and completes names in scope, a module's members after `m.`, and a value's
fields and methods after `x.`. The [VS Code extension](editors/vscode/README.md) uses them with no
change; version 0.2.0 from release 0.1.0 still works.

### Decisions made for this release

[Sum types as unions of structs](docs/decisions/0032-sum-types-are-unions-of-structs.md) (built in
0.3.0), [arrays print in NumPy's form](docs/decisions/0033-printing-arrays.md) (built in 0.3.0),
[no error chaining](docs/decisions/0034-no-error-chaining.md) and
[`dict`](docs/decisions/0035-dict.md).

### Known limits

- Not built yet, though `baryon check` accepts them: `print` of an array or of a tuple holding
  `None`; dict comprehensions; indexing, slicing and methods such as `split` on `str`; `match` on
  `str`; and complex numbers in f-strings and in `a.sum()`. [What builds today](docs/reference/status.md) has the
  full list. Each is reported as an error that says it is not built yet, never miscompiled.
- No reading or writing of files, command-line arguments or running other programs yet: that
  leads release 0.3.0 (roadmap 1.13.6).
- Speed is not yet compared with the C++ and Fortran baselines in [`bench/`](bench/README.md).
  Running within 5% of Fortran is the Phase 1 gate, still ahead.

## 0.1.0: review release, 2026-11-09

The first release, made so that people outside the project can try the language and review its
documentation, code and design. It is a prototype: `baryon check` knows the whole of version 0 of
the language, but the compiler builds only part of it into programs. Reports of anything wrong,
unclear or missing are the point of this release: please
[open an issue](https://github.com/Baryon-Lang/Baryon/issues), and paste what
`baryon --version` prints into it.

### Where to start

- [Beginner's guide](docs/guide/README.md): install `baryon`, run a first program, let the
  compiler catch a mistake, write tests, and simulate a mass on a spring.
- [Coding Baryon in VS Code](docs/guide/vscode.md): set up the editor and write, run and test a
  first program in it.
- [What builds today](docs/reference/status.md): exactly which parts of the language this release
  compiles.
- [Language reference](docs/reference/README.md) and
  [differences from Python](docs/reference/differences-from-python.md): the language itself.
- [Decision records](docs/decisions/README.md): why it is designed this way.

CI builds and runs every program in both guides exactly as written, on macOS (Apple silicon) and
Linux.

### The `baryon` command

[The `baryon` command](docs/reference/commands.md) documents each one.

| Command | What it does |
| --- | --- |
| `baryon check <directory>` | Reports every syntax, module, name, type, ownership and parallel-loop error in a package, with Rust-style messages |
| `baryon build [--release] <directory> <output>` | Compiles a package through LLVM and `clang` into a native executable that starts at `main` in `main.byn` |
| `baryon run [--release] <directory>` | Builds and runs a package in one step |
| `baryon test [--release] <directory>` | Runs every `@test` function and reports which passed, cargo-style |
| `baryon doc <directory> <output>` | Writes the package's documentation strings as Markdown pages |
| `baryon lsp` | A language server that publishes `baryon check`'s diagnostics as the user types |
| `baryon --version`, `baryon --help` | Print the release, `baryon 0.1.0`, and the usage line |

### What compiles

[What builds today](docs/reference/status.md) is the full list. In short:

- Numbers of every width with Python's mixing rules, `bool`, `str` with f-strings and Python's
  escapes, tuples, optional values `T | None`, structs, enums, `match`, `Final` constants, and
  constants and functions read through their module.
- Generic functions, compiled once per type they are called at, and functions passed as
  arguments to a `Callable` parameter, compiled once per function passed so calls inline.
- Lists of numbers and other copy types: `[a, b]`, `append`, `len` and bounds-checked indexing.
- The `math` functions and constants (`sqrt`, `exp`, `log`, `sin`, `cos`, `pow`, `pi`, `e`,
  `tau`, `inf`, `nan`), `abs`, `round`, and the reproducible random number generator
  `random.Philox` with `uniform()`.
- Ownership as the reference describes it: moves, copies, read-only borrows, `InOut` and `Owned`
  parameters, and every value destroyed on every path out of a function.
- Errors: `Raises`, `raise`, `try` and `except`, and panics that report their location and a
  backtrace of `.byn` lines.
- Arrays: `Array` on the heap and `Fixed` inline, `Array(values)` from a list, bounds-checked
  indexing, `shape` and `size`, whole-array arithmetic, and a pairwise floating-point `sum` that
  matches NumPy.
- Parallel and vector loops: `prange` over a thread pool, with inferred reductions that combine
  in a fixed order, so results are reproducible; `accumulate=` for loops that add into shared
  arrays, including struct fields such as `atoms.forces`; and `simd` loops, which ask LLVM to
  vectorize.
- All five [example programs](docs/examples/README.md) build and run: a heat equation solver,
  Lennard-Jones molecular dynamics, the Lorenz system with Runge-Kutta, conjugate gradient, and
  Monte Carlo estimation of pi. A test checks what each prints against an independent NumPy run
  of the same steps, and they agree to every printed digit.

### Fixed during review testing

Testing the release before the tag turned up these, all fixed (roadmap 1.9.15):

- `baryon --version` (or `-V`) and `baryon --help` (or `-h`, `help`) print the release and the
  usage line, with exit status 0 (#116).
- Clearer checker errors: a method or member that does not exist on `str`, a number or a tuple
  is reported by `baryon check` instead of failing at build, and `y = None` followed by `y = 3`
  suggests declaring `y` optional (#116).
- A module-level `N = 10` gets the module-level variable error with a note to write `Final`
  (#114).
- `abs` builds, with Python's results for `-0.0` and `nan`; `a.size` and `a.shape[k]` build
  (#114).
- Methods of a generic struct and `float("nan")` are reported as not built yet instead of failing
  inside the backend or `clang` (#114).
- `Array.zeros` of a `Copy` struct zeroes every field; `zeros` and `ones` of an enum, whose
  members are not numbers, are reported instead of picking a member (#112).
- String literals and f-string text take Python's backslash escapes and raw strings (#115).
- A literal assigned to a field or element takes that place's type, so `p.w = 1.5` works on an
  `f32` field (#115).
- Constants and enum members read through their module, `units.G` and `units.Phase.GAS`, build
  (#115).

### Editor support

The [VS Code extension](editors/vscode/README.md) (version 0.2.0) opens `.byn` files as Baryon,
highlights them with VS Code's Python grammar plus Baryon's own types and loop functions, and
runs `baryon lsp` so errors are underlined as you type. It installs from a `.vsix` file; it is not
on the Visual Studio Marketplace (roadmap decision D.16). Any other editor with a Language Server
Protocol client can run `baryon lsp` directly.

### Known limits

- Not built yet, though `baryon check` accepts them: complex numbers, array and list slices,
  `for` over an array or list, the matrix product `@`, `==`, `in` and `len` on `str`, `**` on
  integers, lists of non-copy types such as `list[str]`, methods of generic structs, and
  `print` of tuples, arrays and structs. [What builds today](docs/reference/status.md) has the
  full list. Each is reported as an error that says it is not built yet, never miscompiled.
- Integer overflow panics in a debug build, the default, and wraps around with `--release`
  ([decision 0030](docs/decisions/0030-integer-overflow.md)). Some operations are not checked
  yet and wrap in both, such as arithmetic in generic functions and integer `a.sum()`; the panic
  names the operator but not its operands. [What builds today](docs/reference/status.md) lists
  them.
- The language server reports diagnostics only: no go to definition, hover or completion yet
  (roadmap 1.16).
- Speed is not yet compared with the C++ and Fortran baselines in [`bench/`](bench/README.md).
  Running within 5% of Fortran is the Phase 1 gate, still ahead.
- Tested on macOS (Apple silicon) and Linux. Windows is untested.
- Open design questions that may change the language are listed under
  [Decisions needed](ROADMAP.md#decisions-needed) in the roadmap.
