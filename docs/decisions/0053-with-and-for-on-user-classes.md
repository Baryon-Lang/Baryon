# 0053. `with` and `for` work on any class with Python's dunder methods

Status: Accepted
Date: 2026-11-13
Roadmap: 1.7.14

## Context

Two statements work only on values the compiler knows. `with` takes `open(...)`, a NetCDF
`Dataset`, an `h5by` `File` and `unsafe` ([decision 0036](0036-files-and-processes.md),
part 3). `for` takes `range`, lists, arrays, strings, dicts, files and a frame's rows. A
program cannot write its own.

That leaves out things physicists write all the time:

- a timer, a profiler region or a log section, opened and closed around a block;
- a compressed file, `gzip.open(path)`, which the library writes in Baryon
  ([decision 0038](0038-standard-library-in-baryon.md)) and has to close and finish on every path
  out of the block (roadmap 3.27.4 and 3.27.10);
- a time stepper, a Markov chain or a reader of records that hands out one value per pass,
  `for state in Integrator(...)`.

Python's answer is two protocols, each a pair of dunder methods (method names written with
double underscores). `with m as x:` calls `m.__enter__()` and binds its result, then calls
`m.__exit__(...)` however the block ends. `for x in v:` calls `iter(v)`, which calls
`v.__iter__()`, then `__next__()` on the result until it raises `StopIteration`.

Collins approved Python's dunder methods for both statements on 2026-11-13 at 02:49 UTC ("Go
with all recommendations for 1-9", item 8 of the open questions). He rejected the other
options: library-only `with` forms, and Baryon names of our own. What remains to settle is how
the two protocols fit the language's ownership rules
([Values and references](../reference/values-and-references.md)), errors as values
([decision 0005](0005-error-handling.md)), and traits
([decision 0031](0031-traits-are-protocols.md)).

## Decision

### 1. The methods are found by name on the class, as in Python

A struct or error type that defines `__enter__` and `__exit__` can be used in `with`. One that
defines `__next__`, or `__iter__`, can be used in `for`. No trait is named and nothing is
imported. The statement looks the methods up on the value's class at compile time and calls
them directly, so the protocol costs what the two calls cost and nothing more.

This is not a second trait mechanism. Decision 0031's traits are for bounds on type parameters,
and they stay nominal. These two statements are syntax that calls methods by fixed names, the way
`a + b` will call `__add__` once D.19 settles operators on user types. A generic function that
wants "any context manager" or "any iterator" needs standard traits for it, `ContextManager` and
`Iterator`. Those would join the standard traits of decision 0031 and be named as bases like any
other. They are idea I.59, left out of this record.

A method reached only through an `Embed` field is not used in version 0. The class must define
the dunder itself.

### 2. `with`: `__enter__` takes ownership and returns the value; `__exit__` runs on every exit

```python
class Section(Struct):
    name: str

    def __enter__(self: Owned[Self]) -> Self:
        print(f"begin {self.name}")
        return self

    def __exit__(self):
        print(f"end {self.name}")


def main():
    with Section("solve") as section:
        solve()
```

- **`__enter__(self: Owned[Self]) -> Self`.** Python's `__enter__` usually ends in
  `return self`. Under the ownership rules a method can return `self` only when it owns it, so
  `__enter__` takes `self` as `Owned` and returns `Self`. The value moves into the `with`, and
  then into the name after `as`. It may also return `Raises[Self, E]`. If it fails, the block
  does not run and `__exit__` is not called, as in Python.
- **`__exit__(self)`** takes `self` in any mode and returns `None` or `Raises[None, E]`. It is
  called on the value `__enter__` returned, which Python's `return self` makes the same value
  Python calls it on. Taking `self` as `Owned` consumes it, which is how a file finishes and
  closes itself. A plain or `InOut` `self` leaves it bound after the block, as Python leaves the
  name bound.
- **It runs on every way out of the block:** the end of the block, `return`, `break`,
  `continue`, and an error raised in the block or passed on from a call. On each of those the
  compiler places the call before the jump, so there is no unwinding and no hidden table. A panic
  stops the program and runs nothing, as everywhere else ([decision 0005](0005-error-handling.md)).
- **`__exit__` takes no other parameters.** Python passes the exception type, value and
  traceback, and a true return value suppresses the exception. Baryon has no exception objects
  and no tracebacks, and an error is handled by `try`, which the reader can see. So `__exit__`
  sees nothing of the error and cannot swallow it. A Python `__exit__(self, exc_type, exc, tb)`
  is a compile error that says to drop the extra parameters.
