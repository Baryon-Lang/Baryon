# Instructions for AI agents

This file is for every AI coding agent working in this repository: Claude, Codex, Copilot, Cursor
and the rest. Humans should read it too.

## Read first

1. [`README.md`](README.md): what this language is for.
2. [`CONTRIBUTING.md`](CONTRIBUTING.md): the rules. They bind you exactly as they bind humans.
3. [`docs/architecture.md`](docs/architecture.md): crate kinds and what may depend on what.
4. [`docs/decisions/`](docs/decisions/README.md): choices already made. Do not reopen one silently.
5. [`docs/README.md`](docs/README.md): where every other document lives.

The mission in one line: Python's readability with the speed and power of Rust, C and Fortran,
built to make physics, mathematics and data science at any scale routine. The motto: get to a
prototype fast, then optimize; a prototype is narrow, never sloppy. Documentation is paramount:
undocumented code is unfinished code.

## Rules specific to agents

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
| Documentation links only | `cargo xtask check-links` |
| Build and open the API docs | `cargo doc --workspace --no-deps --open` |
| Tests for one crate | `cargo test --package <crate>` |
| Format | `cargo fmt --all` |
