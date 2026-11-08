# Types

[What builds today](status.md#types) says which parts of this chapter the compiler builds into
programs yet.

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
([decision 0014](../decisions/0014-number-conversions-are-calls.md)): in `2 * x`, the `2` is an
`f32` if `x` is an `f32`. A literal with no context to decide takes a default: `int` for `7`,
`float` for `7.0` and `1e-3`, `complex` for `2j`. A literal that does not fit its type, such as
`300` where a `u8` is needed, is a compile error.

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

A conversion is a call to the target type: `f64(n)`, `i32(x)`, `u8(k)`
([decision 0014](../decisions/0014-number-conversions-are-calls.md)). It is the only way to change
a number's width or kind.

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

An f-string formats each value with a specification from Python's format mini-language, and
writes what Python writes:

```python
def report(step: i64, name: str, energy: f64) -> str:
    return f"step {step:4d}: {name} = {energy:10.4f} {{ok}}"
```

`report(7, "argon", -1.25)` returns `step    7: argon =    -1.2500 {ok}`. Fill, alignment, sign,
`#`, `0`, width, `,` and `_` grouping, precision and the type letters `d b o x X e E f F g G %`
and `s` all work as in Python, and a value with no specification is written as `print` writes it.
The compiler checks each specification against its value's type, so `{name:+}` on a `str` is a
compile error rather than a run-time one. Not supported yet: the `z` flag, the `n` and `c` type
letters, `!r`, `{value=}`, backslash escapes such as `\n` in an f-string's text, and a
specification built from other values, such as `{x:{width}}`.

A `str` is a pointer and a length. A literal points at the program's own copy of its text and is
never copied ([decision 0022](../decisions/0022-str-values.md)); an f-string, `+` and `s.copy()`
build new text, which the value owns and frees when it is dropped.

## Tuples

`(a, b, c)` is a tuple: a fixed number of values, each with its own type, written `tuple[int, f64]`.
Tuples unpack in assignments and `for` loops: `x, y, z = position`.

## Lists

`list[T]` is a growable sequence of values of one type `T`, stored contiguously on the heap: Rust's
`Vec`, C++'s `std::vector`. It supports `append`, `pop`, `copy`, `len`, indexing, slicing, `in`,
iteration and list comprehensions. `copy` makes a new list holding a copy of each element, which is
how to keep a list and also pass it on, since a list moves
([values and references](values-and-references.md#moves)). A list is for building up data of
unknown length; convert it to an `Array` with `Array(values)` for numeric work. An empty list
needs its type written: `columns: list[i64] = []`.

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

The compiler follows the program's branches to know where a variable has been checked:

- Inside `if x is not None:`, and after `if x is None:` whose body ends in `return`, `raise`,
  `break` or `continue`, `x` is a `T`.
- After an `if`, `x` is a `T` when every branch that carries on leaves it one, so
  `if x is None: x = 0.0` checks it too.
- `and` and `or` check their right side with what the left side tells:
  `x is not None and x > 0.0`. So does a conditional expression: `x if x is not None else 0.0`.
- `assert x is not None` checks `x` for the rest of the block.
- Assigning a `T` makes `x` a `T` until it is assigned `None` or another `T | None`.
- In a `match`, the cases after `case None:` see a `T`.
- A loop body may run many times, so a variable assigned anywhere in a loop is unchecked again
  at the start of the loop and after it.

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
- Structs have no inheritance
  ([decision 0015](../decisions/0015-no-class-inheritance.md)): no base besides `Struct` and,
  optionally, `Copy`. Types share behavior through traits and generic functions, and share data
  by holding one another as fields.
- A struct moves when assigned, unless it is declared `class Particle(Struct, Copy)`; see
  [copy types](values-and-references.md#copy-types).

Python programmers will recognize this as a `@dataclass`. It is written as a class with a base
rather than a decorator so that the base, not a convention, is what changes the semantics.

### Field layout

A struct stores its fields in the order they are declared, each at the next offset its alignment
allows, as a C struct does. The compiler never reorders them. An array of structs is therefore laid
out exactly as its author wrote it, and can be shared byte for byte with C, a NumPy structured
array or a binary file format.

Alignment can leave gaps. A field of 8 bytes starts at a multiple of 8, so a 1-byte `bool` before
it is followed by 7 unused bytes. Declaring fields from largest to smallest keeps the gaps
smallest:

```python
class Sample(Struct):
    """24 bytes: 7 unused after `valid`, 4 more at the end to keep the next element aligned."""

    valid: bool
    value: f64
    count: i32


class PackedSample(Struct):
    """16 bytes: the same fields, largest first, with 3 unused at the end."""

    value: f64
    count: i32
    valid: bool
```

Rust reorders fields to save space unless told not to; this language keeps the declared order
because data shared with other programs must match a layout someone wrote down, and a struct whose
size changes when the compiler improves would break every file written before. When another order
would save space, the compiler warns at the class name and names that order:

```text
warning: reordering the fields of `Sample` would save 8 bytes
 --> main.byn:1:7
  |
1 | class Sample(Struct):
  |       ^^^^^^ 24 bytes as declared, 16 reordered
  = note: declare the fields in the order `value, count, valid`; the compiler keeps the order written, so the layout matches C and `numpy`
  = note: if the order is fixed by a file format or a C library, decorate the struct `@layout("C")` to keep it without this warning
```

The order it names sorts the fields by alignment, largest first, keeping the written order among
fields of equal alignment; no order takes fewer bytes. A generic struct is checked through each
non-generic struct that holds an instance of it.

When the order is fixed on purpose, because the struct mirrors a record in a file format or a C
library, say so with `@layout("C")` ([decision 0017](../decisions/0017-layout-decorator.md)). The
struct keeps its fields and padding exactly as written, and the compiler does not warn:

```python
@layout("C")
class Header(Struct, Copy):
    """The 16-byte header of a trajectory file: 7 bytes of padding follow `version`."""

    version: u8
    frames: u64
```

`"C"` is the only layout, and `@layout` applies only to structs; anything else is a compile error.

## Enums

```python
class Boundary(Enum):
    PERIODIC = 1
    REFLECTING = 2
    ABSORBING = 3
```

A class with the base `Enum` is an enumeration of named constants, as in Python. Enums are `Copy`
and can be compared with `==` and `!=`, but not ordered with `<`. Enums whose variants carry data
wait on roadmap decision D.10.

A `match` must handle every value of its subject. Over an enum, that means a `case` for each
member or a final `case _:`; the compiler names a member that is missing, so adding a member to an
enum points at every `match` that needs a new case. The same holds for `bool` (`True` and `False`),
`T | None` (`None` and the values of `T`) and tuples of these, taken apart item by item. A number
or a string has too many values to list, so a `match` on one needs `case _:` or a capture. A `case`
with an `if` guard may not match, so it does not count towards handling every value.

```python
def wrap(boundary: Boundary, index: i64, n: i64) -> i64:
    match boundary:
        case Boundary.PERIODIC:
            return index % n
        case Boundary.REFLECTING | Boundary.ABSORBING:
            return min(index, n - 1)
```

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

## Default parameter values

A parameter may have a default value, written after its type as in Python. A call that leaves the
argument out gets the default:

```python
DT: Final = 0.25


def drift(position: f64, velocity: f64, dt: f64 = DT, steps: i64 = 8) -> f64:
    """Where a particle moving at a constant `velocity` is after `steps` steps of length `dt`."""
    for step in range(steps):
        position += velocity * dt
    return position


def main():
    print(drift(0.0, 2.0))               # 4.0: dt = 0.25, steps = 8
    print(drift(0.0, 2.0, 0.5))          # 8.0: steps = 8
    print(drift(0.0, 2.0, steps=2))      # 1.0: dt = 0.25
    print(drift(0.0, 2.0, 0.5, steps=3)) # 3.0
```

- The default has the parameter's type, and the compiler checks it as it would an argument: in
  `steps: i64 = 8` the literal is an `i64`, and in `dt: f64 = 1` the `1` is an `f64`. A default
  of the wrong type, such as `dt: f64 = "fast"`, is a compile error at the default.
- Once a parameter has a default, every parameter after it must have one too, as in Python, so
  a call that leaves arguments out leaves out the last ones. An argument after one left out is
  passed by name: `drift(0.0, 2.0, steps=3)`.
- A default may use literals, `Final` constants, arithmetic and calls, but not the function's
  other parameters, which do not exist yet when the default is needed: `dt: f64 = 1.0 / steps`
  is an error that names `steps`.
- Struct fields have no defaults: a struct is built with every field, so its layout and value are
  always written out where it is made.

Python evaluates a default once, when the `def` runs, and shares that one value between every call
that leaves the argument out; a default list that a call appends to keeps the item for the next
call. A default that is a literal or a `Final` constant has the same value whenever it is
evaluated, so for those the question does not arise. When any other default, such as
`Array.zeros(3)` or a call to a function, is evaluated is not decided yet; it is roadmap decision
D.15, listed under [open questions](README.md#open-questions). Until it is decided,
`baryon check` accepts such a default, but `baryon build` reports a call that leaves its argument
out as not built yet.

## Functions as arguments

A parameter of type `Callable[[f64, Vec3], Vec3]` takes a function with that signature. In version
0, the argument must be the name of a module-level function, not a closure. The compiler generates
one copy of the receiving function per function passed to it, so the passed function is inlined
and the call costs nothing ([decision 0026](../decisions/0026-functions-as-arguments.md)).

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
from the arguments. Each type parameter takes the type of the first argument that fixes it, reading
left to right and number literals last, so `largest(x, 0)` with `x: f32` makes the `0` an `f32`.
Every other argument must then have the same type: `largest(x, n)` with `x: f64` and `n: i64` is an
error, as in non-generic code. A generic struct is built the same way: `Pair(1.5, 2.5)` is a
`Pair[f64]`.

Inside a generic function, a type parameter is its own type: a `T` matches only `T`, and only the
operations its bound allows apply to it. A number literal takes the type `T` under `Float`; under
`Number` only an integer literal does, since `T` may be an integer type, so `x * 0.5` needs
`[T: Float]`. Under `VectorSpace` a literal is an `f64`, which is what scales a vector.

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
branch also assigns it or leaves the function. A loop body may run zero times, so a variable first
assigned inside a loop is not assigned after it. A `match` that handles every value counts like an
`if` with an `else`.

A function that returns a value must not reach its end: every path ends in `return` or `raise`.
`while True:` with no `break` counts as never ending, so a search loop that returns from inside
needs no `return` after it.

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
