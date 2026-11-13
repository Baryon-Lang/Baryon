# Syntax

[What builds today](status.md#syntax) says which parts of this chapter the compiler builds into
programs yet.

Every program is valid Python 3.12 syntax ([decision 0004](../decisions/0004-python-syntax.md)).
The grammar is Python's own, published as the
[Python 3.12 grammar](https://docs.python.org/3.12/reference/grammar.html); this chapter does not
repeat it. What this chapter defines is which parts of that grammar the language accepts, and the
names it adds where Python has no spelling.

## Source files

- A source file is UTF-8 text with `\n` line endings. Source files end in `.byn`
  ([decision 0023](../decisions/0023-language-name-baryon.md)).
- One file is one module. A directory of modules is a package; it needs no `__init__` file.
- A module holds only these at its top level: imports, `Final` constants, type aliases, functions
  and classes. There is no other top-level code and there are no module-level variables. A program
  starts at its `main` function, which takes no parameters and is defined in `main.byn` at the
  package's root. No other module may define a function named `main`.

  Why: top-level statements and global variables are shared mutable state, which a parallel loop
  could race on. Without them, every function's inputs are its parameters.

- Each top-level name is defined once per module. Defining a function, class, constant or alias
  twice is an error, where Python would let the second silently replace the first. Two
  `import a.b` and `import a.c` both binding `a` are not a clash.
- Top-level names are visible throughout their module, in any order, because a module holds only
  definitions.
- Imports name modules from the package root: `from physics.nbody import step`. Relative imports,
  `from . import x` and `from .nbody import step`, are not supported, so a module's imports read
  the same wherever the file sits.
- A name starting with `_` is private to its module: importing it, or reading it as
  `module._name`, is an error.

- A string literal as the first statement of a module, function or class is its documentation, as
  in Python. A string literal right after a `Final` constant or a field documents that constant or
  field. Documentation tools read both.

### Documentation strings

A documentation string is written in Markdown: backticks around code, `-` before list items, a
blank line between paragraphs. Its first line is a one-sentence summary that stands alone. Its
indentation in the source is removed before it is read, as Python's `inspect.cleandoc` does.

```python
def kinetic_energy(velocities: Array[Vec3, 1]) -> f64:
    """The total kinetic energy of the atoms, in reduced units.

    Every atom has mass 1, so this is half the sum of `|v|²` over `velocities`.
    """
```

`baryon doc <directory> <output>` writes a package's documentation as Markdown: one page per module,
named after it (`physics.nbody.md`), and an `index.md` listing every module with its summary. A
page gives the module's documentation, then each public constant, type alias, function and
class in source order, with its signature as written and its documentation; a class's fields
and methods follow it. Names starting with `_` are private and left out. An item with no
documentation string is still listed, with its signature.

## Statements

The compiler reports every construct in a "no" row below, and every rule in the notes that can be
seen from the syntax alone, with a message that names it, says why and, where there is one, gives
the spelling to use instead.

| Statement | Accepted | Notes |
| --- | --- | --- |
| Expression statement | yes | Only a call or a documentation string. A bare `x == 1` is an error, which catches it written for `x = 1` |
| Assignment `x = e` | yes | Moves or copies; see [Values and references](values-and-references.md) |
| Annotated assignment `x: T = e` | yes | Declares the variable's type |
| Augmented assignment `x += e` | yes | Every operator Python has: `+= -= *= /= //= %= **= @= &= \|= ^= <<= >>=` |
| Tuple assignment `a, b = e` | yes | Evaluates the whole right side before assigning; see [moves](values-and-references.md#tuple-assignment) |
| Star targets `a, *rest = e` | no | |
| `if`, `elif`, `else` | yes | The condition must be a `bool`; see [Types](types.md#bool) |
| `while` | yes | No `else` clause |
| `for x in e` | yes | Over `range`, `prange`, `simd`, an array, a list, a string or a tuple of one type, over [`enumerate`, `zip` and `reversed`](#enumerate-zip-and-reversed) of these, and over a class with `__next__` or `__iter__`; see [iterators](#iterators). No `else` clause |
| `break`, `continue`, `pass`, `return` | yes | `break` and `return` are not allowed in a `prange` body |
| `def` | yes | At module level, and as methods inside a class. Nested functions are not in version 0 |
| `class` | yes | Only with the base `Struct` (optionally with `Copy`), `Exception` or `Enum`. No inheritance, by [decision 0015](../decisions/0015-no-class-inheritance.md); see [Types](types.md#structs) |
| `import m`, `from m import x`, `as` | yes | No `from m import *`, no relative imports; see [Source files](#source-files) |
| `raise E(...)` | yes | Only inside a function that declares it can fail; see [Errors](errors.md). No `raise ... from`: errors do not chain, so keep a cause in a field ([decision 0034](../decisions/0034-no-error-chaining.md)) |
| `try` with `except` | yes | Each `except` names the error types it handles. No bare `except:`, no `else`, no `finally` |
| `with` | yes | `with unsafe:`, `with open(...) as f:` and the other files, and any class with `__enter__` and `__exit__`; see [`with` and context managers](#with-and-context-managers). The name after `as` is one name |
| `match` | yes | Literal, enum, tuple, [class](types.md#unions-of-structs) and wildcard patterns; must be exhaustive |
| `assert cond, message` | yes | Checked in every build; a false condition panics |
| `del d[k]` | yes | Removes one dict entry; see [Dictionaries](types.md#dictionaries). `del x` is an error: a value is destroyed when its owner goes out of scope |
| `global`, `nonlocal` | no | There are no module-level variables |
| `lambda` | no | Pass a named function instead |
| `async`, `await`, `yield` | no | |
| `type X = ...` | no | Write a type alias as `X = ...`, one spelling for one thing |

`try` has no `finally` because cleanup is automatic: every value is destroyed when its owner goes
out of scope, on every path out of a function.

### `with` and context managers

A *context manager* is a value whose class defines `__enter__` and `__exit__`, Python's two
methods for "set up, then always tear down"
([decision 0053](../decisions/0053-with-and-for-on-user-classes.md)).
`with manager as name:` calls `__enter__`, binds what it returns to `name` for the block, and
calls `__exit__` on it when the block ends, however it ends:

```python
class EnergyAudit(Struct):
    label: str
    state: Oscillator
    start: f64

    def __enter__(self: Owned[Self]) -> Self:
        self.start = self.state.energy()
        return self

    def __exit__(self):
        drift = (self.state.energy() - self.start) / self.start
        print(f"{self.label}: relative energy drift {drift:.3e}")


def main():
    with EnergyAudit("leapfrog", Oscillator(1.0, 0.0), 0.0) as audit:
        leapfrog(audit.state, 0.01, 1000)
```

The whole program is the example [`energy_audit.byn`](../examples/energy_audit.byn).

- **`__enter__(self: Owned[Self]) -> Self`** takes the value and gives it back, which is
  Python's `return self` under the [ownership rules](values-and-references.md): a method can
  return `self` only when it owns it. It may fail, `-> Raises[Self, E]`; then the block does not
  run and `__exit__` is not called.
- **`__exit__(self)`** takes `self` in any mode and returns `None`, or `Raises[None, E]`. Taking
  `self` as `Owned` consumes the value, so reading `name` after the block is an error; a plain or
  `InOut` `self` leaves it there, as Python leaves the name bound.
- **`__exit__` runs on every way out of the block:** its end, `return`, `break`, `continue`, and
  an error raised in it or passed on from a call. A `return` computes its value first, then
  calls `__exit__`. A panic runs nothing.
- **An error from `__exit__` is an error of the `with` statement**, which the function must
  allow or a `try` around the `with` must handle. If the block was leaving with another error,
  the one from `__exit__` replaces it, as in Python.
- **`__exit__` takes nothing but `self`.** Python passes the error and lets `__exit__` swallow it
  by returning true; Baryon errors are handled by `try`, where the reader sees them. Python's
  `__exit__(self, exc_type, exc, tb)` is an error that says so.
- **The name stays put in the block.** It may be read, passed to a call and changed in place,
  as `leapfrog(audit.state, ...)` changes it, but not moved away or assigned a new value, since
  `__exit__` must find it there:

  ```text
  error: `audit` is held by a `with` block, so it cannot be moved
    --> main.byn:47:20
     |
  47 |             kept = audit
     |                    ^^^^^ moved here
    ::: main.byn:46:74
     |
  46 |         with EnergyAudit(f"{steps} steps", Oscillator(1.0, 0.0), 0.0) as audit:
     |                                                                          ----- `__exit__` is called on `audit` when the block ends
     = note: keep a copy instead: `audit.copy()`
  ```

- **Several managers nest:** `with a as x, b as y:` is `with a as x:` around `with b as y:`, so
  `y` exits first. Without `as`, a hidden variable holds the value until `__exit__`.
- The class must define both methods itself; one reached through an `Embed` field does not
  count. A generic function cannot yet bound a type parameter by "any context manager" (idea
  I.59).

### Iterators

An *iterator* is a value whose class defines `__next__`, Python's method for "give me the next
value". `for x in iterator:` calls it once a pass and ends the loop when it raises
`StopIteration`, an error type the language provides, with no fields
([decision 0053](../decisions/0053-with-and-for-on-user-classes.md)):

```python
class Samples(Struct):
    state: Oscillator
    dt: f64
    t: f64
    left: int

    def __next__(self: InOut[Self]) -> Raises[tuple[f64, f64], StopIteration]:
        if self.left == 0:
            raise StopIteration()
        self.left -= 1
        leapfrog(self.state, self.dt, 1)
        self.t += self.dt
        return (self.t, self.state.x)


def main():
    worst = 0.0
    for t, x in Samples(Oscillator(1.0, 0.0), 0.01, 0.0, 1000):
        worst = max(worst, abs(x - cos(t)))
```

The whole program is the example [`energy_audit.byn`](../examples/energy_audit.byn).

- **`__next__(self: InOut[Self]) -> Raises[T, StopIteration]`.** The loop variable owns each `T`,
  so it may be kept, and after the loop it holds the last one, as in Python. Ending the loop
  costs one branch on the returned value, the same as any call that can fail. A `__next__` that
  cannot raise `StopIteration` is an error, since the loop could never end.
- **Other errors pass on.** `Raises[str, StopIteration | OSError]` makes the `for` statement
  fail with `OSError`, which the function must allow or a `try` must handle.
- **The loop uses the iterator itself.** Python calls `__iter__` first, and an iterator's
  `__iter__` returns `self`; Baryon skips the call, so `__iter__` need not be written. A
  variable is borrowed for the loop: the body may read it but not change or move it, and it
  keeps its state afterwards, so a second loop goes on where a `break` left the first. A
  temporary, such as `Samples(...)` above, is freed when the loop ends.
- **An iterable gives an iterator.** A class that defines `__iter__` and not `__next__` is
  *iterable*: the loop calls `__iter__` once, in the mode it declares, and loops over the
  iterator it returns. A container can hand out a new iterator each time and keep its data.
- A comprehension over an iterator, `[x for x in Samples(...)]`, is not built yet; write the
  loop. `for` has no `else`, here as everywhere.

## Expressions

| Expression | Accepted | Notes |
| --- | --- | --- |
| Number, string, `True`, `False`, `None` literals | yes | `1_000_000`, `0x1F`, `1e-10`, `2j`; see [number literals](types.md#number-literals) |
| f-strings with format specifications | yes | `f"{energy:10.4f}"` |
| Arithmetic `+ - * / // % **` and unary `-` `+` | yes | See [number mixing](types.md#number-mixing) |
| Matrix product `@` | yes | On rank-2 arrays and `Fixed` matrices |
| Bitwise `& \| ^ ~ << >>` | yes | Integers only |
| Comparisons, chained: `0 <= i < n` | yes | As in Python, each operand is evaluated once |
| `and`, `or`, `not` | yes | Operands are `bool` |
| `is None`, `is not None` | yes | `is` and `is not` compare only with `None` |
| `in`, `not in` | yes | On lists, strings, tuples and dicts |
| Conditional `a if c else b` | yes | |
| Calls with positional and keyword arguments | yes | No `*args` or `**kwargs`, in calls or in definitions |
| Attributes `a.b`, subscripts `a[i, j]`, slices `a[1:n, ::2]` | yes | See [Arrays](arrays.md) |
| Tuples and lists `(a, b)`, `[a, b]` | yes | |
| List comprehensions `[f(x) for x in xs if p(x)]` | yes | Produce a `list` |
| Dict displays `{k: v}` | yes | Produce a `dict`. See [Dictionaries](types.md#dictionaries) |
| Dict comprehensions `{k: v for x in e if c}` | yes | Produce a `dict`. See [Dictionaries](types.md#dictionaries) |
| Set displays, set comprehensions, generator expressions | no | Not in version 0 |
| Assignment expressions `x := e` | no | |
| `lambda` | no | |

## Names the language adds

Python has no spelling for these, so the language gives them Python-shaped names. They are always
in scope, with no import.

| Name | What it is | Defined in |
| --- | --- | --- |
| `f64`, `f32`, `i64`, `i32`, `i16`, `i8`, `u64`, `u32`, `u16`, `u8`, `complex64`, `complex128` | Sized number types | [Types](types.md#numbers) |
| `Final` | Marks a constant: `N: Final = 64` | [Types](types.md#constants) |
| `Struct` | Base that makes a class a struct | [Types](types.md#structs) |
| `Copy` | Base that makes a struct copy instead of move | [Values and references](values-and-references.md#copy-types) |
| `Array`, `Fixed` | The two array types | [Arrays](arrays.md) |
| `DataFrame` | A table of named columns, each an array | [DataFrames](dataframes.md) |
| `concat` | Stacks frames of one schema, `concat([a, b])` | [DataFrames](dataframe-operations.md#stacking-frames) |
| `InOut`, `Owned` | Parameter modes | [Values and references](values-and-references.md#parameters) |
| `Self` | The type of the struct a method belongs to | [Values and references](values-and-references.md#methods) |
| `Raises` | Return type of a function that can fail | [Errors](errors.md) |
| `prange`, `simd` | Parallel and vectorized loops | [Parallel and vector loops](parallel-loops.md) |
| `unsafe` | Context for operations the compiler cannot check | [Arrays](arrays.md#unchecked-access) |
| `test`, `bench` | Decorators marking test and benchmark functions | Roadmap 1.11 |
| `layout` | Decorator that pins a struct's field order: `@layout("C")` | [Types](types.md#field-layout) |
| `Callable` | The type of a function passed as an argument | [Types](types.md#functions-as-arguments) |
| `Number`, `Float`, `VectorSpace` | Standard traits that bound a type parameter, as in `def rk4_step[S: VectorSpace](...)` | [Types](types.md#traits) |
| `Protocol` | Base that makes a class a trait: `class Body(Protocol)` | [Types](types.md#declaring-a-trait) |
| `Embed` | A struct field whose fields and methods the outer struct reaches: `particle: Embed[Particle]` | [Types](types.md#embedded-structs) |
| `File` | An open text file, the type `open` returns | [Files and processes](files-and-processes.md) |
| `Dyn` | Reserved for a value of any type with a trait, `Dyn[Body]`; an error until roadmap 3.5 | [Types](types.md#declaring-a-trait) |

Python's own built-in names that the language keeps, with their Python meaning where it applies:
`print`, `len`, `range`, `round`, `abs`, `min`, `max`, `sum`, `enumerate`, `zip`, `sorted`,
`reversed`, `int`, `float`, `complex`, `bool`, `str`, `list`, `tuple`, `True`, `False`, `None`,
`Exception`, `Enum` and `open`, which [Files and processes](files-and-processes.md) describes.

`print(a, b, ...)` writes its arguments to standard output as Python does: separated by one space,
followed by a line end. A float prints as Python's `repr` writes it, with the fewest digits that
read back as the same value: `0.1`, `2.0`, `0.30000000000000004`, and an exponent below `1e-4` or
from `1e16` up, as in `1.5e-05` and `1e+16`. An `f32` prints the fewest digits that read back as
the same `f32`, so `f32(0.1)` prints `0.1`. A `bool` prints `True` or `False`.

```python
def main():
    print("mean:", (1.0 + 2.0) / 2.0)  # mean: 1.5
    print(1e16, 3 > 2)                 # 1e+16 True
```

### What `print` writes for each value

| Value | Printed |
| --- | --- |
| A number or a `bool` | As above: `1.5`, `-3`, `True` |
| A `str` | Its characters, without quotes |
| `None` | `None` |
| A `T \| None` | The `T` when there is one, else `None` |
| A [union](types.md#unions-of-structs) | The member it holds, as that member prints: `Dirichlet(value=300.0)` |
| A tuple | `(1, 2.5)`; one item keeps its comma, `(1,)`; the empty tuple is `()` |
| An enum member | The enum's name and the member's: `Phase.GAS` |
| A struct or an error | The type's name, then each field by name, in declaration order: `Point(x=1.0, y=2.0)`, as a Python dataclass prints |
| A list | `[1, 2, 3]`; the empty list is `[]` |
| A dict | `{'a': 1, 'b': 2}`, its entries in insertion order; the empty dict is `{}` |
| An `Array` or `Fixed` array | As NumPy prints an `ndarray`: `[1.  2.5]`, rows on their own lines, only the corners of more than 1000 elements ([Printing arrays](arrays.md#printing)) |

Inside a tuple, a struct, a list or a dict, each item, field, element, key and value is written as Python's `repr` writes it, so a
`str` is quoted: `('argon', 39.95)`, `Atom(name='argon')`, `['He', 'Ne']`, `{'Ar': 39.948}`. A string holding a single quote and
no double quote is quoted with double quotes, `"it's"`, as in Python. A generic struct prints
without its type arguments, `Pair(first=1, second=2)`, and an enum member prints `Phase.GAS`
everywhere, where Python's `repr` inside a container would write `<Phase.GAS: 3>`
([Differences from Python](differences-from-python.md)). An array prints as NumPy prints an
`ndarray` there too, `Grid(cells=[1. 2. 3.])`
([decision 0033](../decisions/0033-printing-arrays.md)).

Printing costs one call into the runtime per number, `bool` or `str` it writes; a struct of ten
fields is ten calls and its punctuation, all chosen when the program is compiled. A list or a
dict is a loop that runs when the program does: one call per element and per `, `, plus, for a
dict, one call per entry to find the next. An array is one call, which formats at most 1000
elements.

### `sep=` and `end=`

`sep=` replaces the space between values and `end=` replaces the line end, each with a `str`.
`None` keeps the default, as in Python. A `sep=` or `end=` of another type is a compile error.
`file=` sends the output to a file or to standard error, and `flush=True` writes it out at once:
see [Standard streams](files-and-processes.md#standard-streams). Any other keyword is a compile
error.

```python
class Point(Struct, Copy):
    x: f64
    y: f64


def main():
    print(Point(1.0, 2.0))                # Point(x=1.0, y=2.0)
    print(("argon", 39.95), None)         # ('argon', 39.95) None
    print(1, 2, 3, sep=", ")              # 1, 2, 3
    print("step", 4, end=": ")
    print("done")                         # step 4: done
```

Output that `end=` leaves without a line end still appears when the program ends, and before the
message of a panic.

### `enumerate`, `zip` and `reversed`

These three walk sequences in a `for` loop or a comprehension, as in Python. The sequences are
those a `for` loop takes: arrays, lists, strings, tuples of one type and `range`.

- `enumerate(values)` gives `(count, value)` pairs, counting from 0, or from `start` when given:
  `enumerate(values, 1)` or `enumerate(values, start=1)`. The count is an `int`.
- `zip(a, b, ...)` gives a tuple of one element from each sequence, and stops at the end of the
  shortest, as Python does.
- `reversed(values)` gives the elements from the last to the first. It takes a sequence whose
  length is known, so not `enumerate` or `zip`; reverse the sequences inside instead,
  `zip(reversed(a), reversed(b))`.

```python
def main():
    names = ["argon", "neon", "xenon"]
    masses = Array([39.948, 20.180, 131.29])
    for i, (name, mass) in enumerate(zip(names, masses), start=1):
        print(i, name, mass)          # 1 argon 39.948, then 2 neon 20.18, ...
    for k in reversed(range(3)):
        print(k)                      # 2, then 1, then 0
```

None of them builds a list or copies a sequence: the loop reads each sequence in place at its own
position, `reversed` from the end, so they cost no more than indexing by hand. A loop over
elements that are not copied, such as the `str`s above, borrows each one, as a plain `for` loop
does ([values and references](values-and-references.md)).

## Names and style

Names follow Python's rules. By convention, functions, variables and modules are `snake_case`,
types are `CapWords` and constants are `UPPER_CASE`; the formatter and linter enforce what Python
tools already enforce. A name starting with `_` is private to its module.
