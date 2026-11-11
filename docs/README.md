# Documentation

Every piece of documentation in the repository, by who it is for. If something you needed was
missing or unclear, that is a bug: open an issue.

## Learning the language

Start with the [beginner's guide](guide/README.md): it installs `baryon`, runs a first program,
writes tests and simulates a mass on a spring, using only what builds today. To work in an
editor, [Coding Baryon in VS Code](guide/vscode.md) sets up VS Code and walks through a first
program there. CI runs every step of both as written.

The compiler is a prototype. `baryon check <directory>` reports every error the checker finds,
but `baryon build`, `baryon run` and `baryon test` compile only part of the language so far, and
all six [example programs](examples/README.md) build and run;
[What builds today](reference/status.md) lists, chapter by chapter, what builds.

[The `baryon` command](reference/commands.md) says how to build the tool and what each of its
commands does, including `baryon lsp`, which shows the same diagnostics, types and documentation on
hover, go to definition and completion in any editor with a Language Server Protocol client, and `baryon doc`, which writes a package's documentation strings
as Markdown pages. The [VS Code extension](../editors/vscode/README.md) highlights `.byn` files
and runs `baryon lsp` for you. The [release notes](../CHANGELOG.md) say what each release holds.

- The [language reference](reference/README.md) defines what a program may say and what it means.
  Python programmers should start with its
  [differences from Python](reference/differences-from-python.md).
- [`examples/`](examples/README.md) has six physics and mathematics programs written in the
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
| [`design/astrobaryon-roadmap.md`](design/astrobaryon-roadmap.md) | AstroBaryon, the astronomy library: scope, build order, prerequisite libraries, and the documentation each piece ships with, including the transition guide from AstroPy |

## Integrating a crate

Every crate documents its full API, with tested examples, in rustdoc. Build and open it with:

```sh
cargo doc --workspace --no-deps --open
```

Add `--document-private-items` to read the internals as well. Start with
[`architecture.md`](architecture.md) to see which crate does what.
