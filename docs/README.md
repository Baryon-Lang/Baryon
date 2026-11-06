# Documentation

Every piece of documentation in the repository, by who it is for. If something you needed was
missing or unclear, that is a bug: open an issue.

## Learning the language

The language guide and reference will live here once the first compiler prototype runs. Until
then, the [design plan](design/language-design-plan.md) shows the planned syntax and semantics,
including a side-by-side comparison with Python.

## Contributing to the compiler

| Document | What it covers |
| --- | --- |
| [`ROADMAP.md`](../ROADMAP.md) | The source of truth: phases, work items, who is on what, decisions needed, ideas |
| [`CONTRIBUTING.md`](../CONTRIBUTING.md) | The binding rules: modularity, documentation, roadmap, cleanliness, tests, dependencies, workflow |
| [`AGENTS.md`](../AGENTS.md) | Extra rules for AI coding agents |
| [`architecture.md`](architecture.md) | Crate kinds, what may depend on what, and how stages connect |
| [`bench/README.md`](../bench/README.md) | The baseline benchmark suite: the kernels, how to run and record them, and the numbers NPL must match |
| [`decisions/`](decisions/README.md) | Every hard-to-reverse choice, why it was made, and what it costs |
| [`design/language-design-plan.md`](design/language-design-plan.md) | Goals, language design, compiler architecture, roadmap and open questions |

## Integrating a crate

Every crate documents its full API, with tested examples, in rustdoc. Build and open it with:

```sh
cargo doc --workspace --no-deps --open
```

Add `--document-private-items` to read the internals as well. Start with
[`architecture.md`](architecture.md) to see which crate does what.
