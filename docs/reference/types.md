# Types

Every value has a type the compiler knows before the program runs. Function signatures say their
types; inside a function body, the compiler works them out.

## Numbers

| Type | What it holds | Python spelling it matches |
| --- | --- | --- |
| `i8`, `i16`, `i32`, `i64` | Signed integers of 8 to 64 bits | `int` is `i64` |
| `u8`, `u16`, `u32`, `u64` | Unsigned integers of 8 to 64 bits | |
| `f32`, `f64` | IEEE 754 binary floating point, 32 and 64 bits | `float` is `f64` |
| `complex64`, `complex128` | A pair of `f32` or `f64`: real and imaginary parts | `complex` is `complex128` |
| `bool` | `True` or `False` | `bool` |

`int`, `float` and `complex` are other names for `i64`, `f64` and `complex128`, so code that uses
Python's names gets the fast, common widths. The sizes are fixed because fixed widths are what let
a number live in a register; Python's unbounded integers cannot.

Floating-point arithmetic follows IEEE 754 exactly, with no reordering, unless a future
`with fastmath:` block allows it. Dividing a float by zero gives an infinity or NaN, as in IEEE
754 and NumPy; dividing an integer by zero panics.

### Number literals

A number literal takes the type its context needs
([decision 0009](../decisions/0009-number-mixing.md)): in `2 * x`, the `2` is an `f32` if `x` is
an `f32`. A literal with no context to decide takes a default: `int` for `7`, `float` for `7.0`
and `1e-3`, `complex` for `2j`. A literal that does not fit its type, such as `300` where a `u8`
is needed, is a compile error.

### Number mixing

These rules apply to `+ - * / // % **` and to comparisons.

1. If both operands have the same type, the result has that type, except that `/` on two integers
   gives an `f64`.
2. If one operand is an integer and the other a float, the integer converts to the float's type,
   and the result has the float's type.
3. If one operand is a float or integer and the other a complex number, the result is the complex
   type of matching width.
4. Any other mix is a compile error: two floats of different widths, two integers of different
   widths, or a signed integer with an unsigned one. The message suggests a conversion.

`//` rounds toward negative infinity and `%` takes the sign of the divisor, as in Python, so
`-7 // 2` is `-4` and `-7 % 2` is `1`. This costs one or two instructions over C's truncating
division, and keeps every Python formula correct when copied over. Integer `**` needs a
non-negative exponent; a negative one panics.

### Conversions

A conversion is a call to the target type: `f64(n)`, `i32(x)`, `u8(k)`. It is the only way to
change a number's width or kind.

| Conversion | Result |
| --- | --- |
| Integer to wider integer, integer to float | Exact, or the nearest float for integers beyond 2⁵³ |
| Integer to narrower integer | Panics if the value does not fit |
| Float to integer | Rounds toward zero, as Python's `int()` does; panics on NaN, an infinity, or a value that does not fit |
| `f64` to `f32` | Rounds to the nearest `f32` |

A conversion that would lose a value panics rather than wraps, because a silently wrapped number in
a simulation is a wrong answer with no error.

### Integer overflow

Until roadmap decision D.11 is made, the design plan's rule holds: arithmetic that overflows an
integer type panics in debug builds and wraps around in release builds.

## bool

`bool` is its own type, not a number: `True + 1` is an error. Conditions in `if`, `while`, `and`,
`or`, `not` and `assert` must be `bool`. Python lets any value stand in a condition (an empty list
counts as false); here, write the test out, such as `len(items) > 0`, so a condition never depends
on a rule the reader has to remember.

## Strings

`str` is immutable UTF-8 text. Version 0 supports string literals, f-strings, `+` to concatenate,
`==` and `!=`, `in`, `len`, which counts characters as Python does, and `print`. Text processing
beyond that arrives with the standard library (roadmap 1.13).

## Tuples

`(a, b, c)` is a tuple: a fixed number of values, each with its own type, written `tuple[int, f64]`.
Tuples unpack in assignments and `for` loops: `x, y, z = position`.

## Lists

`list[T]` is a growable sequence of values of one type `T`, stored contiguously on the heap: Rust's
`Vec`, C++'s `std::vector`. It supports `append`, `pop`, `len`, indexing, slicing, `in`, iteration
and list comprehensions. A list is for building up data of unknown length; convert it to an
`Array` with `Array(values)` for numeric work. An empty list needs its type written:
`columns: list[i64] = []`.

## Optional values

`T | None` holds either a `T` or `None`. Before using the `T`, the program must check, and the
check narrows the type:

```python
def describe(distance: f64 | None) -> str:
    if distance is None:
        return "unknown"
    return f"{distance:.2f} m"
```

Using a `T | None` as a `T` without the check is a compile error. There is no null pointer and no
sentinel value: `None` on a value type costs a flag, not an allocation.

## Structs

A struct is a record type: named fields, stored together, with value semantics.

