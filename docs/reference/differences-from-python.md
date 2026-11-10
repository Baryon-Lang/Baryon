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
| `(-8.0) ** (1 / 3)` is a complex number, and `math.sqrt(-1.0)` and `math.log(0.0)` raise `ValueError` | Float `**` and the `math` functions give IEEE 754 results, as NumPy does: `(-8.0) ** (1 / 3)` and `sqrt(-1.0)` are NaN, `log(0.0)` is `-inf` | The same rule as float division by zero: numeric code expects IEEE results, and a float never turns into a complex number. [Math functions](arrays.md#math-functions) |
| `print` of a class instance writes `<Point object at 0x…>` unless the class defines `__repr__`; inside a list or tuple an enum member is `<Phase.GAS: 3>` | A struct prints as a dataclass does, `Point(x=1.0, y=2.0)`, and an enum member prints `Phase.GAS` everywhere | Every struct is a plain value, as a dataclass is, and the compiler keeps an enum member's position, not its value. [`print`](syntax.md#what-print-writes-for-each-value) |
| `z / 0j` and `0j ** -1` raise `ZeroDivisionError` | They give IEEE 754 results, as NumPy does: `(1+1j) / 0` is `inf+infj`, `0j ** -1` is `nan+nanj` | The same rule as float division by zero. [Complex numbers](types.md#complex-numbers) |
| `True + 1` is `2` | `bool` is not a number; that is an error | [bool](types.md#bool) |
| Any value can be a condition: `if items:` | Conditions must be `bool`: `if len(items) > 0:` | A condition should not depend on a rule the reader has to remember. [bool](types.md#bool) |
| A variable can change type: `x = 1` then `x = "one"` | A variable keeps the type of its first value | One name meaning two things is a common bug in long code. [Type inference](types.md#type-inference) |
| `a[-1]` and `a[i]` with `i = -1` both read the last element | `a[-1]` reads the last element; a negative index computed at run time panics | In a stencil, `u[i - 1]` at `i = 0` is a bug, not a wrap-around. [Indexing](arrays.md#indexing) |
| `a[0:100]` on a 10-element list gives 10 elements | A slice past the end panics | A range that does not fit is usually a bug. [Slicing](arrays.md#slicing) |
| A slice of a list is a copy; a NumPy slice is a view | An array slice is a view, and cannot be bound to a name in version 0 | A named view would be a stored borrow. [Slices](values-and-references.md#slices) |
| A list can hold values of different types | `list[T]` holds one type | Values of one type are stored contiguously, with no boxing. [Lists](types.md#lists) |
| Any hashable value is a `dict` key, floats included, and `d[k]` for a missing key raises `KeyError` | Keys are integers, `bool`, `str`, enums and tuples of them; a missing key panics | A float key that misses by rounding is a silent bug. [Dictionaries](types.md#dictionaries) |
| Exceptions unwind, and any function may raise | A function that can fail says so with `Raises[T, E]`; errors are return values | Failure is visible in signatures and costs one branch. [Errors](errors.md) |
| `sys.exit` raises `SystemExit`, which a `try` can catch | `sys.exit` ends the program at once; nothing catches it | It is not an error a caller can handle, and errors are values here. [Exit status](files-and-processes.md#exit-status) |
| `os.environ[name] = value` changes the program's environment | The environment is read-only | Changing it while another thread reads it is a data race in the C library, and `prange` loops run on several threads. [Environment variables](files-and-processes.md#environment-variables) |
| Every failure is an exception and can be caught | Bugs such as an index out of range panic, and panics cannot be caught | A caught bug is an ignored bug. [Panics](errors.md#panics) |
| `assert` is removed by `python -O` | `assert` is checked in every build | [Syntax](syntax.md#statements) |
| Classes have inheritance and reference semantics | A class is a struct, an error type or an enum: a value type with no inheritance. Types share behavior through traits | Inheritance would make struct sizes unknown and method calls indirect, which stops arrays of structs being contiguous and loops vectorizing. [Decision 0015](../decisions/0015-no-class-inheritance.md) |
| The order of a class's attributes does not matter | A struct's fields are stored in declaration order, so the order can change its size through padding | Arrays of structs are shared byte for byte with C, NumPy and file formats. [Field layout](types.md#field-layout) |
| A default such as `history: list[f64] = []` is created once and shared by every call that leaves the argument out | A default is evaluated afresh at each call that leaves the argument out | A shared mutable default is a well-known bug, and shared state would race in parallel loops. [Default parameter values](types.md#default-parameter-values) |
| Top-level code runs on import; modules have global variables | Only definitions at the top level; the program starts at `main` | Shared mutable state would race in parallel loops. [Source files](syntax.md#source-files) |
| A second `def f` at the top level silently replaces the first | Each top-level name is defined once per module | A replaced definition is almost always a mistake. [Source files](syntax.md#source-files) |
| `from . import x` imports relative to the current package | Imports always name modules from the package root | An import reads the same wherever the file sits. [Source files](syntax.md#source-files) |
| A leading `_` marks a name private by convention only | A leading `_` makes a name private to its module; importing it is an error | A module's private helpers stay free to change. [Source files](syntax.md#source-files) |

## What Python has that version 0 does not

`lambda` and nested functions, generators and `yield`, `async`, `global` and `nonlocal`, `del`
other than `del d[k]`, `*args` and `**kwargs`, star assignment, `:=`, `set` displays, `set` comprehensions,
generator expressions, `try` with `finally` or `else`, `raise ... from`, `for` and `while` with `else`, `with` other
than `with unsafe:`, decorators other than `@test`, `@bench` and `@layout("C")`, and the `type`
statement. The compiler rejects each with a message; none is silently ignored. Some will come
later; the [Syntax](syntax.md) chapter lists what is accepted. Class inheritance will not come: it
is left out by design (see the table above).

## Names that look like Python's modules

`math`, `random`, `sys` and `os` are this language's standard library modules, not CPython's. They follow
Python's names where Python has the function, so `from math import sqrt` reads as expected, but
`random` provides counter-based generators such as `Philox` that Python's module does not. Running
a program under CPython therefore needs a small shim library; roadmap idea I.11 proposes one, to
use CPython as a reference for checking the compiler's output.
