# A tour of the language

This page shows the language a piece at a time, for a reader who knows Python and has run a first
program with the [beginner's guide](README.md). Each section is a short program that builds and
runs today; CI builds and runs every one of them exactly as written, and checks that it prints
what this page shows.

Most of Baryon is Python with types. Four things are new, and the page points each out as it
comes: numbers have fixed sizes, every variable keeps one type, records are declared as
`Struct`s, and a value has one owner. [Differences from Python](../reference/differences-from-python.md)
lists every difference with its reason.

## 1. Values, strings and collections

File `basics/main.byn`:

```python
def main():
    # Numbers: int is a 64-bit integer, float a 64-bit float; f32, i32, u8 and the rest exist.
    n = 7
    x = 2.5
    y: f32 = 1.5
    print(n // 2, n % 2, n / 2, x ** 2, f64(y) + x)

    # Strings and f-strings.
    name = "argon"
    print(f"{name.upper()} has {len(name)} letters; pi is about {3.14159:.2f}")
    words = "H He Li Be".split()
    print(words, " / ".join(words), "He" in words)

    # Lists, slices and comprehensions.
    masses = [1.008, 4.0026, 6.94]
    masses.append(9.0122)
    print(len(masses), sum(masses), masses[-1], masses[1:3])
    squares = [i * i for i in range(5) if i % 2 == 0]
    print(squares)

    # Dicts.
    symbol_to_z = {"H": 1, "He": 2}
    symbol_to_z["Li"] = 3
    for symbol, z in symbol_to_z.items():
        print(symbol, z)
    print(symbol_to_z.get("Be", 0))

    # Tuples and unpacking.
    point = (1.0, 2.0)
    a, b = point
    a, b = b, a
    print(a, b)

    # A value that may be missing: T | None.
    found: int | None = None
    for i, word in enumerate(words):
        if word == "Li":
            found = i
    if found is not None:
        print("Li is at", found)
```

```text
$ baryon run basics
3 1 3.5 6.25 4.0
ARGON has 5 letters; pi is about 3.14
['H', 'He', 'Li', 'Be'] H / He / Li / Be True
4 20.9628 9.0122 [4.0026, 6.94]
[0, 4, 16]
H 1
He 2
Li 3
0
2.0 1.0
Li is at 2
```

Everything here reads as Python reads it. The differences are underneath:

- **Numbers have a size.** `int` is `i64` and `float` is `f64`. Mixing two sizes, such as an
  `f32` and an `f64`, is an error until one is converted with a call such as `f64(y)`, so
  precision never changes without a visible conversion.
- **A variable keeps one type.** `n = 7` makes `n` an `int` for the rest of its scope; assigning
  a `str` to it later is an error. The compiler works out most types itself, so annotations are
  needed only where it cannot, such as `found: int | None = None`.
- **A list holds one type.** `masses` is a `list[f64]`. A record of mixed types is a struct, the
  next section.
- **`T | None` must be checked before use.** After `if found is not None:`, `found` is an `int`
  inside the block. Using it without the check is a compile error, not a crash.

[Types](../reference/types.md) defines numbers, strings, lists, dicts and optionals in full.

## 2. Structs, enums and unions

A Python `@dataclass` is a `Struct` here. A value that is one of several kinds, each with its own
data, is a union of structs, taken apart with `match`.

File `kinds/main.byn`:

```python
from math import pi, sqrt


class Vector(Struct, Copy):
    """A vector in the plane, in metres."""

    x: f64
    y: f64

    def length(self) -> f64:
        return sqrt(self.x * self.x + self.y * self.y)

    def scale(self: InOut[Self], factor: f64):
        self.x *= factor
        self.y *= factor

    def zero() -> Vector:
        return Vector(0.0, 0.0)


class Circle(Struct, Copy):
    radius: f64


class Rectangle(Struct, Copy):
    width: f64
    height: f64


Shape = Circle | Rectangle


def area(shape: Shape) -> f64:
    match shape:
        case Circle(radius=r):
            return pi * r * r
        case Rectangle(w, h):
            return w * h


class Phase(Enum):
    SOLID = 1
    LIQUID = 2
    GAS = 3


def phase_of(kelvin: f64) -> Phase:
    if kelvin < 273.15:
        return Phase.SOLID
    return Phase.LIQUID if kelvin < 373.15 else Phase.GAS


def main():
    v = Vector(3.0, 4.0)
    v.scale(2.0)
    print(v.length(), Vector.zero())

    shapes: list[Shape] = [Circle(1.0), Rectangle(2.0, 3.0)]
    for shape in shapes:
        print(f"{area(shape):.4f}")

    print(phase_of(300.0), phase_of(500.0) == Phase.GAS)
```

```text
$ baryon run kinds
10.0 Vector(x=0.0, y=0.0)
3.1416
6.0000
Phase.LIQUID True
```

- **`class Vector(Struct, Copy)`** declares a struct: named, typed fields stored together, built
  with `Vector(3.0, 4.0)`. `Copy` makes it copy on assignment, like a number; leave it out for a
  struct that holds a large array, so assigning one moves it instead (section 4).
- **Methods read `self` by default.** A method that changes its struct says so:
  `self: InOut[Self]`. `def zero()` has no `self`, so it is called through the class,
  `Vector.zero()`, as a second constructor.
- **No inheritance.** A struct's bases are `Struct`, optionally `Copy`, and the traits it
  implements (section 3). Types share data by holding each other as fields.
- **A union** such as `Shape = Circle | Rectangle` holds exactly one of its members. `match`
  takes it apart by class, and must handle every member: add a `Triangle` to `Shape` and the
  compiler points at each `match` that needs a new `case`.
- **An `Enum`** is a set of named constants, as in Python.

[Structs](../reference/types.md#structs), [enums](../reference/types.md#enums) and
[unions](../reference/types.md#unions-of-structs) have the details.

## 3. Generics, traits and functions as arguments

A function that works for several types takes a type parameter in square brackets, as in Python
3.12. A bound after the colon says what the type must support. A function can also take another
function as an argument.

File `generic/main.byn`:

```python
class Body(Protocol):
    """Anything that knows its own kinetic energy, in joules."""

    def energy(self) -> f64: ...


class Particle(Struct, Copy, Body):
    mass: f64
    speed: f64

    def energy(self) -> f64:
        return 0.5 * self.mass * self.speed * self.speed


def total_energy[T: Body](bodies: list[T]) -> f64:
    total = 0.0
    for body in bodies:
        total += body.energy()
    return total


def largest[T: Number](values: list[T]) -> T:
    best = values[0]
    for value in values:
        if value > best:
            best = value
    return best


def integrate(f: Callable[[f64], f64], a: f64, b: f64, n: i64) -> f64:
    """The integral of `f` from `a` to `b` by the midpoint rule with `n` intervals."""
    h = (b - a) / f64(n)
    total = 0.0
    for i in range(n):
        total += f(a + (f64(i) + 0.5) * h)
    return total * h


def square(x: f64) -> f64:
    return x * x


def main():
    print(total_energy([Particle(2.0, 3.0), Particle(1.0, 4.0)]))
    print(largest([3, 9, 4]), largest([2.5, -1.0]))
    print(f"{integrate(square, 0.0, 3.0, 1000):.6f}")
```

```text
$ baryon run generic
17.0
9 2.5
8.999998
```

- **A trait** is a class whose last base is `Protocol`, listing methods. A struct implements it
  by naming it as a base, and the compiler checks that every method is there. Unlike Python's
  `Protocol`, having the methods is not enough: the struct must name the trait.
- **A bound** such as `[T: Body]` lets the function call `Body`'s methods on a `T`, and nothing
  else. `Number` and `Float` are built-in bounds for the number types.
- **Generic functions cost nothing at run time.** The compiler builds one copy of `largest` for
  `int` and one for `f64`, as C++ and Rust do. You never write the type at the call; it comes
  from the arguments.
- **`Callable[[f64], f64]`** takes a module-level function. The compiler builds a copy of
  `integrate` for each function passed, so the call to `f` is inlined and as fast as writing
  `x * x` in the loop. Lambdas and closures are not in the language yet.

[Generics](../reference/types.md#generics) and
[functions as arguments](../reference/types.md#functions-as-arguments) have the rules.

## 4. Who owns a value

This is the one part of Baryon with no Python equivalent, and the reason it needs no garbage
collector. Every value has one *owner*, a variable or field, and is freed when the owner goes
away. Three rules follow:

- **A function reads its arguments by default.** It *borrows* them for the call: no copy is
  made, and the caller keeps them.
- **A function that changes an argument says so**, with `InOut[T]`. Then the caller sees the
  change.
- **Assigning a value that is not `Copy`, such as an array, moves it.** The old name can no
  longer be used.

File `owners/main.byn`:

```python
def total(values: Array[f64, 1]) -> f64:
    """Reads `values` through a borrow: the caller keeps it."""
    return values.sum()


def normalize(values: InOut[Array[f64, 1]]):
    """Changes the caller's array in place, so that it sums to 1."""
    values /= values.sum()


def main():
    a = Array([1.0, 2.0, 5.0])
    print(total(a))
    normalize(a)
    print(a)
    b = a
    print(b.sum(), a.sum())
```

```text
$ baryon run owners
error: `a` was moved
  --> main.byn:17:20
   |
17 |     print(b.sum(), a.sum())
   |                    ^ read after the move
  ::: main.byn:16:9
   |
16 |     b = a
   |         - `a` moved here
   = note: to keep both, move a copy: `a.copy()`

```

In Python, `b = a` would make two names for one array, and a change through `b` would show
through `a`. Baryon refuses: two names for one array would stop the compiler proving that arrays
never overlap, which is what lets it vectorize every loop. The compiler says what to do instead.
Keep both with a copy:

File `owners/main.byn`:

```python
def total(values: Array[f64, 1]) -> f64:
    """Reads `values` through a borrow: the caller keeps it."""
    return values.sum()


def normalize(values: InOut[Array[f64, 1]]):
    """Changes the caller's array in place, so that it sums to 1."""
    values /= values.sum()


def main():
    a = Array([1.0, 2.0, 5.0])
    print(total(a))
    normalize(a)
    print(a)
    b = a.copy()
    b *= 2.0
    print(b.sum(), a.sum())
```

```text
$ baryon run owners
8.0
[0.125 0.25  0.625]
2.0 1.0
```

Numbers, `bool`s, enums and `Copy` structs copy on assignment, as in Python they seem to, so the
rules matter only for arrays, lists, dicts, strings and structs that hold them.
[Values and references](../reference/values-and-references.md) covers moves, borrows, `InOut`
and `Owned` parameters in full.

## 5. Errors

An error that a caller should handle, such as a file that is missing or text that is not a
number, is an exception, raised and caught as in Python. A function that can raise one says so in
its return type, `Raises[T, E]`, and the compiler checks that every error is either caught or
passed on. A bug, such as an index out of range, is a *panic* instead: it stops the program with
a message and a backtrace, and cannot be caught.

File `errors/main.byn`:

```python
class NegativeMass(Exception):
    """A mass below zero, in kilograms."""

    mass: f64


def parse_mass(text: str) -> Raises[f64, ValueError | NegativeMass]:
    mass = float(text)
    if mass < 0.0:
        raise NegativeMass(mass)
    return mass


def main():
    for text in ["1.5", "-2.0", "heavy"]:
        try:
            print(parse_mass(text))
        except NegativeMass as negative:
            print("negative:", negative.mass)
        except ValueError as bad:
            print("not a number:", bad.message)
```

```text
$ baryon run errors
1.5
negative: -2.0
not a number: could not convert string to float: 'heavy'
```

- An error type is a class with the base `Exception` and typed fields, like a struct.
- `float(text)` raises `ValueError` with Python's message.
- Give each `except ... as` its own name: a name keeps one type for its whole scope, so the two
  clauses cannot share `error` as they can in Python.
- `main` may raise too, as `def main() -> Raises[None, OSError]:`. An error that reaches the
  end of `main` prints its message and exits with status 1.

[Errors and panics](../reference/errors.md) has the full rules and the list of built-in errors.

## 6. Modules and packages

A package is a directory of `.byn` files; each file is a module, and a subdirectory is a package
of modules. `import physics.oscillator` and `from physics.oscillator import simulate` work as in
Python, and the program starts at `main` in `main.byn`. The
[beginner's guide](README.md#5-a-mass-on-a-spring) builds a package with two modules, and
[Syntax](../reference/syntax.md) defines imports.

The standard library is imported the same way: `math`, `random`, `stats`, `linalg`, `fft`,
`byplot`, `netCDF4`, `h5by`, `zlib`, `os`, `pathlib` and the rest. The next two pages use them:
[Arrays and numbers](arrays-and-numbers.md) and [Data, files and plots](data-files-and-plots.md).
