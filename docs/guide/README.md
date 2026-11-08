# Beginner's guide

This guide takes you from nothing to a tested physics program in Baryon: install the tool, run a
first program, let the compiler catch a mistake, write tests, and simulate a mass on a spring. It
assumes you can read Python; it does not assume you know Rust, C or Fortran.

Everything here runs today. Every program in this guide is built and run in CI exactly as written,
and every output shown is what the command printed, so if a step does not work for you, that is a
bug: please open an issue. The compiler is a prototype and does not build the whole language yet;
[What builds today](../reference/status.md) lists what it does.

## 1. Install `baryon`

You need [rustup](https://rustup.rs), which installs the Rust toolchain this repository pins, and
`clang`, which `baryon` uses to optimize and link programs. On macOS, `xcode-select --install`
provides `clang`; on Linux, install your distribution's `clang` package. Then, from a clone of this
repository:

```sh
cargo build --package driver --package runtime
export PATH="$PWD/target/debug:$PATH"
```

That builds `baryon` and the runtime library every program is linked with, side by side in
`target/debug`, and puts them on your `PATH` for this terminal. [The `baryon`
command](../reference/commands.md) has the details.

## 2. A first program

A Baryon program is a directory of `.byn` files, called a package. It starts at the function
`main` in the file `main.byn`. Make a directory `hello` with this file:

File `hello/main.byn`:

```python
def main():
    print("Hello, Baryon!")
    print("1 + 2 =", 1 + 2)
```

Every Baryon file is valid Python syntax, so any editor's Python mode highlights it. In VS Code,
the [Baryon extension](../../editors/vscode/README.md) also shows the compiler's errors as you
type; [Coding Baryon in VS Code](vscode.md) sets it up. Run the program with `baryon run` and the
package's directory:

```text
$ baryon run hello
Hello, Baryon!
1 + 2 = 3
```

`baryon run` compiled the program to machine code, ran it and removed it. To keep the executable,
use `baryon build hello hello/program` instead, then run `hello/program`.

## 3. Numbers, functions and loops

How long does a ball take to fall 10 metres? Here is a simulation that steps time forward in small
increments until the ball reaches the ground:

File `ball/main.byn`:

```python
G: Final = 9.81


def fall(height: f64, dt: f64) -> f64:
    """How long, in seconds, a ball dropped from `height` metres takes to reach the ground."""
    position = height
    velocity = 0.0
    time = 0.0
    while position > 0.0:
        velocity -= G * dt
        position += velocity * dt
        time += dt
    return time


def main():
    for i in range(1, 4):
        height = 10.0 * i
        print(f"from {height:4.1f} m: {fall(height, 0.001):.2f} s")
```

```text
$ baryon run ball
from 10.0 m: 1.43 s
from 20.0 m: 2.02 s
from 30.0 m: 2.47 s
```

It reads as Python, with three differences that make it fast:

- **Every parameter has a type.** `f64` is a 64-bit floating-point number, Python's `float`;
  `i64` is a 64-bit integer, Python's `int`, and you may write `float` and `int` if you prefer.
  Knowing the types is what lets the compiler keep numbers in registers instead of boxing them.
- **Local variables need no types.** `position`, `velocity` and `time` take the type of their
  first value, `f64`, and keep it.
- **`G: Final = 9.81` is a constant**, computed by the compiler. A file holds only constants,
  functions and classes; there are no global variables.

Numbers mix as in Python: `10.0 * i` multiplies a float by the integer `i` and gives a float.
f-strings format numbers with Python's format specifications, such as `:4.1f`. The string right
after `def` documents the function, as in Python; `baryon doc` turns those into pages.

## 4. Let the compiler catch mistakes

Suppose you forget the time step when calling `fall`:

File `ball/main.byn`:

```python
G: Final = 9.81


def fall(height: f64, dt: f64) -> f64:
    """How long, in seconds, a ball dropped from `height` metres takes to reach the ground."""
    position = height
    velocity = 0.0
    time = 0.0
    while position > 0.0:
        velocity -= G * dt
        position += velocity * dt
        time += dt
    return time


def main():
    for i in range(1, 4):
        height = 10.0 * i
        print(f"from {height:4.1f} m: {fall(height):.2f} s")
```

Python would start the program and fail when it reached that line. Baryon finds the mistake
before anything runs. `baryon check` reports every problem in a package without building it,
which makes it the quickest way to find mistakes as you write:

```text
$ baryon check ball
error: `fall` is missing the argument `dt`
  --> main.byn:19:40
   |
19 |         print(f"from {height:4.1f} m: {fall(height):.2f} s")
   |                                        ^^^^^^^^^^^^ missing arguments

checked 1 file(s): 1 error(s)
```

Each message names the file, line and column, and points at the code. Here a default value is the
better fix, so that callers who do not care about the time step need not pass one. A parameter
with a default, as in Python, may be left out:

File `ball/main.byn`:

```python
G: Final = 9.81


def fall(height: f64, dt: f64 = 0.001) -> f64:
    """How long, in seconds, a ball dropped from `height` metres takes to reach the ground."""
    position = height
    velocity = 0.0
    time = 0.0
    while position > 0.0:
        velocity -= G * dt
        position += velocity * dt
        time += dt
    return time


def main():
    for i in range(1, 4):
        height = 10.0 * i
        print(f"from {height:4.1f} m: {fall(height):.2f} s")
```

```text
$ baryon check ball
checked 1 file(s): 0 error(s)
```

```text
$ baryon run ball
from 10.0 m: 1.43 s
from 20.0 m: 2.02 s
from 30.0 m: 2.47 s
```

## 5. A mass on a spring

A mass of 1 kg on a spring of stiffness `k` oscillates. The velocity Verlet method steps it
through time, and keeps its energy almost constant over millions of steps, which is why molecular
dynamics codes use it. This program is bigger, so it gets a package of two modules: the physics
in `physics/oscillator.byn`, and `main.byn` to run it.

File `spring/physics/oscillator.byn`:

```python
"""A mass on a spring, stepped with the velocity Verlet method."""


class State(Struct, Copy):
    """Where the mass is, in metres, and how fast it moves, in metres per second."""

    position: f64
    velocity: f64

    def energy(self, stiffness: f64) -> f64:
        """The total energy, kinetic plus potential, for a mass of 1 kg."""
        kinetic = 0.5 * self.velocity * self.velocity
        return kinetic + 0.5 * stiffness * self.position * self.position


class Unstable(Exception):
    """The time step is too long for the spring: the method would blow up."""

    dt: f64


def step(state: InOut[State], stiffness: f64, dt: f64):
    """Advances `state` by one time step `dt`."""
    acceleration = -stiffness * state.position
    state.position += state.velocity * dt + 0.5 * acceleration * dt * dt
    new_acceleration = -stiffness * state.position
    state.velocity += 0.5 * (acceleration + new_acceleration) * dt


def simulate(
    start: State, stiffness: f64, dt: f64, steps: i64
) -> Raises[Array[f64, 1], Unstable]:
    """The position after each of `steps` time steps, starting from `start`."""
    if stiffness * dt * dt >= 4.0:
        raise Unstable(dt)
    state = start
    positions = Array.zeros(steps)
    for i in range(steps):
        step(state, stiffness, dt)
        positions[i] = state.position
    return positions
```

File `spring/main.byn`:

```python
from physics.oscillator import State, Unstable, simulate


def main():
    try:
        positions = simulate(State(1.0, 0.0), 4.0, 0.01, 400)
        for i in range(0, len(positions), 50):
            print(f"t = {0.01 * (i + 1):4.2f} s   x = {positions[i]:+.4f} m")
    except Unstable as error:
        print(f"a time step of {error.dt} s is too long")
```

```text
$ baryon run spring
t = 0.01 s   x = +0.9998 m
t = 0.51 s   x = +0.5234 m
t = 1.01 s   x = -0.4343 m
t = 1.51 s   x = -0.9926 m
t = 2.01 s   x = -0.6383 m
t = 2.51 s   x = +0.3029 m
t = 3.01 s   x = +0.9656 m
t = 3.51 s   x = +0.7405 m
```

The mass starts at 1 m and swings through `-1` m and back with a period of π seconds, as
`x = cos(2t)` predicts for `k = 4`. Four new ideas carry the program:

- **Structs.** `class State(Struct, Copy)` declares a record of named fields, like a Python
  dataclass, stored as two consecutive `f64`s with no object header. `Copy` makes assigning one
  copy it, as a number is copied.
- **`InOut` parameters.** Arguments are borrowed read-only by default, so a function cannot change
  its caller's values by surprise. `step` takes `InOut[State]` because changing the state is its
  job, and its signature says so.
- **Arrays.** `Array.zeros(steps)` makes an array of `steps` floats, and `positions[i]` indexes it.
  Every index is checked: an index out of range stops the program with a message instead of
  reading the wrong memory.
- **Errors.** `simulate` can fail, and its return type says so: `Raises[Array[f64, 1], Unstable]`
  returns an array or raises `Unstable`. `raise` and `try`/`except` work as in Python, and the
  compiler checks that every error a call can raise is handled somewhere.

## 6. Tests

Tests sit next to the code they check. A test is a function decorated `@test`, with no parameters,
that checks what it expects with `assert`. Velocity Verlet should conserve energy, and `simulate`
should refuse a time step that is too long, so add two tests to the end of
`physics/oscillator.byn`:

File `spring/physics/oscillator.byn`:

```python
"""A mass on a spring, stepped with the velocity Verlet method."""


class State(Struct, Copy):
    """Where the mass is, in metres, and how fast it moves, in metres per second."""

    position: f64
    velocity: f64

    def energy(self, stiffness: f64) -> f64:
        """The total energy, kinetic plus potential, for a mass of 1 kg."""
        kinetic = 0.5 * self.velocity * self.velocity
        return kinetic + 0.5 * stiffness * self.position * self.position


class Unstable(Exception):
    """The time step is too long for the spring: the method would blow up."""

    dt: f64


def step(state: InOut[State], stiffness: f64, dt: f64):
    """Advances `state` by one time step `dt`."""
    acceleration = -stiffness * state.position
    state.position += state.velocity * dt + 0.5 * acceleration * dt * dt
    new_acceleration = -stiffness * state.position
    state.velocity += 0.5 * (acceleration + new_acceleration) * dt


def simulate(
    start: State, stiffness: f64, dt: f64, steps: i64
) -> Raises[Array[f64, 1], Unstable]:
    """The position after each of `steps` time steps, starting from `start`."""
    if stiffness * dt * dt >= 4.0:
        raise Unstable(dt)
    state = start
    positions = Array.zeros(steps)
    for i in range(steps):
        step(state, stiffness, dt)
        positions[i] = state.position
    return positions


@test
def test_energy_is_conserved():
    state = State(1.0, 0.0)
    before = state.energy(4.0)
    for i in range(1000):
        step(state, 4.0, 0.01)
    drift = state.energy(4.0) - before
    assert -1e-4 < drift < 1e-4, "energy drifted"


@test
def test_long_steps_are_refused():
    try:
        positions = simulate(State(1.0, 0.0), 4.0, 1.5, 10)
        assert False, "a step of 1.5 s should be refused"
    except Unstable as error:
        assert error.dt == 1.5
```

`baryon test` builds every test in the package and runs each in its own process, so one failure
cannot stop the others:

```text
$ baryon test spring
running 2 tests
test physics.oscillator.test_energy_is_conserved ... FAILED
test physics.oscillator.test_long_steps_are_refused ... ok

failures:

---- physics.oscillator.test_energy_is_conserved ----
panic: assertion failed: -1e-4 < drift < 1e-4, "energy drifted"
  --> physics/oscillator.byn:51:5
backtrace, most recent call last:
  ...

test result: FAILED. 1 passed; 1 failed
```

The failed `assert` names its condition, its message and its line, followed by a backtrace of the
calls that led there, shown here as `...`. Did the method fail, or the test? The energy of a
Verlet integrator does not stay exactly constant: it wobbles by up to about `(ω dt)² / 4` of the
total, where `ω = √k` is 2 rad/s here, so by about `2e-4` J of 2 J. The method is fine; the
tolerance was too tight. Change the test's last line to allow for it:

File `spring/physics/oscillator.byn`:

```python
"""A mass on a spring, stepped with the velocity Verlet method."""


class State(Struct, Copy):
    """Where the mass is, in metres, and how fast it moves, in metres per second."""

    position: f64
    velocity: f64

    def energy(self, stiffness: f64) -> f64:
        """The total energy, kinetic plus potential, for a mass of 1 kg."""
        kinetic = 0.5 * self.velocity * self.velocity
        return kinetic + 0.5 * stiffness * self.position * self.position


class Unstable(Exception):
    """The time step is too long for the spring: the method would blow up."""

    dt: f64


def step(state: InOut[State], stiffness: f64, dt: f64):
    """Advances `state` by one time step `dt`."""
    acceleration = -stiffness * state.position
    state.position += state.velocity * dt + 0.5 * acceleration * dt * dt
    new_acceleration = -stiffness * state.position
    state.velocity += 0.5 * (acceleration + new_acceleration) * dt


def simulate(
    start: State, stiffness: f64, dt: f64, steps: i64
) -> Raises[Array[f64, 1], Unstable]:
    """The position after each of `steps` time steps, starting from `start`."""
    if stiffness * dt * dt >= 4.0:
        raise Unstable(dt)
    state = start
    positions = Array.zeros(steps)
    for i in range(steps):
        step(state, stiffness, dt)
        positions[i] = state.position
    return positions


@test
def test_energy_is_conserved():
    state = State(1.0, 0.0)
    before = state.energy(4.0)
    for i in range(1000):
        step(state, 4.0, 0.01)
    drift = state.energy(4.0) - before
    assert -1e-3 < drift < 1e-3, "energy drifted"


@test
def test_long_steps_are_refused():
    try:
        positions = simulate(State(1.0, 0.0), 4.0, 1.5, 10)
        assert False, "a step of 1.5 s should be refused"
    except Unstable as error:
        assert error.dt == 1.5
```

```text
$ baryon test spring
running 2 tests
test physics.oscillator.test_energy_is_conserved ... ok
test physics.oscillator.test_long_steps_are_refused ... ok

test result: ok. 2 passed; 0 failed
```

`baryon test` exits with status 0 when every test passes and 1 when one fails, so a CI job can run
it as it is. [Tests](../reference/tests.md) has the details.

## Where to go next

- [Coding Baryon in VS Code](vscode.md) sets up the editor, with errors underlined as you type and
  tasks that run and test a package.
- [Differences from Python](../reference/differences-from-python.md) lists every place a Python
  programmer will be surprised, with the reason.
- [Values and references](../reference/values-and-references.md) explains borrowing, `InOut`
  and moves, the one part of Baryon with no Python equivalent.
- [Arrays](../reference/arrays.md) covers `Array` and `Fixed`, the types numeric code is built
  on.
- The [example programs](../examples/README.md) show where the language is going: molecular
  dynamics, a heat equation solver, conjugate gradient, Runge-Kutta and Monte Carlo. They use
  features that do not build yet; [What builds today](../reference/status.md) says which.
- The [language reference](../reference/README.md) defines everything else.
