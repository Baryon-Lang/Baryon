# Coding Baryon in VS Code

This page sets up Visual Studio Code for Baryon and walks through a first program in it: build the
compiler, install the extension, open a package, write code with errors underlined as you type,
read types and documentation on hover, jump to definitions and complete names, then run it and
test it. It is written for people trying release 0.2.0, and takes about fifteen
minutes.

Like the [beginner's guide](README.md), every program here is built and run in CI exactly as
written, and every output shown is what the command printed. If a step does not work for you,
that is a bug: please [open an issue](https://github.com/Baryon-Lang/Baryon/issues).

The compiler is a prototype. `baryon check` and the editor's diagnostics cover the whole language,
but `baryon run` builds only part of it; [What builds today](../reference/status.md) lists which
part. It is tested on macOS (Apple silicon) and Linux; Windows is untested.

## 1. What you need

| Tool | Why | How to get it |
| --- | --- | --- |
| [VS Code](https://code.visualstudio.com) 1.82 or later | The editor | Download it |
| [rustup](https://rustup.rs) | Builds `baryon`; installs the Rust version this repository pins on first use | The command on its home page |
| `clang` | Optimizes and links compiled programs; `baryon run` and `baryon test` need it | macOS: `xcode-select --install`. Linux: your distribution's `clang` package |
| [Node.js](https://nodejs.org) 18 or later | Packages the extension. Only needed once, and not if someone hands you the `.vsix` file | Download it, or your package manager |
| `git` | Gets the source | Comes with the macOS command line tools and every Linux distribution |

## 2. Build `baryon`

From a terminal:

```sh
git clone https://github.com/Baryon-Lang/Baryon.git
cd Baryon
cargo build --package driver --package runtime
```

The first build downloads the pinned Rust toolchain and takes a few minutes. It writes the
`baryon` program and the runtime library every compiled program links with, side by side in
`target/debug/`. Keep them together: `baryon` finds the runtime next to itself.

Note the full path of the program; the editor needs it in step 4:

```sh
echo "$PWD/target/debug/baryon"
```

To use `baryon` from any terminal, add that directory to your `PATH` in your shell's startup file,
such as `~/.zshrc` on macOS:

```sh
export PATH="$HOME/Baryon/target/debug:$PATH"
```

with `$HOME/Baryon` replaced by wherever you cloned the repository.

## 3. Install the extension

The extension is not on the Visual Studio Marketplace. If you were given `baryon-0.2.0.vsix`, skip
to the second step. Otherwise package it from the clone:

```sh
cd editors/vscode
npm ci
npx @vscode/vsce package
```

`npm ci` installs the extension's one dependency, Microsoft's language server client, at the
versions `package-lock.json` pins. The last command writes `baryon-0.2.0.vsix`.

Then install it: in VS Code, open the Extensions view (the four squares in the left bar), click
the `...` at its top right, choose **Install from VSIX...** and pick the file. From a terminal,
`code --install-extension baryon-0.2.0.vsix` does the same.

## 4. Point the extension at `baryon`

Open the settings (**Code > Settings > Settings** on macOS, **File > Preferences > Settings**
elsewhere), search for `baryon path`, and set **Baryon: Path** to the full path from step 2, for
example `/Users/you/Baryon/target/debug/baryon`.

A full path always works. The default, `baryon`, works only if VS Code itself can find `baryon` on
its `PATH`, and an editor started from the Dock or a launcher often does not see the `PATH` your
shell sets.

## 5. Open a package

A Baryon program is a package: a folder of `.byn` files, one of them `main.byn` with a function
`main`. The language server checks the first folder you open as one package, so open the package's
own folder, not a folder above it.

Make an empty folder `pendulum`, then **File > Open Folder...** and choose it. If VS Code asks
whether you trust the folder's authors, answer yes; the extension does not run in restricted mode.

## 6. Write a program

A pendulum swings back and forth with a period that, for small swings, is close to
2&pi;&radic;(L/g). This program checks that formula against a simulation. Create
`period.byn` in the folder with **File > New File...**:

File `pendulum/period.byn`:

```python
"""The period of a pendulum, from a simulation and from the small-angle formula."""

from math import sin, sqrt

G: Final = 9.81
PI: Final = 3.141592653589793


def simulated(length: f64, angle: f64, dt: f64) -> f64:
    """The period, in seconds, of a pendulum `length` metres long released at `angle` radians.

    Steps the equation of motion until the pendulum first reaches the bottom, which takes a
    quarter of a period.
    """
    theta = angle
    omega = 0.0
    time = 0.0
    while theta > 0.0:
        omega -= G / length * sin(theta) * dt
        theta += omega * dt
        time += dt
    return 4.0 * time


def small_angle(length: f64) -> f64:
    """The small-angle period, 2 pi sqrt(L / g), in seconds."""
    return 2.0 * PI * sqrt(length / G)
```

The bottom right of the window shows **Baryon** as the file's language. Keywords, strings and
numbers are colored as in Python, and Baryon's own names, such as `f64`, `Final`, `Array` and
`prange`, are colored as types and built-in functions. Typing `:` at the end of a `def` or `while`
line and pressing Enter indents the next line, and **Toggle Line Comment** (`Cmd+/` or `Ctrl+/`)
comments with `#`.

Now the program's entry point. Type this into a new file `main.byn`, and on purpose misspell
`length` in the second-to-last line:

File `pendulum/main.byn`:

```python
import period


def main():
    for i in range(1, 4):
        length = 0.5 * i
        swing = period.simulated(length, 0.1, 0.0001)
        formula = period.small_angle(lenght)
        print(f"{length:.1f} m: simulated {swing:.3f} s, formula {formula:.3f} s")
```

As soon as you stop typing, `lenght` is underlined in red. Hover over it to read the error, or open
the Problems view (**View > Problems**) to see every error in the package. It is the same error
`baryon check` prints in a terminal (**Terminal > New Terminal** opens one in the package's folder;
here it is run from the folder above):

```text
$ baryon check pendulum
error: undefined name `lenght`
 --> main.byn:8:38
  |
8 |         formula = period.small_angle(lenght)
  |                                      ^^^^^^ not defined in this scope

checked 2 file(s): 1 error(s)
```

Fix the spelling, and the underline and the Problems entry disappear:

File `pendulum/main.byn`:

```python
import period


def main():
    for i in range(1, 4):
        length = 0.5 * i
        swing = period.simulated(length, 0.1, 0.0001)
        formula = period.small_angle(length)
        print(f"{length:.1f} m: simulated {swing:.3f} s, formula {formula:.3f} s")
```

The editor knows what every name is, too. Hover over `simulated` in `period.simulated(...)`: a
box shows its signature and its documentation string, as `baryon doc` writes it.

```text
def simulated(length: f64, angle: f64, dt: f64) -> f64

The period, in seconds, of a pendulum `length` metres long released at `angle` radians.
...
```

Hover over `swing` and the box shows the type the compiler worked out for it, `swing: f64`,
although the program never wrote one. **Go to Definition** (`F12`, or `Cmd`-click on macOS and
`Ctrl`-click elsewhere) on `simulated` opens `period.byn` at the `def` line; it works the same on
variables, constants, fields, methods and enum members, in whichever file of the package they
are defined. Now type `period.` on a new line inside the loop: the editor offers what `period`
defines, `G`, `PI`, `simulated` and `small_angle`, each with its type or signature. After a
struct value and a dot it offers the struct's fields and methods, after an enum's name its
members, and anywhere else every name in scope. Delete the line again before going on.

## 7. Run it

In the terminal, run the package. Inside the package's folder that is `baryon run .`:

```text
$ baryon run pendulum
0.5 m: simulated 1.419 s, formula 1.419 s
1.0 m: simulated 2.007 s, formula 2.006 s
1.5 m: simulated 2.458 s, formula 2.457 s
```

`baryon run` compiled the package to machine code with `clang`, ran it, and removed the
executable. For a 0.1 radian swing, the simulation and the formula agree to a millisecond.

## 8. Test it

A test is a function marked `@test` that takes no arguments; it passes if it returns and fails if
an `assert` in it fails. Put the tests in their own file `tests.byn`:

File `pendulum/tests.byn`:

```python
import period


@test
def test_small_swings_match_the_formula():
    difference = period.simulated(1.0, 0.01, 0.0001) - period.small_angle(1.0)
    assert -0.001 < difference < 0.001, "the simulation disagrees with the formula"


@test
def test_wide_swings_are_slower():
    assert period.simulated(1.0, 1.0, 0.0001) > period.small_angle(1.0)
```

```text
$ baryon test pendulum
running 2 tests
test tests.test_small_swings_match_the_formula ... ok
test tests.test_wide_swings_are_slower ... ok

test result: ok. 2 passed; 0 failed
```

The second test is the physics the formula leaves out: a pendulum released at 1 radian swings more
slowly than the small-angle formula says. [Tests](../reference/tests.md) has the details of the
test runner.

## 9. Run and test from the editor

To run and test without typing commands, add a file `.vscode/tasks.json` to the package folder:

```json
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "baryon run",
      "type": "process",
      "command": "${config:baryon.path}",
      "args": ["run", "${workspaceFolder}"],
      "group": { "kind": "build", "isDefault": true },
      "problemMatcher": []
    },
    {
      "label": "baryon test",
      "type": "process",
      "command": "${config:baryon.path}",
      "args": ["test", "${workspaceFolder}"],
      "group": { "kind": "test", "isDefault": true },
      "problemMatcher": []
    }
  ]
}
```

**Terminal > Run Build Task** (`Cmd+Shift+B` or `Ctrl+Shift+B`) now runs the program, and
**Terminal > Run Task... > baryon test** runs the tests. Both use the program you set in
**Baryon: Path**.

## Troubleshooting

| What you see | Why, and the fix |
| --- | --- |
| The language in the bottom right says **Python** or **Plain Text** for a `.byn` file | The extension is not installed or not enabled; check the Extensions view. If your settings map `*.byn` to `python` in `files.associations`, remove that line |
| A notification says **Could not start `baryon lsp`** | **Baryon: Path** does not name the program. Set it to the full path from step 2 and run **Developer: Reload Window** from the Command Palette |
| Highlighting works but no error is ever underlined | The folder you opened is not the package. Open the folder that holds `main.byn`, or make the package the first folder of a multi-root workspace |
| Errors stay after you fix them, or the server stopped | Open **View > Output** and pick **Baryon** in the list to read the server's log, then **Developer: Reload Window**. If the server crashed, please open an issue with that log |
| `baryon run` says `` `clang` was not found `` | Install `clang` as in step 1 |
| `baryon run` says `` `clang` is version 14 `` | Install `clang` 15 or newer as in step 1 |
| `baryon run` says the runtime library `libruntime.a` is missing | The runtime was not built, or `baryon` was copied away from it. Run the `cargo build` command of step 2 again and use `baryon` from `target/debug/`, where it puts both |
| `baryon run` reports an error that ends in `not lowered yet` or names the backend | The program is valid Baryon that the prototype compiler cannot build yet. [What builds today](../reference/status.md) lists what does |
| You changed **Baryon: Path** and nothing happened | The server starts once per window; run **Developer: Reload Window** |

## Where to go next

- The [beginner's guide](README.md) goes further: types, errors, structs, `InOut` parameters and
  arrays, with a mass-on-a-spring simulation.
- [What builds today](../reference/status.md) says what the compiler builds in this release.
- The [extension's README](../../editors/vscode/README.md) explains how it highlights and which
  names it colors.
- Found a bug or something unclear? Please
  [open an issue](https://github.com/Baryon-Lang/Baryon/issues): it is how this release gets
  reviewed.
