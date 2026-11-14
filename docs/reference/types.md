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
754 and NumPy: `/` and `//` give an infinity with the sign of the quotient, or NaN when both sides
are zero, and `%` gives NaN. Dividing an integer by zero with `//` or `%` panics, for two numbers
and for arrays alike.

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

### Rounding

`round(x)` is the nearest integer to `x`, an `int`, with a tie going to the even neighbour, as in
Python: `round(2.5)` is `2`. `round(x, digits)` keeps `x`'s type and gives the multiple of
`10 ** -digits` nearest `x`, also with ties to even; a negative `digits` rounds to tens, hundreds
and so on. A float rounds its exact binary value, as Python does, not the shorter text `print`
shows: `2.675` is stored a little below 2.675, so `round(2.675, 2)` is `2.67`, while `0.125` is
exact and rounds to the even `0.12`. An integer with zero or more digits is itself.

```python
def main():
    print(round(2.675, 2), round(0.125, 2), round(1250.0, -2))   # 2.67 0.12 1200.0
    print(round(1350, -2), round(2.5))                         # 1400 2
```

### Integer overflow

Arithmetic whose exact result does not fit its integer type *overflows*. A debug build, the
default for `baryon build`, `run` and `test`, stops the program with a panic that names the
operator and the line; a release build, `--release`, wraps around in two's complement, as C and
Rust's release builds do ([decision 0030](../decisions/0030-integer-overflow.md)):

```python
def double(n: i64) -> i64:
    return n + n
```

`double(9223372036854775807)` panics with ``integer overflow in `+` `` in a debug build and returns
`-2` in a release build. `+`, `-`, `*`, `**`, their in-place forms `+=`, `-=`, `*=`, `**=`, and `-x`
on a signed integer are checked, wherever they appear: in a generic function called with an integer type,
element by element in whole-array arithmetic such as `a + b` on integer arrays, and inside
integer `a.sum()` and `v.dot(w)`. `abs` of the type's minimum, `abs(-9223372036854775808)`, does
not fit either and panics with ``integer overflow in `-` ``. A result that differs between the two builds is an overflow the debug build
would have reported, so test in a debug build and time in a release build.

### Complex numbers

A complex number holds two floats: its *real part* and its *imaginary part*. `complex128`, also
spelled `complex`, has `f64` parts, the same as Python's `complex`; `complex64` has `f32` parts,
the same as `numpy.complex64`. Write one with an imaginary literal, a number followed by `j`, or
build one from its parts with `complex(re, im)`:

```python
def impedance(r: f64, l: f64, c: f64, omega: f64) -> complex:
    return complex(r, omega * l - 1 / (omega * c))


def main():
    z = impedance(50.0, 0.1, 1e-5, 2000.0)
    current = 10.0 / z
    print(z, abs(current), (10.0 * current.conjugate()).real / 2)
```

This prints `(50+150j) 0.0632455532033676 0.1`, as Python does: the impedance of a resistor, a
coil and a capacitor in series at angular frequency `omega`, the amplitude of the current a 10 V
source drives through it, and the power it draws.

