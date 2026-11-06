# Differences from Python

Programs in this language are valid Python syntax and mostly read the way Python does. This page
lists every place where the same code means something different, or where Python code is rejected,
so that nothing surprises a Python programmer twice. Each row links to the rule and gives the
reason in a line.

## What a program means

| Python | Here | Why |
| --- | --- | --- |
| `b = a` makes two names for one list | `b = a` moves an array, list or struct; `a` cannot be used afterward | No garbage collector; ownership decides when memory is freed. [Moves](values-and-references.md#moves) |
| A function can change any list passed to it | Parameters are read-only unless declared `InOut[T]` | The compiler must know what a call can change to prove arrays never alias. [Parameters](values-and-references.md#parameters) |
| `int` has no size limit | `int` is a 64-bit integer; sized types such as `i32` and `u8` exist | Fixed widths are what fit in a register. [Numbers](types.md#numbers) |
| `float32` and `float64` mix silently in NumPy | Mixing widths is an error until converted with `f64(x)` | Precision never changes without a visible conversion. [Number mixing](types.md#number-mixing) |
| `1.0 / 0.0` raises `ZeroDivisionError` | `1.0 / 0.0` is infinity, as in IEEE 754 and NumPy | Numeric code expects IEEE results. [Numbers](types.md#numbers) |
| `True + 1` is `2` | `bool` is not a number; that is an error | [bool](types.md#bool) |
| Any value can be a condition: `if items:` | Conditions must be `bool`: `if len(items) > 0:` | A condition should not depend on a rule the reader has to remember. [bool](types.md#bool) |
| A variable can change type: `x = 1` then `x = "one"` | A variable keeps the type of its first value | One name meaning two things is a common bug in long code. [Type inference](types.md#type-inference) |
| `a[-1]` and `a[i]` with `i = -1` both read the last element | `a[-1]` reads the last element; a negative index computed at run time panics | In a stencil, `u[i - 1]` at `i = 0` is a bug, not a wrap-around. [Indexing](arrays.md#indexing) |
| `a[0:100]` on a 10-element list gives 10 elements | A slice past the end panics | A range that does not fit is usually a bug. [Slicing](arrays.md#slicing) |
| A slice of a list is a copy; a NumPy slice is a view | An array slice is a view, and cannot be bound to a name in version 0 | A named view would be a stored borrow. [Slices](values-and-references.md#slices) |
| A list can hold values of different types | `list[T]` holds one type | Values of one type are stored contiguously, with no boxing. [Lists](types.md#lists) |
| Exceptions unwind, and any function may raise | A function that can fail says so with `Raises[T, E]`; errors are return values | Failure is visible in signatures and costs one branch. [Errors](errors.md) |
| Every failure is an exception and can be caught | Bugs such as an index out of range panic, and panics cannot be caught | A caught bug is an ignored bug. [Panics](errors.md#panics) |
| `assert` is removed by `python -O` | `assert` is checked in every build | [Syntax](syntax.md#statements) |
| Classes have inheritance and reference semantics | A class with base `Struct` is a value type with no inheritance | Value semantics are what make arrays of structs contiguous. [Structs](types.md#structs) |
| Top-level code runs on import; modules have global variables | Only definitions at the top level; the program starts at `main` | Shared mutable state would race in parallel loops. [Source files](syntax.md#source-files) |

## What Python has that version 0 does not

`lambda` and nested functions, generators and `yield`, `async`, `global` and `nonlocal`, `del`,
`*args` and `**kwargs`, star assignment, `:=`, `dict` and `set` displays and comprehensions,
generator expressions, `try` with `finally` or `else`, `for` and `while` with `else`, `with` other
than `with unsafe:`, class inheritance, decorators other than `@test` and `@bench`, and the `type`
statement. The compiler rejects each with a message; none is silently ignored. Some will come
later; the [Syntax](syntax.md) chapter lists what is accepted.

## Names that look like Python's modules

`math` and `random` are this language's standard library modules, not CPython's. They follow
Python's names where Python has the function, so `from math import sqrt` reads as expected, but
`random` provides counter-based generators such as `Philox` that Python's module does not. Running
a program under CPython therefore needs a small shim library; roadmap idea I.11 proposes one, to
use CPython as a reference for checking the compiler's output.
