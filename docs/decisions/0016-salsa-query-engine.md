# 0016. Incremental queries run on the `salsa` crate

Status: Accepted
Date: 2026-11-06
Roadmap: 1.4

## Context

The design plan makes compile speed a product requirement: an incremental rebuild after a
one-function edit in under 200 ms, and a language server that answers from the same results as the
batch compiler. Both need a *query engine*: every stage after reading files is a function of the
compiler's inputs whose result is cached together with what it read, so an edit reruns only the
queries that read what changed. The plan names two ways to get one: the `salsa` crate, which
rust-analyzer and Astral's `ty` type checker are built on, or a thin clone of it.

The engine is hard to replace later. Every stage is written as queries against it, so it touches
every compiler crate, which is why it gets a record (`CONTRIBUTING.md` rule 7).

## Decision

- **Use `salsa`**, pinned to one exact version in the root `Cargo.toml` and upgraded on purpose,
  because its API changes between minor versions. Only the features the compiler uses are on:
  `macros` and `inventory`. Parallel queries (`rayon`) are turned on when a stage needs them.
- **The `database` interface crate holds the shared shape:** the inputs (each source `File`, and the
  `Package` that lists them), the file and module database (module names and the module map), and
  the `Db` trait with one method per stage result, such as `Db::parse`.
- **A stage reads another stage's result through `Db`, never through that stage's crate.** The
  application owns the concrete database and implements each `Db` method by calling the
  implementation crate inside a cached (`#[salsa::tracked]`) function. Replacing the parser, for
  example, changes that one function in the application, and nothing else. This keeps
  decision 0001's rule that implementation crates never depend on each other.
- **Spans keep using `source::FileId`.** Each `File` input carries the `FileId` its spans use, and
  the application implements `source::Sources` so diagnostics render from the database.

## Consequences

- Memoization, dependency tracking, early cutoff (a query whose result is unchanged does not
  rerun its readers), cycle detection, and cancellation for the language server come ready-made and
  tested by two production type checkers. Writing them ourselves would cost weeks and would be the
  riskiest code in the compiler, since a missed dependency means a stale result.
- About thirty crates join the dependency tree, all under licenses `deny.toml` allows. Salsa is
  macro-heavy, so its generated items carry no documentation; the one module that declares inputs
  allows that lint, and every struct documents its generated methods.
- Salsa is still marked experimental, and minor versions break its API. The exact pin and a
  deliberate upgrade contain that. If it ever stops being maintained, the `Db` trait and the
  inputs are the seam: a replacement engine re-implements the `database` crate and the
  application's wiring, and stages that only call `Db` methods do not change.
- A thin clone stays possible later, behind the same seam, if profiling shows salsa's overhead
  matters against the compile-speed targets.
