# 0037. `int(text)` and `float(text)` raise a built-in `ValueError`

Status: Accepted
Date: 2026-11-10
Roadmap: 1.13.12

## Context

Roadmap 1.13.12 lets a program parse the text it reads from a file: split a line, strip it, and
turn each field into a number. In Python that last step is `int(field)` or `float(field)`, and
text that is not a number raises `ValueError`:

```python
>>> int("12a")
ValueError: invalid literal for int() with base 10: '12a'
>>> float("")
ValueError: could not convert string to float: ''
```

Bad input is an expected failure, not a bug: a data file with a stray header line or a blank
field is ordinary. So it must be an error the program can handle, not a panic
([decision 0005](0005-error-handling.md)). In this language a function that can fail says so in
its return type, `Raises[T, E]`, and `E` is a class with the base `Exception`. Python's
`ValueError` is not declared anywhere a Baryon program can see, so something has to provide it.

The options:

1. **A built-in `ValueError`, as Python.** The language provides `class ValueError(Exception)`
   with one field, `message: str`. `int(text)` returns `Raises[int, ValueError]` and
   `float(text)` returns `Raises[f64, ValueError]`.
2. **A panic.** `int(text)` returns `int`, and bad text stops the program. Short to write, but a
   blank line at the end of a data file would crash the analysis that reads it, with no way to
   skip the line.
3. **A different shape.** A `ParseError` of our own, or a result such as `int | None`. Either
   reads differently from Python, so every ported `except ValueError` would need rewriting.

## Decision

Option 1, a built-in `ValueError`, as Python. Collins chose this on 2026-11-10.

```python
class ValueError(Exception):
    """A value had the right type but a wrong content, such as text that is not a number."""

    message: str
```

- `ValueError` is a name the language provides, defined in the standard library's
  `<std>/builtins.byn` as [decision 0038](0038-standard-library-in-baryon.md) describes, so it is
  an ordinary `Exception` class: a program catches it with `except ValueError as error`, reads
  `error.message`, lists it in `Raises`, and may raise one itself with
  `raise ValueError("negative mass")`. A program that ends `main` with one prints it as it prints
  any error.
- `int(text: str) -> Raises[int, ValueError]` reads text as Python's `int(text)` does with base
  10: optional whitespace around it, an optional `+` or `-`, then decimal digits, with single
  underscores allowed between digits (PEP 515), so `" -1_000\n"` is `-1000`. Anything else
  raises `ValueError` with Python's message, `invalid literal for int() with base 10: '12a'`,
  quoting the text as Python's `repr` would.
- An integer outside `int`'s range, -2<sup>63</sup> to 2<sup>63</sup> - 1, raises `ValueError`
  too, with the message `int() result out of range: '<text>'`. Python's integers have no range,
  so this is the one message Python never gives.
- `float(text: str) -> Raises[f64, ValueError]` reads text as Python's `float(text)` does:
  whitespace around it, a sign, decimal digits with underscores between digits, a fraction and
  an exponent, and `inf`, `infinity` and `nan` in any case. Text too large for `f64` gives
  infinity, as in Python. Anything else raises `ValueError` with Python's message,
  `could not convert string to float: '12a'`.
- `int(x)` and `float(x)` of a number are unchanged: they cannot fail and return the number
  converted, with no `Raises`.
- Digits are ASCII digits only. Python also accepts digits from other scripts, such as `"٣"`.
  [Differences from Python](../reference/differences-from-python.md) lists this.
- A program that declares its own `class ValueError(Exception)` keeps it: by decision 0038 a
  user's module is never shadowed by the library. Its `int(text)` still raises the built-in
  `ValueError`, a different type, and the compiler names the mismatch where it is not covered.

## Consequences

- Ported parsing code reads as in Python:

  ```python
  def read_masses(lines: list[str]) -> list[f64]:
      masses: list[f64] = []
      for line in lines:
          try:
              masses.append(float(line))
          except ValueError:
              continue
      return masses
  ```

- Because `int(text)` and `float(text)` can fail, a function that calls them must handle
  `ValueError` or list it in its `Raises`, as for any fallible call. A Python programmer who
  expects to ignore it is told so by the compiler, not by a crash in production.
- The cost of a failed parse is building the message, one allocation. A successful parse costs
  one pass over the text and no allocation.
- `ValueError` becomes the natural error for other built-ins that Python lets fail on bad
  values, such as `str.index` or `list.remove` of a missing item, once they are built. Those
  that panic today, such as `split` with an empty separator, stay panics until a decision says
  otherwise, since changing them makes a call fallible.
- `int(text, base)` is not part of this record; it would add a parameter, not a new error.
