# Contributing

These rules bind every contributor, human or AI agent. A change that breaks one is not merged.
Most of them are enforced by `cargo xtask ci`; the rest are enforced in review.

## Mission

Baryon is a compiled language that reads like Python and runs like Rust, C and
Fortran. It exists to make physics, mathematics and data science fast to write and fast to run.
Enormous calculations and huge datasets should be routine work, not heroics.

Every decision is judged against that mission: does this change make a simulation, a solver or an
analysis easier to write, faster to run, or harder to get wrong? The full design is in
[`docs/design/language-design-plan.md`](docs/design/language-design-plan.md).

## Motto: prototype first, then optimize

We follow the Skunk Works golden rule: get to a working prototype as fast as possible, then
optimize.

- **A prototype is narrow, never sloppy.** Cut scope, not quality. A prototype implements less, and
  what it implements meets every rule below, documentation included.
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
- A crate's public API is the smallest that does the job. Everything else is private or
  `pub(crate)`.
- No global mutable state. A stage takes inputs and returns outputs, including its diagnostics. It
  never prints, reads environment variables or touches the file system unless that is its one job.
- A change to an interface crate is its own pull request.
- A replacement implementation passes the same tests as the one it replaces.

## Rule 2: documented extensively

Documentation is paramount. A physicist new to the language and an engineer integrating one of
its crates should both be able to learn what they need from the documentation alone. Code without
documentation is unfinished, and unfinished code is not merged.

- **Every item is documented, public and private:** crates, modules, types, fields, variants,
  functions and constants. The `missing_docs` and `clippy::missing_docs_in_private_items` lints
  enforce it, and CI builds the documentation for private items with every rustdoc warning
  treated as an error.
- **Every crate explains itself.** Its `//!` documentation says what it is for, where it sits in
  the compiler, and shows an example.
- **Examples are tested.** Every public type or function a caller uses directly has an example in
  its documentation. Examples are doc tests, so they run in CI and cannot drift from the code.
- **The standard sections are always present:** `# Errors` on anything that returns a `Result`,
  `# Panics` on anything that can panic, `# Safety` on anything `unsafe`.
- **Say why and when, not only what.** Document units, invariants and limits, the cost of anything
  that touches large data, and the decision record behind a non-obvious choice.
- **Docs change with the code.** A pull request that changes behavior updates every document it
  affects: rustdoc, `docs/`, the README and, once it exists, the language reference. A language
  feature ships with its reference entry and a worked example in the same pull request, and a
  change that teaches lowering or a backend to build a feature updates
  [What builds today](docs/reference/status.md).
- **Links stay alive.** `cargo xtask check-links` fails on any relative link in the Markdown
  documentation that points nowhere.
- **Write for a learner.** Plain words, short sentences, and a definition the first time a term
  appears.

[`docs/README.md`](docs/README.md) maps all documentation by reader.

## Rule 3: the roadmap is the source of truth

[`ROADMAP.md`](ROADMAP.md) is where project management happens. It lists every phase, work item,
open decision and idea, with who is working on what.

- **Claim before you start.** Mark the item `In progress` with your name, or add the item if it is
  missing. Link your draft pull request so others can see the claim.
- **Close when you finish.** The pull request that completes an item marks it `Done` and links
  itself.
- **Ideas are always welcome, through the roadmap.** Add any new idea to the Ideas table. Work that
  is not on the roadmap does not get merged, so a good idea that skips the roadmap is lost.
- **Never delete a row.** Finished and dropped items are the project's history.

`cargo xtask check-roadmap` validates every row, and CI fails any change that does not update
`ROADMAP.md`.

## Rule 4: exceptionally clean

The repository should look like someone obsessive built it, because someone did.

- Formatted with `rustfmt`. Zero warnings under `clippy::pedantic` and the workspace lints in
  [`Cargo.toml`](Cargo.toml).
- `unsafe` is forbidden in every crate. A crate that truly needs it (the runtime, a backend) gets an
  exception through a decision record, and every `unsafe` block carries a `// SAFETY:` comment.
  The runtime's exception and its rules are
  [decision 0021](docs/decisions/0021-runtime-crate-and-unsafe-code.md).
- Library code never calls `unwrap`, `expect`, `panic!`, `todo!` or `dbg!`, and never prints.
  Errors are values. `assert!` is reserved for invariants whose violation is a bug.
- No dead code, no commented-out code, no `TODO` comments. Open an issue instead.
- Names are full words: `expression`, not `expr`. Universal abbreviations (`id`, `io`, `ir`) are
  fine.
- A file past about 500 lines is split by responsibility.
- The top-level layout in the [README](README.md#repository-layout) is fixed. A new top-level
  directory needs a decision record.

## Rule 5: tested

- Every crate has unit tests. Every bug fix adds a test that fails without the fix.
- Tests are deterministic and fast. Diagnostics are tested by their full rendered text.
- Never weaken, skip or delete a test to make a change pass.

## Rule 6: few dependencies

- External crates are declared once, in `[workspace.dependencies]` in the root `Cargo.toml`.
- A pull request that adds one says in a sentence why writing it ourselves would be worse.
  Compiler crates aim for zero external dependencies.
- Every dependency, direct or indirect, passes the rules in [`deny.toml`](deny.toml): a license
  compatible with Apache-2.0 and MIT, no RustSec advisory, one version per crate, and crates.io
  as the only source. `cargo xtask check-dependencies` enforces them; `deny.toml` says how to
  record an exception and why each one exists.

## Rule 7: decisions are written down

A choice that is hard to reverse, or that affects more than one crate, gets a decision record in
[`docs/decisions/`](docs/decisions/README.md) before or with the code that implements it.

## Workflow

1. Find your item in [`ROADMAP.md`](ROADMAP.md), or add it.
2. Branch from `main` as `<type>/<short-description>`, for example `feat/syntax-tree-lowering`.
3. Mark the item `In progress` with your name, open a draft pull request, and link it from the
   item.
4. Make one logical change. Keep the pull request small enough to review in one sitting.
5. Update the documentation the change affects.
6. Mark the item `Done` when the work is complete.
7. Run `cargo xtask ci` and make it pass. CI runs the same command on Linux for every push, and
   on macOS (Apple silicon) too once the pull request carries the `ready-to-merge` label and on
   every push to `main`. A macOS minute costs ten Linux minutes, so CI saves it for the run that
   decides the merge. A change to Markdown alone, outside `docs/guide/`, runs only the conflict,
   roadmap and link checks.
8. Write commits as [Conventional Commits](https://www.conventionalcommits.org/):
   `type(scope): summary`, imperative mood, at most 72 characters, with a body that explains why.
   Types: `feat`, `fix`, `perf`, `refactor`, `test`, `docs`, `build`, `ci`, `chore`.
9. Mark the pull request ready for review and add the `ready-to-merge` label, which starts the
   macOS run. A pull request merges only when every job, macOS included, is green on its last
   commit. `main` always passes CI; pull requests are squash-merged.

The Rust toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml) and `Cargo.lock` is
committed, so every machine builds the same thing.
