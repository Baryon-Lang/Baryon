# NPL: New Programming Language

A compiled language that reads like Python and runs like Rust, C and Fortran, built so that
physics, mathematics and data science at any scale are routine work.

NPL is a working name; the final name is still open.

```python
# Planned syntax, not final.
def heat_step(u: Array[f64, 2], alpha: f64) -> Array[f64, 2]:
    out = u.copy()
    for i in range(1, u.shape[0] - 1):
        for j in range(1, u.shape[1] - 1):
            laplacian = u[i + 1, j] + u[i - 1, j] + u[i, j + 1] + u[i, j - 1] - 4.0 * u[i, j]
            out[i, j] = u[i, j] + alpha * laplacian
    return out
```

## Goals

- **Reads like Python.** Python's syntax wherever Python has a spelling for something, with static
  types.
- **Runs like Fortran.** Numeric kernels within 5% of `gfortran -O3`. Arrays never alias, so every
  loop can be vectorized.
- **Safe like Rust.** Ownership and no garbage collector; no use-after-free or data races in safe
  code.
- **Built for science.** N-dimensional arrays, complex numbers, parallel loops, reproducible
  random numbers and per-block floating-point control are part of the language, not add-ons.
- **Documented to learn from.** Every feature, crate and function is documented with tested
  examples, so the language can be learned and integrated from its documentation alone.

## Status

Phase 0: design and foundation. The compiler does not exist yet. The plan, its phases and its open
decisions are in [`docs/design/language-design-plan.md`](docs/design/language-design-plan.md).

## Getting started

Install [rustup](https://rustup.rs). The pinned toolchain installs itself on first use.

```sh
cargo xtask ci
```

That runs every check CI runs: format, architecture rules, documentation links, lints, tests and
API docs.

## Documentation

[`docs/README.md`](docs/README.md) lists every document by reader: learning the language,
contributing to the compiler, or integrating one of its crates.

## Repository layout

| Path | Contents |
| --- | --- |
| `crates/` | The compiler, one crate per responsibility |
| `xtask/` | Repository automation behind `cargo xtask` |
| `docs/README.md` | Map of all documentation |
| `docs/architecture.md` | Crate kinds and dependency rules |
| `docs/decisions/` | Decision records |
| `docs/design/` | The language design plan |
| `.github/` | CI workflow and pull request template |

## Contributing

Read [`CONTRIBUTING.md`](CONTRIBUTING.md). AI agents start with [`AGENTS.md`](AGENTS.md).

## License

[Apache License 2.0](LICENSE).
