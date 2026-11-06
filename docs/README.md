# Documentation

Every piece of documentation in the repository, by who it is for. If something you needed was
missing or unclear, that is a bug: open an issue.

## Learning the language

The compiler only checks syntax, supported constructs, module names and names so far (`cargo run --package driver -- check
<directory>`), so the language is learned from its definition and from examples. A beginner's
guide will follow the first compiler prototype that runs programs.

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
