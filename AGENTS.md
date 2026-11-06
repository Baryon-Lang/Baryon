# Instructions for AI agents

This file is for every AI coding agent working in this repository: Claude, Codex, Copilot, Cursor
and the rest. Humans should read it too.

## Read first

1. [`README.md`](README.md): what this language is for.
2. [`CONTRIBUTING.md`](CONTRIBUTING.md): the rules. They bind you exactly as they bind humans.
3. [`ROADMAP.md`](ROADMAP.md): what is planned, who is working on what, and what is done.
4. [`docs/architecture.md`](docs/architecture.md): crate kinds and what may depend on what.
5. [`docs/decisions/`](docs/decisions/README.md): choices already made. Do not reopen one silently.
6. [`docs/README.md`](docs/README.md): where every other document lives.

The mission in one line: Python's readability with the speed and power of Rust, C and Fortran,
built to make physics, mathematics and data science at any scale routine. The motto: get to a
prototype fast, then optimize; a prototype is narrow, never sloppy. Documentation is paramount:
undocumented code is unfinished code.

## Rules specific to agents

- Update [`ROADMAP.md`](ROADMAP.md) when you start a task and when you finish it: mark the item
  `In progress` with your name, then `Done` with the pull request link. Check it first, so you
  never start work someone else has claimed. Put any idea you have along the way in its Ideas
  table instead of acting on it.
- Run `cargo xtask ci` and make it pass before every push.
- Document everything you add or change, public and private, as `CONTRIBUTING.md` rule 2
  requires: what it is for, why it works this way, and a tested example for anything public.
  Update every document your change affects in the same pull request.
- Fix the code, never the checks. Do not change lint levels, architecture rules, CI or tests to
  get a pass. If a rule looks wrong, say so in the pull request and let a human decide.
- Keep the diff to the task. No drive-by refactors, renames or reformatting of unrelated code.
- Do not add an external dependency, a crate, a top-level directory or `unsafe` code without the
  justification or decision record the rules require.
- When the task needs a design choice that the docs and decision records do not settle, stop and
  ask, or propose a decision record for review. Do not decide it inside a code change.
- Match the style of the code around you. Read the neighbouring module before writing a new one.

## Commands

| Task | Command |
| --- | --- |
| Every check CI runs | `cargo xtask ci` |
| Crate dependency rules only | `cargo xtask check-architecture` |
| External dependency licenses, advisories, duplicates and sources only | `cargo xtask check-dependencies` |
| Documentation links only | `cargo xtask check-links` |
| Example programs parse as Python only | `cargo xtask check-examples` |
| Roadmap only | `cargo xtask check-roadmap` |
| Build and open the API docs | `cargo doc --workspace --no-deps --open` |
| Tests for one crate | `cargo test --package <crate>` |
| Format | `cargo fmt --all` |
| Baseline benchmarks: build, verify, time | `cargo kernels` (see [`bench/README.md`](bench/README.md)) |
