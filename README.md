# Baryon: A new language for math and physics

A compiled language that reads like Python and runs like Rust, C and Fortran, built so that
physics, mathematics and data science at any scale are routine work.

A baryon is three quarks bound into one particle; Baryon binds Python's readability, Rust's safety
and Fortran's speed into one language. Source files end in `.byn`, and the command-line tool is
`baryon`. [Decision 0023](docs/decisions/0023-language-name-baryon.md) records the choice.

```python
def heat_step(u: Array[f64, 2], alpha: f64) -> Array[f64, 2]:
    out = u.copy()
    for i in range(1, u.shape[0] - 1):
        for j in range(1, u.shape[1] - 1):
            laplacian = u[i + 1, j] + u[i - 1, j] + u[i, j + 1] + u[i, j - 1] - 4.0 * u[i, j]
            out[i, j] = u[i, j] + alpha * laplacian
    return out
```

## Goals

- **Reads like Python.** Python's syntax wherever Python has a spelling for something, with static
  types.
- **Runs like Fortran.** Numeric kernels within 5% of `gfortran -O3`. Arrays never alias, so every
  loop can be vectorized.
- **Safe like Rust.** Ownership and no garbage collector; no use-after-free or data races in safe
  code.
- **Built for science.** N-dimensional arrays, complex numbers, parallel loops, reproducible
  random numbers and per-block floating-point control are part of the language, not add-ons.
- **Documented to learn from.** Every feature, crate and function is documented with tested
  examples, so the language can be learned and integrated from its documentation alone.

## Status

The compiler is a working prototype. `baryon build <directory> <output>` compiles a package through
LLVM into a native executable that starts at `main` in `main.byn`, with `clang` 15 or newer
installed; `baryon run` builds and runs it in one step, `baryon test` runs every `@test` function,
and `baryon watch` rebuilds on every save. It builds numbers, complex numbers, `str`, lists, dicts,
tuples, optionals, structs, enums, unions of structs with `match`, traits, generics, functions as
arguments, errors, arrays with slices, broadcasting and `@`, `prange` and `simd` loops, `with` and
`for` on a program's own classes, and all seven example programs. The standard library, written
in Baryon itself, covers `math` (with the double-double `f64x2` and `fsum`), `random.Philox`,
`stats`, `linalg` and `fft`, `DataFrame` with pandas's names, plotting with `byplot` (SVG, PNG
and PDF), data files (text tables, `.npy`, FITS, NetCDF and HDF5 through `h5by`), compression
(`zlib`, `gzip`, `compression.zstd`, `bz2` and `lzma`), and files, paths and processes. `baryon lsp` is a language server for any editor, and
the [VS Code extension](editors/vscode/README.md) runs it for you; `baryon doc` writes a
package's documentation strings as Markdown pages.
[What builds today](docs/reference/status.md) lists, feature by feature, what `baryon build`
compiles so far, and [The `baryon` command](docs/reference/commands.md) documents every command.

Release 0.2.0 is the first with prebuilt downloads for Linux and macOS and the first standard
libraries. Releases are time-based: a new one every two weeks, cut from `main`.
[`CHANGELOG.md`](CHANGELOG.md) says what each holds, and the [roadmap](ROADMAP.md#releases) says
how releases are made.

[`ROADMAP.md`](ROADMAP.md) is the source of truth for what is planned, in progress and done, and the
place to add new ideas. The reasoning behind it is in the
[design plan](docs/design/language-design-plan.md).

## Getting started

To write Baryon programs, follow the [beginner's guide](docs/guide/README.md): it installs
`baryon`, from a download or from source, and runs a first program.

To work on the compiler, install [rustup](https://rustup.rs). The pinned toolchain installs itself
on first use.

```sh
cargo xtask ci
```

That runs every check CI runs: format, roadmap, architecture rules, documentation links, example
syntax, dependency rules, lints, tests and API docs. It needs Python 3.12 or newer to check the
example programs, and [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) to check
dependencies:

```sh
cargo install --locked cargo-deny@0.20.2
```

## Documentation

New to Baryon? Start with the [beginner's guide](docs/guide/README.md), the first of four pages
on how to code in Baryon: then [a tour of the language](docs/guide/language-tour.md),
[arrays and numbers](docs/guide/arrays-and-numbers.md), and
[data, files and plots](docs/guide/data-files-and-plots.md). To try it in an editor,
[Coding Baryon in VS Code](docs/guide/vscode.md) sets up VS Code, from building `baryon` to
running and testing a first program. [`CHANGELOG.md`](CHANGELOG.md) says what each release
holds.

[`docs/README.md`](docs/README.md) lists every document by reader: learning the language,
contributing to the compiler, or integrating one of its crates.

## Repository layout

| Path | Contents |
| --- | --- |
| `ROADMAP.md` | Phases, work items, decisions and ideas: the source of truth |
| `CHANGELOG.md` | Release notes: what each release holds |
| `THIRD-PARTY-NOTICES.md` | Third-party content the repository carries, such as the DejaVu Sans outlines, with its licenses |
| `crates/` | The compiler, one crate per responsibility |
| `xtask/` | Repository automation behind `cargo xtask` |
| `deny.toml` | Dependency rules: allowed licenses and sources, advisories, duplicate crates |
| `editors/` | Editor integrations; `editors/vscode/` is the VS Code extension |
| `bench/` | Baseline physics kernels in C++ and Fortran, and `cargo kernels`, which times them |
| `docs/README.md` | Map of all documentation |
| `docs/architecture.md` | Crate kinds and dependency rules |
| `docs/decisions/` | Decision records |
| `docs/design/` | The language design plan and the AstroBaryon roadmap |
| `docs/guide/` | How to code in Baryon, from installing it to data and plots, and the VS Code setup guide, all run in CI as written |
| `docs/examples/` | Example programs written in the language |
| `docs/reference/` | The language reference: what a program may say and what it means |
| `.github/` | CI workflow and pull request template |

## Contributing

Read [`CONTRIBUTING.md`](CONTRIBUTING.md), then pick or add an item in [`ROADMAP.md`](ROADMAP.md).
AI agents start with [`AGENTS.md`](AGENTS.md).

## License

Baryon is licensed under either of the [Apache License 2.0](LICENSE) or the
[MIT License](LICENSE-MIT), at your option ([decision 0055](docs/decisions/0055-dual-license.md)).
Unless you say otherwise, a contribution you submit is licensed the same way, under both, with no
further terms. Third-party content the repository carries, such as the DejaVu Sans
font's outlines, is under its own licenses, listed in
[`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md).