- **An error from `__exit__` is an error of the `with` statement.** The enclosing function must
  allow it, or a `try` around the `with` must handle it, as for any call. If the block was
  already leaving with an error, the one from `__exit__` replaces it, as in Python. Errors do
  not chain ([decision 0034](0034-no-error-chaining.md)).
- **The name after `as` stays put inside the block.** It may be read, passed and changed in
  place, but not moved away or assigned a new value, since `__exit__` must find it there. Either
  is a compile error that names the `with`.
- **Several items nest:** `with a as x, b as y:` is `with a as x:` around `with b as y:`, so
  `y` exits first.
- Without `as`, the value lives in a hidden variable until `__exit__`.

### 3. `for`: `__next__` raises `StopIteration` to end the loop

```python
class Countdown(Struct):
    left: int

    def __iter__(self: Owned[Self]) -> Self:
        return self

    def __next__(self: InOut[Self]) -> Raises[int, StopIteration]:
        if self.left == 0:
            raise StopIteration()
        self.left -= 1
        return self.left + 1


def main():
    for n in Countdown(3):
        print(n)
```

- **`StopIteration` is an error type the language provides**, with no fields, like `EOFError`. It
  is raised and declared like any error ([decision 0005](0005-error-handling.md)), so ending the
  loop costs one branch on the returned value. That is the same branch Rust spends on `None`
  from `Iterator::next`.
- **`__next__(self: InOut[Self]) -> Raises[T, StopIteration]`.** The loop calls it once a pass
  and binds the result to the loop variable, which owns it. `StopIteration` ends the loop.
  Any other error `__next__` lists, `Raises[str, StopIteration | OSError]`, passes on from the
  `for` statement like any call's error. A `__next__` that does not list
  `StopIteration` is a compile error, since the loop could never end.
- **An iterator is used as it is.** When the class defines `__next__`, the loop calls it on the
  value directly and does not call `__iter__`. Python calls `__iter__` first, but an iterator's
  `__iter__` returns `self`, so the result is the same. Skipping it is what lets a loop borrow an
  iterator held in a variable rather than consume it: `for line in f:` inside
  `with gzip.open(path) as f:` reads lines and leaves `f` for `__exit__`. A variable is borrowed as
  `InOut` for the loop and keeps its state after it, so a second loop continues where the first
  stopped, as in Python. A temporary, `Countdown(3)`, is owned by the loop and freed after it.
- **An iterable gives an iterator.** When the class defines `__iter__` but not `__next__`, the
  loop calls `__iter__` once, in whatever mode it declares, and loops over the iterator it
  returns, which must define `__next__`. A list-like class can then hand out a fresh iterator each
  time and keep its data.
- The loop variable is a name or a tuple of names, as for any loop. Comprehensions and `prange`
  over a user iterator are not in version 0 and are compile errors that say so.

## Alternatives

- **Named traits, `class Section(Struct, ContextManager)`.** Fits decision 0031 to the
  letter, but every Python program would need an edit to compile, and the statement would still
  call the same methods. Kept for generic bounds only (idea I.59).
- **`__next__` returns `T | None`, with `None` ending the loop.** Cheaper to write and no new
  error type, but it is not Python. A `__next__` copied from Python raises `StopIteration`, and a
  `T` that is itself optional could not tell its `None` from the end. Rejected.
- **`__enter__` borrows `self` and the `as` name borrows the manager.** Needs a borrow that
  outlives one call, which the ownership rules rule out on purpose. Rejected.
- **Pass the error to `__exit__`.** Needs a value that can hold any error type, which is
  `Dyn` (roadmap 3.5), and suppressing errors by return value hides control flow that `try`
  shows. Left out of version 0. A later record can add it without breaking `__exit__(self)`.

## Consequences

- `with` and `for` read the same in Baryon and Python for classes that follow Python's usual
  shapes. The differences are three annotations, `Owned[Self]`, `InOut[Self]` and
  `Raises[..., StopIteration]`, which
  [Differences from Python](../reference/differences-from-python.md) lists once the two
  statements are built.
- The standard library can write its own context managers and iterators in Baryon:
  `gzip.open`, `lzma.open` and `bz2.open` return library classes, so no compiler special case is
  added for each.
- `__exit__` is compiled once at each exit path of the block, and each error type that can
  leave the block is its own path. A block with many exits makes more code, never slower code.
- The compiler gains a list of open `with` blocks while lowering a function. `return`,
  `break`, `continue` and errors consult it, as they already consult the open loops and `try`
  handlers.
- Python's three-argument `__exit__`, suppression, `contextlib`, generators as iterators
  (`yield`), and protocols reached through `Embed` are left out. Each is a compile error that
  says what to write instead.
