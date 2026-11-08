# The `baryon` command

`baryon` is the one tool for a Baryon program: it checks, builds, runs, tests and documents a
package, and serves an editor. This chapter lists every command, what it needs, what it prints and
the exit status it ends with. It is a prototype (roadmap 1.11): arguments are positional, and there
are no options yet.

| Command | What it does |
| --- | --- |
| [`baryon check <directory>`](#baryon-check) | Reports every error and warning in the package |
| [`baryon build <directory> <output>`](#baryon-build) | Compiles the package into the executable `output` |
| [`baryon run <directory>`](#baryon-run) | Builds the package and runs it |
| [`baryon test <directory>`](#baryon-test) | Builds and runs every `@test` function |
| [`baryon doc <directory> <output>`](#baryon-doc) | Writes the package's documentation as Markdown |
| [`baryon lsp`](#baryon-lsp) | Runs the language server an editor talks to |

## Installing

`baryon` is built from this repository with the pinned Rust toolchain
([rustup](https://rustup.rs) installs it on first use):

```sh
cargo build --package driver --package runtime
```

That writes `target/debug/baryon` and, next to it, the runtime library `libruntime.a`, which every
program is linked with ([decision 0021](../decisions/0021-runtime-crate-and-unsafe-code.md)).
`baryon` looks for the runtime next to its own executable, so keep the two together; put
`target/debug` on your `PATH` or call `baryon` by its path.

`build`, `run` and `test` also need `clang`, which optimizes the compiled program and links it
([decision 0020](../decisions/0020-llvm-backend-emits-text.md)). On macOS it comes with the command
line tools, `xcode-select --install`, which also provide the `dsymutil` that keeps line numbers
for backtraces. On Linux, install your distribution's `clang` package. `check`, `doc` and `lsp`
need neither.

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
described under [`baryon run`](#baryon-run). Called with no arguments or wrong ones, `baryon` prints
the usage line and exits with 2:

```text
baryon: usage: baryon check <directory> | baryon build <directory> <output> | baryon run <directory> | baryon test <directory> | baryon doc <directory> <output> | baryon lsp
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
baryon build <directory> <output>
```

Checks the package as `check` does, compiles it, and writes the executable `output`, linked with
the runtime. On success it prints `built <output> from <n> file(s)` to standard error and exits
with 0. A package with errors, or that uses a feature the compiler cannot build yet, gets its
errors printed and exits with 1, with nothing written. Warnings are shown only by `check` today.

The program is optimized (`clang -O2`) and keeps its line tables, so a panic prints a backtrace of
`.byn` lines and `gdb` and `lldb` stop at and show them. On macOS the line tables are written to
`<output>.dSYM` beside the executable; keep it with the executable for backtraces to name lines.
There is one build mode for now; debug and release builds come later.

Running the executable runs `main`. It exits with:

| Status | Meaning |
| --- | --- |
| 0 | `main` returned |
| 1 | An error reached the end of `main`; it is printed as `error: ConfigError(line=3, path='heat.toml')` ([Errors](errors.md#main)) |
| 101 | A panic, such as a failed `assert` or an index out of range, printed with its location and a backtrace ([Panics](errors.md#panics)) |

## `baryon run`

```text
baryon run <directory>
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
baryon test <directory>
```

Builds every top-level function decorated `@test` into one test program and runs each test in its
own process, so a panic ends only its own test. It prints one line per test and a summary to
standard output, then the output and failure of each test that failed. The package needs no
`main`. [Tests](tests.md) defines what a test is and shows the full report.

Exit status 0 when every test passed, including when there are none; 1 when a test failed or the
package does not build.

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
`baryon check`, updated as you type. Go to definition, hover and completion come later (roadmap
1.16).

To use it, point your editor's Language Server Protocol client at `baryon lsp` for `.byn` files.
A VS Code extension that does this for you is roadmap item 1.17. It exits with 0 after the editor
sends `shutdown` and then `exit`, and with 1 if the editor exits or closes the connection without
`shutdown` first, as the protocol asks.
