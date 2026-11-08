//! The documentation generator, end to end: programs go through the parser and the
//! `documentation` crate, and the Markdown pages are compared in full.
//!
//! These tests live in the driver because they need two implementations, and only an
//! application crate may depend on more than one (decision 0001).

use driver::Session;

/// The page of `main.byn` holding `text`, or the rendered problems that stopped it. The
/// program's first newline, which only lets a test start it on its own line, is dropped.
fn page(text: &str) -> String {
    let mut session = Session::new();
    let text = text.strip_prefix('\n').unwrap_or(text);
    if let Err(error) = session.add_file("main.byn", text) {
        return error.to_string();
    }
    match session.document() {
        Ok(modules) => modules
            .iter()
            .map(documentation::ModuleDocumentation::markdown)
            .collect(),
        Err(problems) => problems
            .iter()
            .map(|problem| diagnostics::render(problem, &session).to_string())
            .collect::<Vec<_>>()
            .join("\n\n"),
    }
}

#[test]
fn functions_and_classes_list_their_signatures_and_documentation() {
    let text = r#"
"""Particles in a box.

Units are reduced: mass 1, length sigma.
"""

Vec3 = Fixed[f64, (3,)]
"""A position or velocity."""

MASS: Final = 1.0


@layout("C")
class Particle(Struct):
    """One particle."""

    position: Vec3
    """Where it is."""
    velocity: Vec3
    _scratch: f64

    def speed(self) -> f64:
        """How fast it moves."""
        return 0.0

    def _reset(self) -> None:
        pass


def step(
    particles: InOut[Array[Particle, 1]],
    dt: f64,  # seconds
) -> None:
    """Moves every particle by `dt`.

    Uses forward Euler.
    """
    pass


def _helper() -> None:
    pass
"#;
    let expected = r#"# `main`

Particles in a box.

Units are reduced: mass 1, length sigma.

## `Vec3`

```python
Vec3 = Fixed[f64, (3,)]
```

A position or velocity.

## `MASS`

```python
MASS: Final = 1.0
```

## `Particle`

```python
@layout("C")
class Particle(Struct)
```

One particle.

### `Particle.position`

```python
position: Vec3
```

Where it is.

### `Particle.velocity`

```python
velocity: Vec3
```

### `Particle.speed`

```python
def speed(self) -> f64
```

How fast it moves.

## `step`

```python
def step(particles: InOut[Array[Particle, 1]], dt: f64) -> None
```

Moves every particle by `dt`.

Uses forward Euler.
"#;
    assert_eq!(page(text), expected);
}

#[test]
fn a_module_without_documentation_is_only_its_items() {
    assert_eq!(
        page("def area(r: f64) -> f64:\n    return r\n"),
        "# `main`\n\n## `area`\n\n```python\ndef area(r: f64) -> f64\n```\n",
    );
}

#[test]
fn a_syntax_error_stops_documentation() {
    assert_eq!(
        page("def area(r: f64 -> f64:\n    return r\n"),
        "error: expected `)`, found `->`\n --> main.byn:1:17\n  |\n1 | def area(r: f64 -> f64:\n  |                 ^^ expected `)`",
    );
}

/// Every example program documents without error, with a summary for the index.
#[test]
fn the_example_programs_document() {
    let mut session = Session::new();
    for (path, text) in [
        (
            "conjugate_gradient.byn",
            include_str!("../../../docs/examples/conjugate_gradient.byn"),
        ),
        (
            "heat_equation.byn",
            include_str!("../../../docs/examples/heat_equation.byn"),
        ),
        (
            "lennard_jones.byn",
            include_str!("../../../docs/examples/lennard_jones.byn"),
        ),
        (
            "lorenz_rk4.byn",
            include_str!("../../../docs/examples/lorenz_rk4.byn"),
        ),
        (
            "monte_carlo_pi.byn",
            include_str!("../../../docs/examples/monte_carlo_pi.byn"),
        ),
    ] {
        assert!(session.add_file(path, text).is_ok());
    }
    let modules = session.document().unwrap();
    assert_eq!(modules.len(), 5);
    assert!(modules.iter().all(|module| module.summary().is_some()));
    assert!(documentation::index(&modules).contains(
        "| [`lennard_jones`](lennard_jones.md) | Molecular dynamics: argon-like atoms in a \
         periodic box, integrated with velocity Verlet. |"
    ));
}
