# 0038. Standard-library classes are Baryon source shipped inside the compiler

Status: Accepted
Date: 2026-11-10
Roadmap: 1.13.6

## Context

[Decision 0036](0036-files-and-processes.md) adds the first standard-library types that are plain
data: `OSError`, an `Exception` with four fields, and `ErrorKind`, an `Enum` of ten members. Later
slices of the same record add more: `subprocess.CompletedProcess` (`returncode`, `stdout`,
`stderr`) and `pathlib.Path`. Until now the compiler has built in every standard type as a
`types::StandardType` ([decision 0028](0028-philox-random-numbers.md)): a variant the checker,
lowering, the backend and drop elaboration each know by name. That suits `random.Philox`, whose
operations are runtime calls and whose fields a program never reads.

A data type is different. A program reads `error.kind` and `error.filename`, compares
`error.kind == ErrorKind.FILE_NOT_FOUND`, catches `except OSError as error`, prints the error at
the end of `main`, and the error's two `str` fields must be freed. A user-defined
`class OSError(Exception)` gets every one of those from code that already exists. A
`StandardType` gets none of them: each would be a new special case in four crates, and again for
every data type after it.

The forces:

1. The project's goal is a standard library written in Baryon (`ROADMAP.md`, "Baryon stands on
   its own"), so a mechanism that moves toward it beats one that moves away.
2. The best part is no part: field access, enum constants, `==`, `except`, printing and drops
   should not be built twice.
3. Some operations cannot be written in Baryon yet: opening a file, reading the environment.
   Those stay runtime built-ins, and a type whose whole value is a runtime handle, such as
   `File`, stays a `StandardType`.
4. A user's module must never be shadowed or broken by the library's, and the library must not
   slow down a build measurably.

## Decision

Collins accepted this on 2026-11-10, choosing the library written in Baryon over more built-in types.

The compiler carries standard-library source files written in Baryon, embedded in the `driver`
with `include_str!` and added to every package as ordinary files under the reserved root
`<std>/`, such as `<std>/os.byn`. They are parsed, resolved, checked and lowered like the user's
files, so their classes are ordinary `Struct`, `Exception` and `Enum` types.

- A standard module such as `os` is still one of `database::STANDARD_MODULES`. A member that the
  module's library file defines, such as `os.ErrorKind`, resolves to that definition; any other
  member, such as `os.getenv`, stays a built-in as today.
- A name the language provides that the library defines, such as `OSError`, resolves to the
  library's definition. The library file that holds such names is `<std>/builtins.byn`.
- A user file cannot be named into `<std>/`, so it never clashes with a library module, and
  diagnostics never point into a library file because the library is checked by the test suite.
- A type whose value is a runtime handle (`File`) or whose operations are runtime code (`Philox`)
  stays a `StandardType`, and its methods stay built-ins.
- The runtime writes an `OSError` into memory laid out as the class declares it, so the class's
  field order is part of the runtime's interface; a test in the runtime pins it.

The first library files hold `OSError` and `ErrorKind`, exactly as decision 0036 part 2 writes
them.

**Alternative: more `StandardType` variants.** `OSError` and `ErrorKind` as built-in types, with
field access, enum constants, comparison, printing and drops taught to the checker, lowering,
the backend and drop elaboration for each. It needs no new resolution rule, but it repeats work
that exists, and repeats it again for `CompletedProcess`, `Path` and every data type after them.

## Consequences

- `OSError`, `ErrorKind` and later data types cost one class each, with no new compiler code.
  Go to definition and hover in the editor show their source.
- Every build parses and checks the library files: a few hundred lines today, well under a
  millisecond. A library that grows large would be cached or checked once; that is a later
  decision.
- Resolution gains one rule: a standard member or built-in name may be defined by a library
  file. The resolver, not each later stage, applies it.
- The library files must stay valid for every check the compiler runs on user code, including
  the padding warning, and a change to them is a change to the language.
