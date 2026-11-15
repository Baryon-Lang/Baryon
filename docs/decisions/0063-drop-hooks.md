# 0063. A class's `__del__` runs when its value is dropped

Status: Proposed
Date: 2026-11-14
Roadmap: 1.7.15

## Context

A value in Baryon has one owner, and the compiler drops it at a known point: when its owner's
function returns, when the variable holding it is given a new value, or at the end of the
statement that made a temporary ([Values and references](../reference/values-and-references.md)).
Dropping frees what the value owns: arrays, lists, dicts and strings, and a `File`, which it
closes and writes out ([decision 0036](0036-files-and-processes.md), part 3).

A class written in Baryon cannot do the same. Dropping a struct frees its fields, and runs none
of its own code. That matters for a class that holds work not yet done:

- `gzip.open(path, "wb")` gives a `GzipFile`, a class the library writes in Baryon
  ([decision 0038](0038-standard-library-in-baryon.md)). It gathers compressed data and writes it
  at `close()`. `f = gzip.open(p, "wb"); f.write(data)` with no `close()`, common in scripts,
  leaves an empty file with no error. CPython writes the file when the object is collected.
- Any other writer that holds data until `close()` would have the same gap. `GzipFile` is the
  one that exists today. `h5by` does not need the hook: each of its operations writes a
  complete, closed file, and nothing waits for `close()`.
- A program's own class that holds a lock, a timer or a buffer has nothing to run at its end
  unless every caller uses `with` ([decision 0053](0053-with-and-for-on-user-classes.md)).

Python's answer is `__del__`, which CPython calls when an object's reference count reaches zero.
Rust's is the `Drop` trait, whose `drop(&mut self)` runs when a value goes out of scope. Baryon's
drops come at the points Rust's do, so the two agree on when; they differ on what is allowed.

Collins chose on 2026-11-14 at 05:35 UTC (card 36, "Add `__del__`, user classes too"): the
compiler calls `__del__(self: InOut[Self])` when a value is dropped, for a program's own classes
as well as the library's, and an error that escapes it panics, as a `File` that cannot be written
out when it is dropped panics. The card is silent on the rest. This record follows Rust's `Drop`
where it is silent, and lists each such choice in part 9.

## Decision

1. **The method.** A struct class may define `def __del__(self: InOut[Self])`, returning `None`
   or `Raises[None, E]`. `self` is the whole value, still owning every field. Any other
   signature is a compile error that shows this one.

2. **When it runs.** Exactly once for each value of the class, when the value is dropped:
   - when the function that owns it returns or raises, for a variable or an `Owned` parameter;
   - when the variable, field or element holding it is given a new value, for the old value;
   - at the end of the statement that made it, for a temporary nothing kept;
   - when what holds it is dropped: a list, a `dict`, a tuple, an optional, a union, or another
     struct's field.

   A value that is moved is not dropped where it was: its new owner drops it, once. A value
   returned from a function is dropped by the caller. Variables a function owns are dropped
   last declared first, as Baryon drops them today.

3. **Order.** `__del__` runs first, on the whole value. Then the value's fields are dropped, in
   the order they are declared, and a field whose class has `__del__` runs its own then. So an
   outer value is finished before the values it holds, as in Rust.

4. **Errors.** An error that escapes `__del__` cannot be raised at a drop, which may be at the end
   of a function that raises nothing. The program prints the error as an uncaught error is
   printed, then panics: `` `GzipFile.__del__` raised the error above when its value was dropped ``.
   Code inside `__del__` may still catch errors with `try` as anywhere else.

5. **Panics and `sys.exit`.** A panic ends the program at once, and `sys.exit` ends it after
   writing out open `File`s; neither runs `__del__` for the values still alive. This is Rust's
   rule for a panic that aborts and for `process::exit`. A program that must finish a value on
   every path uses `with` or calls the method itself before it exits.

6. **What the checker forbids.**
   - Calling `__del__` by name, `value.__del__()`: the compiler calls it, and a second call
     would run it twice. The error says to give the class a method such as `close()` and call
     that from `__del__`.
   - Moving a field out of a value whose class has `__del__`, as `kept = log.buffer`: `__del__`
     needs the whole value, inside `__del__` as anywhere else. Copying a field of a copy type
     is allowed, and so is taking a copy of any field with `.copy()`.
   - `__del__` on a class declared `Copy`: a copy type is copied, never dropped.
   - `__del__` on a generic class, for now: each instance needs the hook compiled at its type
     arguments, which is not built yet. The error says so.

7. **What it is not.** `__del__` is found on the class itself, as every dunder method is
   ([decision 0053](0053-with-and-for-on-user-classes.md), part 6), not through `Embed`; an
   embedded field of a class with `__del__` still runs its own `__del__` when it is dropped as
   a field. Enums and unions have no methods, so they have no `__del__`; a union holding a
   struct with one runs it when the union is dropped.

8. **The library uses it.** `GzipFile.__del__` calls `close()`, so a file opened to write is
   written out when it is dropped, as in CPython, and a failure to write it panics, as a
   dropped `File`'s does.

9. **Choices the card left open, settled by Rust's `Drop`.** Each is above; listed here so they
   can be reviewed together:
   - `__del__` runs before the fields are dropped (part 3);
   - a moved value runs it once, where its last owner drops it (part 2);
   - a panic and `sys.exit` do not run it (part 5);
   - calling it by name is an error (part 6);
   - moving a field out of a value that has it is an error (part 6);
   - a `Copy` class cannot have it (part 6);
   - a generic class cannot have it yet (part 6).

## Alternatives

- **No hook; document the gap.** A `GzipFile` dropped without `close()` would stay an empty file
  with no error. Silent data loss is the failure the mission most needs to rule out.
- **Write out on every `write`.** The library could rewrite the gzip file after each member, so
  a dropped file is never empty. Data since the last member is still lost, and each member
  costs a rewrite of the whole file. It hides the gap without closing it.
- **A library-only hook.** Collins chose to give it to user classes too: a program's own buffer
  or writer has the same need.
- **CPython's rules.** CPython ignores an error in `__del__` after printing it, may run `__del__`
  at exit, and may not run it at all for objects in a reference cycle. Ignoring the error turns
  data loss into a message that scrolls away; Baryon has no cycles and no collector, so its drop
  points are exact and Rust's rules fit them.

## Consequences

- A class can promise its own cleanup, and `GzipFile` finishes its file whether or not a
  script closes it.
- Drop code in the compiler calls a function for such a class: the backend's drop of a struct
  calls `__del__` first, and drop elaboration keeps the drop of a struct that has `__del__` even
  when its fields own nothing.
- A class with `__del__` cannot give away its fields; such a class offers a method that moves
  them out and leaves the value finished, or the caller copies.
- A program that relies on `__del__` at a panic or at `sys.exit` loses that work; `with` is the
  way to finish a value on every path.
- Generic classes with `__del__` wait for the hook to be compiled per instance.
