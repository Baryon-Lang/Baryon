# The `baryon` command

`baryon` is the one tool for a Baryon program: it checks, builds, runs, tests and documents a
package, and serves an editor. This chapter lists every command, what it needs, what it prints and
the exit status it ends with. It is a prototype (roadmap 1.11): arguments are positional, and the
options are `--release`, `--timings`, `-k` for `test` and `--run` for `watch`.

| Command | What it does |
| --- | --- |
| [`baryon check <directory>`](#baryon-check) | Reports every error and warning in the package |
| [`baryon build [--release] [--timings] <directory> <output>`](#baryon-build) | Compiles the package into the executable `output` |
| [`baryon run [--release] [--timings] <directory>`](#baryon-run) | Builds the package and runs it |
| [`baryon test [--release] [--timings] [-k <text>] <directory>`](#baryon-test) | Builds and runs every `@test` function, or those whose name contains `text` |
| [`baryon watch [--release] [--run] <directory> <output>`](#baryon-watch) | Builds the package, then rebuilds it each time a file is saved |
| [`baryon doc <directory> <output>`](#baryon-doc) | Writes the package's documentation as Markdown |
| [`baryon lsp`](#baryon-lsp) | Runs the language server an editor talks to |
| `baryon --version` | Prints the release, such as `baryon 0.2.0`; put it in a bug report |
| `baryon --help` | Prints the usage line |

## Installing

From release 0.2.0, due on 2026-11-25, each release has prebuilt archives for Linux on x86_64 and macOS on Apple
silicon, holding `baryon` and `libruntime.a` together; the
[beginner's guide](../guide/README.md#download-a-prebuilt-baryon) says how to download, check and
install one. On any other system, or to follow `main`, `baryon` is built from this repository
with the pinned Rust toolchain
([rustup](https://rustup.rs) installs it on first use):

```sh
cargo build --package driver --package runtime
```

That writes `target/debug/baryon` and, next to it, the runtime library `libruntime.a`, which every
program is linked with ([decision 0021](../decisions/0021-runtime-crate-and-unsafe-code.md)).
`baryon` looks for the runtime next to its own executable, so keep the two together; put
`target/debug` on your `PATH` or call `baryon` by its path.

`build`, `run` and `test` also need `clang` 15 or newer, which optimizes the compiled program and
links it ([decision 0020](../decisions/0020-llvm-backend-emits-text.md)); older versions reject
the opaque pointers the backend emits. On macOS it comes with the command line tools,
`xcode-select --install`, which also provide the `dsymutil` that keeps line numbers for
backtraces. On Linux, install your distribution's `clang` package. `baryon` runs `clang` if it is
15 or newer, and otherwise the newest of `clang-30` down to `clang-15` it finds, the names
distributions give versions installed side by side; so on Ubuntu 22.04, whose `clang` is 14,
`sudo apt install clang-15` is enough. With no new enough `clang`, `build` says which version it
found and how to install one. `check`, `doc` and `lsp` need neither. On Linux, installing
LLVM's linker too (the `lld` package) makes every build's link about twice as fast; `baryon`
uses it when `clang` can find it, and links with the system linker otherwise.

## Packages

Every command except `lsp` takes the directory of a package. A package is every `.byn` file under
that directory, at any depth, and each file is a module named by its path:
`physics/nbody.byn` is the module `physics.nbody`, imported as `from physics.nbody import step`
([Syntax](syntax.md#source-files)). There is no manifest file yet.

A program starts at the function `main` in the file `main.byn` at the package's root; `build` and
`run` need it, and `check`, `test` and `doc` do not.

## Exit status

Every command uses the same exit statuses for its own outcome:

| Status | Meaning |
| --- | --- |
| 0 | The command succeeded |
| 1 | The package has errors, or does not build, or a test failed |
| 2 | The command was called wrongly, or a file or directory could not be read |

`baryon run` is the exception: once the program starts, it exits with the program's own status, as
described under [`baryon run`](#baryon-run). `baryon --version` and `baryon --help` print to
standard output and exit with 0. Called with no arguments or wrong ones, `baryon` prints the usage
line to standard error and exits with 2:

```text
baryon: usage: baryon check <directory> | baryon build [--release] [--timings] <directory> <output> | baryon run [--release] [--timings] <directory> | baryon test [--release] [--timings] [-k <text>] <directory> | baryon watch [--release] [--run] <directory> <output> | baryon doc <directory> <output> | baryon lsp
```

## `baryon check`

```text
baryon check <directory>
```

Loads the package and checks it without building it: module names, syntax, the constructs the
language accepts, names that refer to nothing, imports, types, moves and borrows, and every other
rule in this reference. It prints each error and warning to standard error, each with its source
line, then a count:

```text
warning: reordering the fields of `S` would save 8 bytes
 --> main.byn:1:7
  |
1 | class S(Struct, Copy):
  |       ^ 24 bytes as declared, 16 reordered
  = note: declare the fields in the order `b, c, a`; the compiler keeps the order written, so the layout matches C and `numpy`
  = note: if the order is fixed by a file format or a C library, decorate the struct `@layout("C")` to keep it without this warning

checked 1 file(s): 0 error(s)
```

Exit status 0 when there are no errors, even with warnings; 1 when there is an error.

`check` is the fastest way to find a mistake, and it accepts the whole of version 0. A package
that checks may still use a feature `build` cannot compile yet; see
[What builds today](status.md).

## `baryon build`

```text
baryon build [--release] [--timings] <directory> <output>
```

Checks the package as `check` does, compiles it, and writes the executable `output`, linked with
the runtime. On success it prints `built <output> from <n> file(s)` to standard error and exits
with 0. A package with errors, or that uses a feature the compiler cannot build yet, gets its
errors printed and exits with 1, with nothing written. Warnings are shown only by `check` today.

A debug build is compiled fast and not optimized (`clang -O0`); a release build is optimized
(`clang -O2`, with loops that sum floats vectorized while still adding in order, so results match
the debug build to the bit). Both keep their line tables, so a panic prints a backtrace of
`.byn` lines and `gdb` and `lldb` stop at and show them. On macOS the line tables are written to
`<output>.dSYM` beside the executable; keep it with the executable for backtraces to name lines.

### Debug and release builds

`build` makes a *debug build* unless given `--release`, anywhere after the command; `run` and
`test` take the same flag. Both keep their line tables. They differ in two things
([decision 0030](../decisions/0030-integer-overflow.md),
[decision 0048](../decisions/0048-debug-builds-compile-fast.md)):

| Build | Compiled | Integer arithmetic that overflows |
| --- | --- | --- |
| Debug, the default | Fast, not optimized: runs up to ten times slower | Panics with the operator and its location: ``panic: integer overflow in `+` `` |
| Release, `--release` | Optimized: takes about five times longer to compile | Wraps around in two's complement, and the check costs nothing |

Develop and test in a debug build, which compiles quickly and stops an overflow at the line that
caused it. Time, run long simulations in, and ship a release build. Every other check,
such as array bounds, stays on in both ([Integer overflow](types.md#integer-overflow)).

### Timing a build

`--timings`, anywhere after `build`, `run` or `test`, prints to standard error how long each
stage of the build took, once the executable is linked, then the package's lines per second:

```text
$ baryon build --timings nbody nbody.out
timings for 106 line(s) in 1 file(s):
  load              0 ms
  compile          21 ms  check, lower, IR passes, LLVM text
  machine code    239 ms  clang: optimize, generate code
  link            136 ms  clang: link with the runtime
  total           397 ms  267 lines/s
built nbody.out from 1 file(s)
```

`compile` is the time spent in `baryon` itself; `machine code` and `link` are `clang`. In a
debug build, a program with enough code is split into one LLVM module per core and each is
compiled by a `clang` of its own at the same time, so `machine code` is the time the slowest of
them took. A release build stays one module, so `clang` can inline any call into any function. Standard
library modules a program uses, such as `linalg`, are compiled too but not counted in its lines.
The [compile-speed baseline](../../bench/README.md#compile-speed) lists these numbers for the
benchmark kernels.

Running the executable runs `main`. It exits with:

| Status | Meaning |
| --- | --- |
| 0 | `main` returned |
| 1 | An error reached the end of `main`; it is printed as `error: ConfigError(line=3, path='heat.toml')` ([Errors](errors.md#main)) |
| 101 | A panic, such as a failed `assert` or an index out of range, printed with its location and a backtrace ([Panics](errors.md#panics)) |

## `baryon run`

```text
baryon run [--release] [--timings] <directory>
```

Builds the package as `build` does, into a scratch directory, runs it with the terminal's standard
input and output, and removes it. Once the program has run, `baryon run` exits with the program's
own status from the table above, so a script can tell a panic (101) from an error (1). A package
that does not build exits with 1, as `build` does, and nothing runs. Unlike `build`, it prints
nothing of its own on success, so the program's output is all you see:

```text
$ baryon run hello
hello, world
```

## `baryon test`

```text
baryon test [--release] [--timings] [-k <text>] <directory>
```

Builds every top-level function decorated `@test` into one test program and runs each test in its
own process, so a panic ends only its own test. It prints one line per test and a summary to
standard output, then the output and failure of each test that failed. The package needs no
`main`. [Tests](tests.md) defines what a test is and shows the full report.

With `-k <text>`, only the tests whose full name, `module.test_name`, contains `text` run, as
pytest's `-k` picks tests at its simplest: `-k test_flux` runs that test (and any whose name
contains it, such as `test_flux_speed`), and `-k heat.` runs the module `heat`'s tests. The
summary then counts the rest: `test result: ok. 1 passed; 0 failed; 3 filtered out`. The whole
package is still built, so a test elsewhere that does not compile stops the run.

Exit status 0 when every test passed, including when there are none; 1 when a test failed or the
package does not build.

## `baryon watch`

```text
baryon watch [--release] [--run] <directory> <output>
```

Builds the package into the executable `output` as [`build`](#baryon-build) does, then keeps
running: each time a `.byn` file in the package is saved, it builds again, and prints
`built <output> in <n> ms, <k> of <m> modules compiled` to standard error. With `--run`, it
runs the program after each build, with the terminal's input and output, then prints
`exited with <status>`. A build with errors prints them and `watching for changes`, and the
next save builds again, so `watch` never stops on its own; stop it with `Ctrl-C`.

It keeps the package loaded between builds ([decision 0050]): only a saved file's text is read
again, and the compiler's query engine reuses the parsing and checking of every file the edit did
not change. Adding or removing a file reads the package again. It looks at the files five times a
second, so a save is seen within 0.2 s. With `--run`, it looks again only once the program has
ended, so a program that never ends holds up the next build until you stop it.

[decision 0050]: ../decisions/0050-incremental-builds.md

It also keeps the machine code. The program is split into LLVM modules of a few functions each,
cut so that an edit changes only the modules holding the functions it reaches, and a module whose
text did not change reuses its machine code instead of running `clang` again; the message counts
the modules `clang` compiled. A change inside one function recompiles one module. A line added or
removed moves the source lines of every function below it in that file, so their modules are
compiled again too, since each instruction records its line for the debugger.

It keeps each function's LLVM text too. A function's text is written again only when something
it reads changed: its own body, the declarations of the types it uses, the names of the
functions it calls, or the text above it in its file, which its line numbers count. The other
functions of the edited file are still lowered again, since lowering reads the file as a whole,
but their text is kept when their lowered body comes out the same.

On the 4,706-line program of roadmap 2.0, a rebuild after changing a number in one function takes
about 0.3 s, against 0.85 s for `baryon build`, on four cores: checking takes 0.02 s, lowering
the edited file again 0.05 s, writing the modules 0.02 s, `clang` on the one module 0.1 s and the
link 0.06 s. A function of more than about 2,000 lines of LLVM text gets a module of its own, so
editing it, or a function near it, recompiles only what changed. Roadmap 2.2.2 aims at a rebuild
under 200 ms after a one-function edit.

## `baryon doc`

```text
baryon doc <directory> <output>
```

Writes the package's documentation as Markdown into the directory `output`, creating it if needed:
one page per module, named after the module, such as `physics.nbody.md`, and an `index.md` listing
every module with its summary. It prints `documented <n> module(s) in <output>` to standard error.
What a page holds is in [Syntax](syntax.md#documentation-strings).

A package with syntax or module naming errors gets no pages: its errors are printed and the exit
status is 1. Type errors do not stop `doc`, so documentation can be written for a package that is
still being fixed.

## `baryon lsp`

```text
baryon lsp
```

Runs the language server: an editor starts it and talks to it over standard input and output in
the Language Server Protocol, version 3.17
([decision 0019](../decisions/0019-language-server-protocol.md)). It loads every `.byn` file under
the editor's workspace root as one package and reports the same errors and warnings as
`baryon check`, updated as you type. It also answers three questions about the place the cursor
is, shows inlay hints, renames and test lenses, from the same checked package:

- **Hover**: what the name under the cursor is, as one line of code, and the documentation string
  of what it names, as `baryon doc` writes it. A variable shows the type the compiler inferred for
  it (`energy: f64`), a function its signature (`def step(dt: f64) -> None`), a constant its type
  and value (`G: Final[f64] = 9.81`), and a field, method or enum member what its class declares.
- **Go to definition**: the definition of a variable, function, class, constant, field, method or
  enum member, in whichever file of the package it is; a module goes to the start of its file.
  Names the language and the standard library provide have no source to go to.
- **Completion**: after `m.`, a module's public definitions and, for a package, its modules;
  after a struct value and a dot, its fields and methods; after an enum's name and a dot, its
  members; anywhere else, the names in scope: the function's variables bound on earlier lines,
  the module's definitions and imports, and the language's names. A dot asks for completion by
  itself. Names starting with `_` are offered only inside their own module. Methods of arrays,
  lists and `str` are not offered yet.
- **Inlay hints**: after the first assignment of each variable that has no annotation, the type
  the compiler inferred for it, as `energy: f64` where the code says `energy = …`; loop variables
  and the names a tuple is unpacked into get one too. Parameters and variables written
  `x: T = …` already say their type and get none.
- **Rename**: a variable within its function, or a function, class, constant or type alias
  defined at the top of a module everywhere the package names it, imports and `module.name`
  included. The resolver says what each name refers to, so a different thing with the same name
  is never touched. Parameters (a call may name one as a keyword argument), fields, methods,
  enum members and modules cannot be renamed yet, and the editor says so. A new name that is
  already taken where the rename reaches is refused too, since it would join two variables or
  hide a builtin, an import or another definition: a builtin such as `print`, any name the
  variable's function uses, or any name in a module the definition is defined or named in.
- **Test lenses**: a **Run test** link above each top-level function decorated `@test`, which
  asks the editor to run the command `baryon.runTest` with the test's full name. The VS Code
  extension runs it as `baryon test -k <name>`; another editor can bind the command itself.

Answers come from the parts of the file that parse and check, so code half typed still gets them
where it can, and gets nothing rather than an error where it cannot. Fixes come later (roadmap
1.16).

To use it, point your editor's Language Server Protocol client at `baryon lsp` for `.byn` files.
In VS Code, the [Baryon extension](../../editors/vscode/README.md) does this for you. It exits
with 0 after the editor sends `shutdown` and then `exit`, and with 1 if the editor exits or closes
the connection without `shutdown` first, as the protocol asks.