| Operation | Result |
| --- | --- |
| `z + w`, `z - w`, `z * w`, `z / w`, `-z` | A complex number, computed as Python computes it, so the bits match |
| `z ** w` | A complex number: repeated multiplication for a whole exponent of at most 100 in size, polar form otherwise, as in Python |
| `z == w`, `z != w` | A `bool`: both parts equal, or either part different |
| `z.real`, `z.imag` | The part, a float of the parts' width: `f64` for a `complex128` |
| `z.conjugate()` | The same number with its imaginary part negated |
| `abs(z)` | The modulus, `math.hypot(z.real, z.imag)`, a float |
| `complex(re, im)`, `complex64(re, im)` | A complex number of that type from two real numbers, each converted to the parts' type |
| `complex(x)` | `x` as a complex number with imaginary part `0.0`, as for any [conversion](#conversions) |

A real number mixes with a complex number by [rule 3](#number-mixing): `2 * z` converts `2` to
`2+0j` first, so `1.5 - 2j` is `(1.5-2j)` and an `f64` mixes with a `complex128`, an `f32` with a
`complex64`. `complex64` arithmetic works in `f32`, as NumPy's does.

The parts of a complex number are read, never assigned: it is a value, like a float, so
`z.real = 1.0` is an error; write `z = complex(1.0, z.imag)`. As in Python, there is no `<`,
`<=`, `>` or `>=`, no `//` or `%`, no `round`, `min` or `max`, and no conversion to a real
number: `f64(z)` is an error that suggests `z.real` or `abs(z)`.

Dividing by zero and raising zero to a negative or complex power follow the rule for floats:
they give IEEE 754 results, as NumPy does, where Python raises `ZeroDivisionError`. `z / 0`
divides each part by `+0.0`, so `(1+1j) / 0` is `inf+infj` and `1j / 0` is `nan+infj`, and
`0j ** -1` is `nan+nanj`. A result too large for the parts' type has infinite parts where
Python raises `OverflowError`.

`print(z)` writes what Python's `print` writes: each part as `print` writes a float, but
without the `.0` of a whole number, the imaginary part always with its sign, all in
parentheses, as `(1.5-2j)`;
a number whose real part is `+0.0` is written as its imaginary part alone, `2j`. `-2j` prints
`(-0-2j)`, as in Python, since negating `2j` negates its real part `0.0` too. A `complex64`
prints its parts' fewest `f32` digits, `(0.1+0.2j)`, as NumPy does.

In an f-string, `f"{z}"` writes what `print(z)` writes, and a format specification works as
Python's does for a complex number: a type letter formats each part with it and drops the
parentheses, so `f"{z:.2f}"` of `1.5-2j` is `1.50-2.00j`; a sign, `,` or `_` grouping and a
precision apply to each part, and a width, fill and alignment to the whole. The imaginary part
always has its sign, and `#` keeps a point in each part: `f"{z:#}"` of `1+2j` is `(1.+2.j)`. As
in Python, `%`, `0` padding and `=` alignment cannot format a complex number.

```python
def impedance(resistance: f64, reactance: f64) -> str:
    z = complex(resistance, reactance)
    return f"Z = {z:.1f} ohm, |Z| = {abs(z):.2f}"
```

`impedance(3.0, -4.0)` returns `Z = 3.0-4.0j ohm, |Z| = 5.00`.

## bool

`bool` is its own type, not a number: `True + 1` is an error. Conditions in `if`, `while`, `and`,
`or`, `not` and `assert` must be `bool`. Python lets any value stand in a condition (an empty list
counts as false); here, write the test out, such as `len(items) > 0`, so a condition never depends
on a rule the reader has to remember.

## Strings

`str` is immutable UTF-8 text. Version 0 supports string literals, f-strings, `+` to concatenate,
`==` and `!=`, `in`, `len`, which counts characters as Python does, and `print`. `<`, `<=`, `>` and
`>=` order strings as Python does, character by character by code point, with a string before any
longer string it starts: `"Z" < "a"`, `"ab" < "abc"`. Comparing, `in` and `len` each read the
string, so they take time in proportion to its length.

```python
def is_noble(element: str) -> bool:
    return element in "He Ne Ar Kr Xe Rn" and len(element) == 2
```

### Indexing, slicing and methods

`text[i]` is the character at position `i`, as a one-character `str`, and `text[i:j:k]` is a new
`str` of the characters the range takes. Positions count characters, as in Python, so
`"Ångström"[5]` is `"r"`. The rules are a list's: a negative literal counts from the end,
`text[-1]` is the last character, and an index or a bound outside the text panics, where Python
would raise `IndexError` or cut the slice short
([differences from Python](differences-from-python.md)). UTF-8 stores characters in one to four
bytes, so finding position `i` reads the text from its start: indexing costs time in proportion
to the length, not constant time. A loop that visits every character should be `for c in text`,
which reads the text once.

These methods work as Python's do. Each reads the text and returns a new value; none changes it.

| Method | Gives |
| --- | --- |
| `text.split(sep)` | A `list[str]` of the pieces between each `sep`; two `sep`s together give an empty piece. An empty `sep` panics |
| `text.split()` | A `list[str]` of the words that runs of whitespace separate, with no empty word |
| `text.strip()`, `lstrip()`, `rstrip()` | The text without whitespace at both ends, the start or the end |
| `text.strip(chars)`, `lstrip(chars)`, `rstrip(chars)` | The same, removing any of the characters of `chars` |
| `sep.join(pieces)` | The `str`s of the `list[str]` `pieces` with `sep` between each two |
| `text.startswith(prefix)`, `text.endswith(suffix)` | A `bool` |
| `text.find(sub)` | The position of the first `sub`, in characters, or -1 |
| `text.replace(old, new)` | The text with every `old` replaced by `new` |
| `text.lower()`, `text.upper()` | The text in lower or upper case, by Unicode's full case mapping, so `"ß".upper()` is `"SS"` |
| `text.copy()` | A new `str` with the same text |

Whitespace is what Python's `str.isspace` accepts: spaces, tabs, line ends and the other Unicode
spaces. Every result is new text the value owns, never a view into the original, so it outlives
the string it came from; making it costs one pass over the text.

```python
def symbols(line: str) -> list[str]:
    """`" ar, he ,NE"` gives `['AR', 'HE', 'NE']`."""
    return [entry.strip().upper() for entry in line.split(",")]
```

`int(text)` and `float(text)` read a number from text as Python's do: whitespace around it, a
sign, and underscores between digits, as in `" -1_024\n"`, and for `float` an exponent and
`inf`, `infinity` and `nan` in any case. Text that is not a number raises
[`ValueError`](errors.md#error-types-the-language-provides) with Python's message, such as
`invalid literal for int() with base 10: '12a'`, so a function that reads numbers handles it or
lists it in its `Raises` ([decision 0037](../decisions/0037-reading-numbers-from-text.md)).
Only `int` and `float` read text; `f32(text)` is an error.

```python
def total(line: str) -> Raises[float, ValueError]:
    """`"1.5, 2.5,3"` gives `7.0`; `"1.5,x"` raises `ValueError`."""
    result = 0.0
    for field in line.split(","):
        result += float(field)
    return result
```

`text.encode()` gives the text's UTF-8 bytes as a new `Array[u8, 1]`, and `data.decode()`, on
an `Array[u8, 1]`, gives them back as a `str`
([decision 0064](../decisions/0064-utf-8-text-from-bytes.md)). Bytes that are not UTF-8 raise
`ValueError` with Python's message, such as
`'utf-8' codec can't decode byte 0xff in position 0: invalid start byte`; Python raises
`UnicodeDecodeError`, which `except ValueError` also catches there. Both take `encoding="utf-8"`
and no other encoding, which is a compile error, and `decode` takes no `errors=`. Each costs one
pass over the bytes and one copy. They turn the output of
[`gzip.decompress`](compression.md#gzipdecompress) and the other codecs into text:

```python
def first_line(data: Array[u8, 1]) -> Raises[str, ValueError]:
    """The bytes of `"T = 293.15 K\nP = 1 bar\n"` give `"T = 293.15 K"`."""
    return data.decode().split("\n")[0].copy()
```

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
letters, `!r`, `{value=}`, `\N{name}` escapes, and a
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
([values and references](values-and-references.md#moves)). A slice, `values[1:n]`, is a new list
too, as in Python, unlike an array slice, which is a view; its bounds follow the rules of
[array slicing](arrays.md#slicing), so a bound past the end panics. Assigning to a slice,
`values[i:j] = more`, replaces the elements in the range with copies of those of the list `more`,
which may be longer or shorter, so the list grows or shrinks as in Python; `values[i:i] = more`
inserts at `i`, and `more` stays as it was. It rebuilds the list, one pass over both. A slice
with a step, `values[::2] = more`, cannot be assigned to yet. A list is for building up data of
unknown length; convert it to an `Array` with `Array(values)` for numeric work. An empty list
needs its type written: `columns: list[i64] = []`.

A list's items share one type, so `[1, "a"]` is a compile error. `None` is the one exception:
items of one type `T` and `None` make a `list[T | None]`, a list of
[optional values](#optional-values), with no annotation. `[1.5, None, 2.5]` is a
`list[f64 | None]`, wherever it is written and whichever item comes first. Two types other
than `None` stay an error, with or without a `None` beside them, and so does a `None` in a list
whose written type has no room for one, such as `values: list[f64] = [1.5, None]`.

```python
def main():
    levels = [1, 2, 3, 4, 5]
    levels[1:3] = [9, 8, 7]
    levels[0:0] = [0]
    print(levels)  # [0, 1, 9, 8, 7, 4, 5]
```

`sum(values)`, `min(values)` and `max(values)` reduce a list, or a rank-1 array, of numbers.
`sum(values, start)`, or `sum(values, start=...)`, starts from `start`, and its type is the common
type of the elements and the start, as `+` mixes them. A list sums as CPython 3.12 sums it, bit
for bit: integers exactly, each `+` checked for overflow; floats with Neumaier's compensation,
which keeps the rounding error of each addition and adds it back once at the end, so
`sum([1e16, 1.0, -1e16])` is `1.0` and ten `0.1`s sum to `1.0`. An array sums as
[`a.sum()`](arrays.md#the-order-sum-adds-in) does, in NumPy's pairwise order, so array code
matches NumPy. `min` and `max` of an empty list panic, and a NaN anywhere makes them NaN, as for
an [array](arrays.md#methods).

`sorted(values)` is a new list of a list's elements from the smallest, and
`sorted(values, reverse=True)` from the largest; the list itself does not change. Numbers sort by
value and `str`s by code point, as Python orders them. The sort is stable: equal elements keep
their order, from the largest too. Every float NaN goes last, either way, as `numpy.sort` puts it.
It copies the list, then takes O(n log n) comparisons.

```python
def main():
    energies = [0.1, 0.2, 0.3]
    print(sum(energies), sum([1, 2, 3], 10))   # 0.6 16
    print(min(energies), max([3, 1, 2]))       # 0.1 3
    print(sorted(["neon", "argon"]))             # ['argon', 'neon']
```

## Dictionaries

`dict[K, V]` maps keys of type `K` to values of type `V`, as Python's `dict` does: a hash table
that keeps its entries in the order they were first added
([decision 0035](../decisions/0035-dict.md)). A key is an integer, `bool`, `str`, enum or a tuple
of them. A float cannot be a key, because floats are not compared exactly (`0.1 + 0.2` is not
`0.3`), so `{0.3: "x"}[0.1 + 0.2]` would miss; bin the value to an integer first. A struct cannot
be a key either; use a tuple of its fields.

```python
def main():
    masses = {"Ar": 39.948, "He": 4.0026}  # dict[str, f64], from the first entry
    masses["Ne"] = 20.180                  # a new key goes last
    masses["He"] = 4.003                   # an existing key keeps its place
    print(len(masses), masses["Ne"], "Xe" in masses)  # 3 20.18 False
    print(masses.get("Kr", 83.798))        # 83.798: the default, the dict is unchanged
    print(masses.pop("Ar"))                # 39.948, and the entry is gone
    for name, mass in masses.items():      # He 4.003, then Ne 20.18
        print(name, mass)

    counts: dict[int, int] = {}            # an empty dict needs its type written
    for i in range(10):
        k = i % 3
        if k in counts:
            counts[k] += 1                 # updates the entry in place
        else:
            counts[k] = 1

    squares = {i: i * i for i in range(5) if i != 2}  # {0: 0, 1: 1, 3: 9, 4: 16}
    lengths = {name: len(name) for name in ["Ar", "Ne"]}
    seen: dict[str, int] = {}
    for word in ["a", "b", "a"]:
        seen[word] = seen.setdefault(word, 0) + 1
    seen.update(lengths)                   # sets "Ar" and "Ne", keeps "a" and "b"
    del seen["a"]                          # removes the entry; a missing key panics
    seen.clear()                           # empty, and still a dict[str, int]
```

| Operation | Meaning |
| --- | --- |
| `{k: v, ...}`, `{}` | A display. The types come from the annotation or the first entry |
| `d[k]` | The value for `k`, read, written or updated in place. A missing key panics: `key 'Xe' is not in the dict` |
| `d[k] = v` | Sets the value, adding the key last when it is missing |
| `k in d`, `k not in d` | Whether the key is there |
| `len(d)` | The number of entries |
| `d.get(k)`, `d.get(k, default)` | A copy of the value, or `None` or the default when the key is missing. Values must copy or be `str`, or tuples of them; a `list` or an array would be a hidden large copy, so ask with `in` and read `d[k]` |
| `d.pop(k)`, `d.pop(k, default)` | Takes the entry out and gives its value; a missing key panics, or gives the default |
| `d.setdefault(k, default)` | A copy of the value for `k`, after adding `k` with `default` when it is missing. Values must copy or be `str`, as for `get` |
| `d.update(other)` | Sets each entry of `other`, a dict of the same type, in `other`'s order |
| `d.clear()` | Removes every entry |
| `del d[k]` | Removes the entry; a missing key panics |
| `d.copy()` | A new dict with the same entries |
| `{k: v for x in e if c}` | A dict comprehension: one entry per pass, a repeated key keeping its first place and its last value |
| `for k in d`, `d.keys()`, `d.values()`, `d.items()` | The keys, values or `(key, value)` pairs, in insertion order |

A loop borrows the dict, so its body cannot add, set or pop entries of the same dict; loop over a
copy, `for k in d.copy():`, to change the original. `keys()`, `values()` and `items()` are only for
`for` loops. A loop variable borrows a key or value that does not copy, such as a `list` value,
so keep a copy to store it elsewhere: `out.append(v.copy())`. A key is copied in by `d[k] = v`,
`setdefault` and a comprehension, so the variable it came from stays usable, as in Python. A dict
moves like a list ([values and references](values-and-references.md#moves)). Lookups hash with a
fixed seed, so a program behaves the same on every run. `popitem`, `fromkeys`, `|` and `**` in a
display are not in the language yet.

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
- A method called through its class, as Python calls a function found on a class, takes no
  receiver: each argument goes to the parameter at its position. A `def` without `self` is a
  function of the class, such as a second constructor, and `Particle.kinetic_energy(p)` is
  `p.kinetic_energy()`. A [generic](#generics) struct's methods are called through its class
  the same way, `Pair.both(1.5)`.

  ```python
  class Point(Struct):
      x: f64
      y: f64

      def origin() -> Point:
          return Point(0.0, 0.0)

      def on_axis(x: f64) -> Point:
          return Point(x, 0.0)


  start = Point.origin()
  end = Point.on_axis(2.5)
  ```
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
and can be compared with `==` and `!=`, but not ordered with `<`. A value that is one of several
kinds, each with its own data, is a [union of structs](#unions-of-structs) instead.

A `match` must handle every value of its subject. Over an enum, that means a `case` for each
member or a final `case _:`; the compiler names a member that is missing, so adding a member to an
enum points at every `match` that needs a new case. The same holds for `bool` (`True` and `False`),
`T | None` (`None` and the values of `T`), [unions](#unions-of-structs) (each member), structs
taken apart by class patterns, and tuples of these, taken apart item by item. A number
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

## Unions of structs

A value that is exactly one of several kinds, each kind with its own data, is a *union*: each kind
is an ordinary struct, and the union joins them with `|`, usually under a [type alias](#type-aliases)
([decision 0032](../decisions/0032-sum-types-are-unions-of-structs.md)). Rust calls this an enum
with data; typed Python writes it the same way as here.

The example program [`rod_boundaries.byn`](../examples/rod_boundaries.byn) puts these boundary
conditions to work on a heated rod.

```python
class Periodic(Struct, Copy):
    pass


class Dirichlet(Struct, Copy):
    value: f64


class Neumann(Struct, Copy):
    flux: f64


class Robin(Struct, Copy):
    a: f64
    b: f64
    g: f64


Boundary = Periodic | Dirichlet | Neumann | Robin
```

- A union's members are structs, error types, enums and `None`. A number, string, array or type
  parameter is not a member in version 0, and neither is a generic struct used with a type
  parameter. `T | None` with one `T` of any type is the [optional value](#optional-values) it
  always was.
- Order and repeats do not matter, and a union inside a union is flattened:
  `Dirichlet | Periodic` is the same type as `Periodic | Dirichlet`, and `Hit | None` with
  `Hit = Wall | Side` is `Wall | Side | None`.
- A kind with no data is a struct with no fields, made with `Periodic()`.

A member's value is accepted wherever its union is expected:

```python
def solve(left: Boundary, right: Boundary) -> Array[f64, 1]: ...

temperature = solve(Dirichlet(300.0), Neumann(0.0))
```

The reverse needs a check: a `Boundary` is never used as a `Dirichlet` until a `match` has shown
it is one. A union is not accepted where a wider union is expected yet, such as a
`Periodic | Dirichlet` passed as a `Boundary`.

`match` takes a union apart with class patterns. A class pattern names a struct and matches its
fields by position, in declaration order, or by keyword; fields it does not name match anything.
A capture binds the field's value.

```python
def ghost_value(boundary: Boundary, inside: f64, dx: f64) -> f64:
    """The temperature one cell outside the edge that enforces `boundary`."""
    match boundary:
        case Periodic():
            return inside
        case Dirichlet(value=v):
            return 2.0 * v - inside
        case Neumann(flux):
            return inside + flux * dx
        case Robin(a, b, g):
            return (g * dx + (b - 0.5 * a * dx) * inside) / (b + 0.5 * a * dx)
```

The `match` must have a case for each member, or a final `case _:`. Leaving out `Robin` is a
compile error that names it, so adding a kind to `Boundary` points at every `match` that must
handle it:

```text
error: this `match` does not handle `Robin()`
```

On a union with an enum member, `case Side.LEFT:` matches that member of the enum, and on one with
`None`, `case None:` matches `None`. Class patterns work on a plain struct too, such as
`match wall: case Dirichlet(value=v): ...`.

A union has no methods of its own, but it reaches its members' methods in two ways, each a
branch on the member the union holds, never a table of functions:

- **A shared method.** `x.m(...)` on a union compiles when every member defines `m` with the
  same parameters, modes, defaults and return type, `Raises` included, and it runs the member's
  `m`. A member without `m`, or with a different one, is an error that names it.
- **A shared trait.** A union satisfies a bound `[T: Body]` when every member names `Body` as a
  base. The generic function is compiled once for the union, and each `body.energy()` inside it
  calls the method of the member each value holds.

```python
class Atom(Struct, Copy, Body):
    m: f64
    v: f64

    def energy(self) -> f64:
        return 0.5 * self.m * self.v * self.v


class Dimer(Struct, Copy, Body):
    first: Atom
    second: Atom

    def energy(self) -> f64:
        return self.first.energy() + self.second.energy()


Molecule = Atom | Dimer

molecules: list[Molecule] = [Atom(1.0, 2.0), Dimer(Atom(1.0, 1.0), Atom(2.0, 1.0))]
print(molecules[0].energy())        # 2.0, Atom's energy
print(total_energy(molecules))      # 3.5, with total_energy[T: Body] from "Traits"
```

A union of `Copy` members is `Copy`; otherwise it moves, as a struct does. A union value is a tag
followed by room for its largest member, so an `Array` of a union stores every element at the
size of the largest kind: a `Periodic` in a `Boundary` array takes 32 bytes, 24 for the fields of a
`Robin` and 8 for the tag and its padding.
A hot loop over millions of values of one kind is faster with its own array of that kind.

What builds today, and what a union cannot do yet, is in [What builds today](status.md#types).

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

A default is evaluated at each call that leaves the argument out, so every such call gets a value
of its own ([decision 0027](../decisions/0027-default-parameter-evaluation.md)). Python instead
evaluates a default once, when the `def` runs, and shares that one value between every call: a
default list that one call appends to keeps the item for the next call. Here, a default of
`Array.zeros(3)` gives each call a fresh array of zeros. The default is evaluated after the
arguments that were passed, in the order the parameters are declared, and it sees the names of the
function's own module, never the caller's: a default may call a private `_helper` of its module
even when the caller cannot name it. A default that can raise raises from the call, so the function
declares the error in its `Raises[...]` and the caller handles it as one the call raised.

## Functions as arguments

A parameter of type `Callable[[f64, Vec3], Vec3]` takes a function with that signature. In version
0, the argument must be the name of a module-level function, not a closure. The compiler generates
one copy of the receiving function per function passed to it, so the passed function is inlined
and the call costs nothing ([decision 0026](../decisions/0026-functions-as-arguments.md)).
The receiving function may pass the parameter on to another `Callable` parameter or copy it into
a local. Which function a name holds must be known when compiling, so a local that holds one of
two different functions depending on a branch is reported when building.

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

A generic struct's method called through its class finds the class's type arguments the same
way, from all its arguments: `Pair.both(1.5)` is the `both` of `Pair[f64]`, and
`Pair.total(p)` takes them from `p`. The class's type parameters come first, then the method's
own. A method no argument tells the type argument of, such as `def empty() -> Bag[T]`, is an
error called that way; write the type arguments on the class instead, as an annotation does:
`Bag[f64].empty()`. Written there, they are the class's, and the arguments are checked against
them.

```python
class Pair[T: Number](Struct, Copy):
    a: T
    b: T

    def both(x: T) -> Pair[T]:
        return Pair(x, x)

    def total(self) -> T:
        return self.a + self.b


p = Pair.both(1.5)          # a Pair[f64]
q = Pair[i64].both(2)       # a Pair[i64]
print(Pair.total(p))        # 3.0, the same as p.total()
```

Inside a generic function, a type parameter is its own type: a `T` matches only `T`, and only the
operations its bound allows apply to it. A number literal takes the type `T` under `Float`; under
`Number` only an integer literal does, since `T` may be an integer type, so `x * 0.5` needs
`[T: Float]`. Under `VectorSpace` a literal is an `f64`, which is what scales a vector.

### Traits

A trait names a set of operations a type supports. The standard library has four, and user code
declares its own ([decision 0031](../decisions/0031-traits-are-protocols.md)).

| Trait | A type satisfies it if it supports | Satisfied by |
| --- | --- | --- |
| `Number` | `+ - * /`, comparison, and literals | Every number type |
| `Float` | `Number`, plus the `math` functions | `f32`, `f64` |
| `VectorSpace` | `+` and `-` with itself, and `*` by an `f64` on either side | `f64`, `Fixed[f64, ...]`, `Array[f64, ...]` |
| `Copy` | Copying on assignment | See [copy types](values-and-references.md#copy-types) |

### Declaring a trait

A trait is a class whose last base is `Protocol`. It lists methods, each with the body `...`, and
nothing else: no fields and no type parameters. `Self` in a method stands for the type that
implements the trait.

```python
class Body(Protocol):
    """A point mass that knows its own energy."""

    def energy(self) -> f64: ...

    def mass(self) -> f64: ...


class Charged(Protocol):
    def charge(self) -> f64: ...


class ChargedBody(Body, Charged, Protocol):
    """Requires `Body` and `Charged`: a type that has it has all three methods."""
```

The traits before `Protocol` are the ones it *requires*. A trait cannot require itself, and two
traits it is made of cannot both list a method of the same name.

A trait is not a type: `x: Body` is an error. To accept any type that has a trait, bound a type
parameter by it. A value that may be one of several such types, `Dyn[Body]`, is reserved for
roadmap item 3.5 and is an error until then.

### Implementing a trait

A struct or error type implements a trait by naming it among its bases. This is not inheritance:
the trait adds no fields and no code. It is a promise, checked at the class, that the struct
defines every method the trait lists, and every method of the traits it requires, with the same
parameters, modes and return type, `Self` read as the struct.

```python
class Particle(Struct, Copy, Body):
    v: f64
    m: f64

    def energy(self) -> f64:
        return 0.5 * self.m * self.v * self.v

    def mass(self) -> f64:
        return self.m
```

A missing method, or one with another signature, is an error at the base that names the trait,
showing both signatures. A struct that has the methods but does not name the trait does not have
it, unlike Python's `Protocol`. Naming a class that is not a trait is the inheritance error of
[decision 0015](../decisions/0015-no-class-inheritance.md). An enum cannot implement a trait in
version 0, since enums have no methods yet.

### Using a trait as a bound

A bound is written as for the standard traits, one per type parameter:

```python
def total_energy[T: Body](bodies: list[T]) -> f64:
    total = 0.0
    for body in bodies:
        total += body.energy()
    return total
```

Inside the function, a `T` has the methods its bound lists and those of the traits the bound
requires, and nothing else: `body.m` is an error even when every caller's type has a field `m`.
A call passes the bound when its type argument names the trait, or a trait that requires it.
The function is compiled once for each type it is called with, so `body.energy()` becomes a direct
call of `Particle.energy`, which the compiler can inline: no slower than writing the function for
`Particle` by hand.

To ask for two traits, declare a trait that requires both and bound by it. `[T: (Body, Charged)]`
is an error, because in Python it means `T` is exactly `Body` or exactly `Charged`.

### Embedded structs

A field written `Embed[T]`, where `T` is a struct, is stored exactly like a field of type `T`. The
fields and methods of `T` can also be reached directly through the outer struct:

```python
class Ion(Struct, Copy, ChargedBody):
    particle: Embed[Particle]
    q: f64

    def charge(self) -> f64:
        return self.q


ion = Ion(Particle(2.0, 39.9), 1.0)
ion.m           # ion.particle.m
ion.energy()    # ion.particle.energy()
```

- The layout is the same as a plain field: `ion.m` is the same offset as `ion.particle.m`, so
  embedding costs nothing at run time.
- A name the outer struct defines itself hides an embedded one. A name two embedded structs
  provide at the same depth is an error at the class. An embedded struct's own embedded fields are
  reached too, one depth further.
- A promoted method satisfies a trait the outer struct names, as `energy` and `mass` do for `Ion`
  above. Embedding alone implements nothing: the trait must be named.
- A promoted method receives the embedded struct as `self`, not the outer one, so it cannot call
  a method the outer struct defines. One that takes `self` as `Owned` must be called on the field
  by name, which moves the whole struct, and does not satisfy a trait for the outer struct.
- A struct cannot embed itself, directly or through another struct, and only structs can be
  embedded.

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
it. The one exception is the name an `except ... as` binds, which belongs to its `except` block
(see [Errors](errors.md)), as Python deletes it when the block ends. A variable may be used only
where every path that reaches the use has assigned it; the compiler checks this and names the path
that does not.

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
