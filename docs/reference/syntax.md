# Syntax

Every program is valid Python 3.12 syntax ([decision 0004](../decisions/0004-python-syntax.md)).
The grammar is Python's own, published as the
[Python 3.12 grammar](https://docs.python.org/3.12/reference/grammar.html); this chapter does not
repeat it. What this chapter defines is which parts of that grammar the language accepts, and the
names it adds where Python has no spelling.

## Source files

- A source file is UTF-8 text with `\n` line endings. `.npl` is the working file extension until
  roadmap decision D.6 names the language.
- One file is one module. A directory of modules is a package; it needs no `__init__` file.
- A module holds only these at its top level: imports, `Final` constants, type aliases, functions
  and classes. There is no other top-level code and there are no module-level variables. A program
  starts at its `main` function, which takes no parameters and is defined in `main.npl` at the
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

`npl doc <directory> <output>` writes a package's documentation as Markdown: one page per module,
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
| `for x in e` | yes | Over `range`, `prange`, `simd`, an array, a list, a string or a tuple of one type. No `else` clause |
| `break`, `continue`, `pass`, `return` | yes | `break` and `return` are not allowed in a `prange` body |
| `def` | yes | At module level, and as methods inside a class. Nested functions are not in version 0 |
| `class` | yes | Only with the base `Struct` (optionally with `Copy`), `Exception` or `Enum`. No inheritance, by [decision 0015](../decisions/0015-no-class-inheritance.md); see [Types](types.md#structs) |
| `import m`, `from m import x`, `as` | yes | No `from m import *`, no relative imports; see [Source files](#source-files) |
| `raise E(...)` | yes | Only inside a function that declares it can fail; see [Errors](errors.md) |
| `try` with `except` | yes | Each `except` names the error types it handles. No bare `except:`, no `else`, no `finally` |
| `with unsafe:` | yes | The only `with` form in version 0 |
| `match` | yes | Literal, enum, tuple and wildcard patterns; must be exhaustive |
| `assert cond, message` | yes | Checked in every build; a false condition panics |
| `del` | no | A value is destroyed when its owner goes out of scope |
| `global`, `nonlocal` | no | There are no module-level variables |
| `lambda` | no | Pass a named function instead |
| `async`, `await`, `yield` | no | |
| `type X = ...` | no | Write a type alias as `X = ...`, one spelling for one thing |

`try` has no `finally` because cleanup is automatic: every value is destroyed when its owner goes
out of scope, on every path out of a function.

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
| `in`, `not in` | yes | On lists, strings and tuples |
| Conditional `a if c else b` | yes | |
| Calls with positional and keyword arguments | yes | No `*args` or `**kwargs`, in calls or in definitions |
| Attributes `a.b`, subscripts `a[i, j]`, slices `a[1:n, ::2]` | yes | See [Arrays](arrays.md) |
| Tuples and lists `(a, b)`, `[a, b]` | yes | |
| List comprehensions `[f(x) for x in xs if p(x)]` | yes | Produce a `list` |
| Dict and set literals and comprehensions, generator expressions | no | Not in version 0 |
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
| `InOut`, `Owned` | Parameter modes | [Values and references](values-and-references.md#parameters) |
| `Self` | The type of the struct a method belongs to | [Values and references](values-and-references.md#methods) |
| `Raises` | Return type of a function that can fail | [Errors](errors.md) |
| `prange`, `simd` | Parallel and vectorized loops | [Parallel and vector loops](parallel-loops.md) |
| `unsafe` | Context for operations the compiler cannot check | [Arrays](arrays.md#unchecked-access) |
| `test`, `bench` | Decorators marking test and benchmark functions | Roadmap 1.11 |
| `layout` | Decorator that pins a struct's field order: `@layout("C")` | [Types](types.md#field-layout) |
| `Callable` | The type of a function passed as an argument | [Types](types.md#functions-as-arguments) |
| `Number`, `Float`, `VectorSpace` | Standard traits that bound a type parameter, as in `def rk4_step[S: VectorSpace](...)` | [Types](types.md#traits) |

Python's own built-in names that the language keeps, with their Python meaning where it applies:
`print`, `len`, `range`, `round`, `abs`, `min`, `max`, `sum`, `enumerate`, `zip`, `int`, `float`,
`complex`, `bool`, `str`, `list`, `tuple`, `True`, `False`, `None`, `Exception` and `Enum`.

## Names and style

Names follow Python's rules. By convention, functions, variables and modules are `snake_case`,
types are `CapWords` and constants are `UPPER_CASE`; the formatter and linter enforce what Python
tools already enforce. A name starting with `_` is private to its module.
