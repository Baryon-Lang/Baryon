# Documentation

Every piece of documentation in the repository, by who it is for. If something you needed was
missing or unclear, that is a bug: open an issue.

## Learning the language

The compiler is a prototype. `baryon check <directory>` reports every error the checker finds,
but `baryon build`, `baryon run` and `baryon test` compile only part of the language so far, and
none of the [example programs](examples/README.md) builds yet;
[What builds today](reference/status.md) lists, chapter by chapter, what does. Until the beginner's guide (roadmap item 0.8.3) is written, the language is
learned from its definition and from the examples. Build the tool with
`cargo build --package driver` and run it as `target/debug/baryon`.

To see the same diagnostics in an editor as you type, point its Language Server Protocol client
at `baryon lsp` for `.byn` files: build it with `cargo build --package driver` and run
`target/debug/baryon lsp`. A VS Code extension that does this for you is roadmap item 1.17.

To read a package's documentation strings as pages, run
`cargo run --package driver -- doc <directory> <output>`: it writes one Markdown page per module
and an `index.md` listing them. What it reads is in the
[syntax chapter](reference/syntax.md#documentation-strings).

- The [language reference](reference/README.md) defines what a program may say and what it means.
  Python programmers should start with its
  [differences from Python](reference/differences-from-python.md).
- [`examples/`](examples/README.md) has five physics and mathematics programs written in the
  language, each explaining the features it uses.
- The [decision records](decisions/README.md) explain each settled or proposed design choice.
- The [design plan](design/language-design-plan.md) gives the reasoning behind the whole design.
  Where it and a decision record differ, the decision record wins.

## Contributing to the compiler

| Document | What it covers |
| --- | --- |
| [`ROADMAP.md`](../ROADMAP.md) | The source of truth: phases, work items, who is on what, decisions needed, ideas |
| [`CONTRIBUTING.md`](../CONTRIBUTING.md) | The binding rules: modularity, documentation, roadmap, cleanliness, tests, dependencies, workflow |
| [`AGENTS.md`](../AGENTS.md) | Extra rules for AI coding agents |
| [`architecture.md`](architecture.md) | Crate kinds, what may depend on what, and how stages connect |
| [`bench/README.md`](../bench/README.md) | The baseline benchmark suite: the kernels, how to run and record them, and the numbers Baryon must match |
| [`decisions/`](decisions/README.md) | Every hard-to-reverse choice, why it was made, and what it costs |
| [`design/language-design-plan.md`](design/language-design-plan.md) | Goals, language design, compiler architecture, roadmap and open questions |

## Integrating a crate

Every crate documents its full API, with tested examples, in rustdoc. Build and open it with:

```sh
cargo doc --workspace --no-deps --open
```

Add `--document-private-items` to read the internals as well. Start with
[`architecture.md`](architecture.md) to see which crate does what.