```python
class Particle(Struct):
    """A point mass."""

    position: Fixed[f64, (3,)]
    velocity: Fixed[f64, (3,)]
    mass: f64

    def kinetic_energy(self) -> f64:
        return 0.5 * self.mass * self.velocity.dot(self.velocity)
```

- A class with the base `Struct` is a struct. It lists every field with a type annotation and no
  default.
- `Particle(p, v, 1.0)` builds one, with arguments in field order, by position or by name.
- Methods are `def` inside the class, with `self` first. How `self` is passed is in
  [Values and references](values-and-references.md#methods).
- Structs have no inheritance: no base besides `Struct` and, optionally, `Copy`. Share code
  between structs with functions and generics instead.
- A struct moves when assigned, unless it is declared `class Particle(Struct, Copy)`; see
  [copy types](values-and-references.md#copy-types).

Python programmers will recognize this as a `@dataclass`. It is written as a class with a base
rather than a decorator so that the base, not a convention, is what changes the semantics.

## Enums

```python
class Boundary(Enum):
    PERIODIC = 1
    REFLECTING = 2
    ABSORBING = 3
```

A class with the base `Enum` is an enumeration of named constants, as in Python. Enums are `Copy`
and can be compared with `==` and matched with `match`. Enums whose variants carry data wait on
roadmap decision D.10.

## Errors

A class with the base `Exception` is an error type: a struct that a fallible function can return
in place of its result. See [Errors and panics](errors.md).

## Type aliases

A module-level assignment whose right side is a type makes an alias. `Vec3` below is another name
for `Fixed[f64, (3,)]`, usable everywhere the full type is, including as a constructor:
`Vec3(1.0, 0.0, 0.0)`.

```python
Vec3 = Fixed[f64, (3,)]
```

## Functions as arguments

A parameter of type `Callable[[f64, Vec3], Vec3]` takes a function with that signature. In version
0, the argument must be the name of a module-level function, not a closure. The compiler generates
one copy of the receiving function per function passed to it, so the passed function is inlined
and the call costs nothing.

## Generics

Functions and structs take type parameters with Python 3.12's syntax. A bound after `:` names the
trait the type must satisfy.

```python
def rk4_step[S: VectorSpace](derivative: Callable[[f64, S], S], t: f64, state: S, h: f64) -> S:
    k1 = derivative(t, state)
    k2 = derivative(t + h / 2, state + h / 2 * k1)
    k3 = derivative(t + h / 2, state + h / 2 * k2)
    k4 = derivative(t + h, state + h * k3)
    return state + h / 6 * (k1 + 2 * k2 + 2 * k3 + k4)
```

Generics are compiled once per set of type arguments used (monomorphized), as in Rust and C++, so
they cost nothing at run time. A type argument is never written at the call; the compiler infers it
from the arguments.

### Traits

A trait names a set of operations a type supports. Version 0 has only the standard library's
traits; how user code declares new ones is roadmap decision D.9.

| Trait | A type satisfies it if it supports | Satisfied by |
| --- | --- | --- |
| `Number` | `+ - * /`, comparison, and literals | Every number type |
| `Float` | `Number`, plus the `math` functions | `f32`, `f64` |
| `VectorSpace` | `+` and `-` with itself, and `*` by an `f64` on either side | `f64`, `Fixed[f64, ...]`, `Array[f64, ...]` |
| `Copy` | Copying on assignment | See [copy types](values-and-references.md#copy-types) |

## Type inference

- Every parameter has a type annotation. The return type is annotated too, except that a function
  that returns nothing may leave it out.
- A local variable takes the type of the first value assigned to it. An annotation, `x: f32 = 0`,
  fixes the type instead.
- A variable keeps one type for its whole scope. Assigning a value of another type is an error,
  where Python would allow it; one name meaning two things in one function is a common source of
  bugs in long numeric code.
- Inference is local to one function body. A function's signature is all its callers see, so
  changing a body never changes a caller's types, which keeps incremental compilation fast.

## Scope and definite assignment

Variables have function scope, as in Python: one assigned inside an `if` or a loop is visible after
it. A variable may be used only where every path that reaches the use has assigned it; the compiler
checks this and names the path that does not.

```python
def classify(energy: f64) -> str:
    if energy < 0.0:
        label = "bound"
    elif energy > 0.0:
        label = "free"
    else:
        label = "critical"
    return label  # assigned on every path
```

After a `try` statement, a variable assigned in the `try` body is assigned only if every `except`
branch also assigns it or leaves the function.

## Constants

```python
N: Final = 64
ALPHA: Final = 0.1
DT: Final[f32] = 0.005
```

A module-level name annotated `Final` is a constant. Its value is computed by the compiler, so it
must be built from literals, other constants, arithmetic and the `math` functions. Its type is the
literal's default (`int`, `float`) unless written as `Final[T]`. Constants can be used where the
compiler needs a compile-time value, such as the shape of a `Fixed` array.

`Final` is Python's own `typing.Final`, so type checkers and editors already understand it.
