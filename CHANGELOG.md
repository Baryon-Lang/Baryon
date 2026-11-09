# Release notes

What each release of Baryon holds, newest first. A release is a tag on `main`; nothing is
published to a package registry yet. [`ROADMAP.md`](ROADMAP.md#releases) tracks the steps of the
release in progress.

## 0.1.0: review release (not tagged yet)

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
