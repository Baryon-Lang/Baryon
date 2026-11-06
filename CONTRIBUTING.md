# Contributing

These rules bind every contributor, human or AI agent. A change that breaks one is not merged.
Most of them are enforced by `cargo xtask ci`; the rest are enforced in review.

## Mission

NPL (working name) is a compiled language that reads like Python and runs like Rust, C and
Fortran. It exists to make physics, mathematics and data science fast to write and fast to run.
Enormous calculations and huge datasets should be routine work, not heroics.

Every decision is judged against that mission: does this change make a simulation, a solver or an
analysis easier to write, faster to run, or harder to get wrong? The full design is in
[`docs/design/language-design-plan.md`](docs/design/language-design-plan.md).

## Motto: prototype first, then optimize

We follow the Skunk Works golden rule: get to a working prototype as fast as possible, then
optimize.

- **A prototype is narrow, never sloppy.** Cut scope, not quality. A prototype implements less, and
  what it implements meets every rule below.
- **Optimize with evidence.** Performance work starts from a benchmark that shows the problem and
  ends with one that shows the gain.
- **The best part is no part.** Before adding a crate, a dependency, an abstraction or an option,
  check whether the change works without it.

## Rule 1: modular and replaceable

No module may break another. Any implementation can be swapped for a new one without touching
the rest.

- One crate, one responsibility. Every crate declares a kind, and the kind limits what it may
  depend on. [`docs/architecture.md`](docs/architecture.md) defines the kinds;
  `cargo xtask check-architecture` enforces them.
- An implementation crate never depends on another implementation crate. Stages exchange data only
  through interface crates, and only application crates wire implementations together.
- A crate's public API is the smallest that does the job, and every public item is documented.
  Everything else is private or `pub(crate)`.
- No global mutable state. A stage takes inputs and returns outputs, including its diagnostics. It
  never prints, reads environment variables or touches the file system unless that is its one job.
- A change to an interface crate is its own pull request.
- A replacement implementation passes the same tests as the one it replaces.

## Rule 2: exceptionally clean

The repository should look like someone obsessive built it, because someone did.

- Formatted with `rustfmt`. Zero warnings under `clippy::pedantic`, `missing_docs` and the
  workspace lints in [`Cargo.toml`](Cargo.toml).
- `unsafe` is forbidden in every crate. A crate that truly needs it (the runtime, a backend) gets an
  exception through a decision record, and every `unsafe` block carries a `// SAFETY:` comment.
- Library code never calls `unwrap`, `expect`, `panic!`, `todo!` or `dbg!`, and never prints.
  Errors are values. `assert!` is reserved for invariants whose violation is a bug, documented
  under `# Panics`.
- No dead code, no commented-out code, no `TODO` comments. Open an issue instead.
- Names are full words: `expression`, not `expr`. Universal abbreviations (`id`, `io`, `ir`) are
  fine.
- Every module opens with a `//!` comment saying what it is for. Other comments explain why, not
  what.
- A file past about 500 lines is split by responsibility.
- The top-level layout in the [README](README.md#repository-layout) is fixed. A new top-level
  directory needs a decision record.

## Rule 3: tested

- Every crate has unit tests. Every bug fix adds a test that fails without the fix.
- Tests are deterministic and fast. Diagnostics are tested by their full rendered text.
- Never weaken, skip or delete a test to make a change pass.

## Rule 4: few dependencies

- External crates are declared once, in `[workspace.dependencies]` in the root `Cargo.toml`.
- A pull request that adds one says in a sentence why writing it ourselves would be worse.
  Compiler crates aim for zero external dependencies.

## Rule 5: decisions are written down

A choice that is hard to reverse, or that affects more than one crate, gets a decision record in
[`docs/decisions/`](docs/decisions/README.md) before or with the code that implements it.

## Workflow

1. Branch from `main` as `<type>/<short-description>`, for example `feat/lexer-indentation`.
2. Make one logical change. Keep the pull request small enough to review in one sitting.
3. Run `cargo xtask ci` and make it pass. CI runs the same command on macOS (Apple silicon) and
   Linux.
4. Write commits as [Conventional Commits](https://www.conventionalcommits.org/):
   `type(scope): summary`, imperative mood, at most 72 characters, with a body that explains why.
   Types: `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `build`, `ci`, `chore`.
5. Open a pull request using the template. `main` always passes CI; pull requests are squash-merged.

The Rust toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml) and `Cargo.lock` is
committed, so every machine builds the same thing.
