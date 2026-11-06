# Architecture

The compiler is a set of small crates with one-way dependencies. Each crate declares a kind, and
the kind decides what it may depend on. The rules exist for one reason: any implementation can be
replaced without touching anything else.

## Crate kinds

| Kind | Holds | May depend on |
| --- | --- | --- |
| `foundation` | Building blocks with no compiler logic: source text, spans, diagnostics | `foundation` |
| `interface` | Data types and traits that stages exchange: syntax trees, types, IR | `foundation`, `interface` |
| `implementation` | One replaceable piece of the compiler: a parser, a type checker, a backend | `foundation`, `interface` |
| `application` | An executable that wires implementations together: the driver, the language server | `foundation`, `interface`, `implementation` |
| `tooling` | Repository automation | any kind except `tooling` |

A crate declares its kind in its `Cargo.toml`:

```toml
[package.metadata.architecture]
kind = "foundation"
```

`cargo xtask check-architecture` fails if a crate has no kind, an unknown kind, or a dependency its
kind forbids. CI runs it on every pull request.

## How stages connect

An implementation never depends on another implementation. Stages talk through interface crates:

```text
source, diagnostics            foundation
        │
syntax ─┼─ types ─ ir          interface: the data each stage produces
        │
parser   checker   backend     implementation: each reads and writes interface types only
        │
driver                         application: runs parser → checker → backend in order
```

The type checker reads `syntax` trees; it does not know which parser built them. To replace the
parser, write a new implementation crate that produces `syntax` trees and point the driver at it.
Nothing else changes.

## Rules every stage follows

- **Inputs in, outputs out.** A stage returns its results and its diagnostics. It does not print,
  exit the process or keep global state.
- **Ids, not references.** Stages refer to files by `source::FileId` and to text by
  `source::Span`, and later to declarations by id. That keeps stage outputs small and cacheable,
  which the incremental query engine in the design plan depends on.
- **Diagnostics are data.** Problems are `diagnostics::Diagnostic` values. Only an application
  decides how to show them.

## Crates

| Crate | Kind | Purpose |
| --- | --- | --- |
| [`source`](../crates/source) | `foundation` | Source files, file ids, byte spans, line and column lookup |
| [`diagnostics`](../crates/diagnostics) | `foundation` | Compiler messages and their plain-text rendering |
| [`xtask`](../xtask) | `tooling` | `cargo xtask`: the CI checks, including this document's rules |

A crate is created when it has code to hold, never as a placeholder.

## Adding a crate

1. Create `crates/<name>/` with a `Cargo.toml` that inherits the workspace package fields and
   lints, as the existing crates do.
2. Declare its kind under `[package.metadata.architecture]`.
3. Add it to `[workspace.dependencies]` in the root `Cargo.toml` if other crates will use it.
4. Add a row to the table above.
5. Run `cargo xtask ci`.
