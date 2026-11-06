# 0017. `@layout("C")` pins a struct's layout and silences the padding warning

Status: Accepted
Date: 2026-11-06
Roadmap: I.14

## Context

A struct stores its fields in the order they are declared, each at the next offset its alignment
allows, as a C struct does; the compiler never reorders them (language reference, "Types", "Field
layout"). When another order would take fewer bytes, the type checker warns at the class name and
names that order (roadmap 1.7.1).

Sometimes the order is fixed on purpose: the struct mirrors a record in a binary file format, a
packet header or a C library's struct, and moving a field would break the match. The author of
such a struct cannot act on the warning, so it must be possible to turn it off for that struct,
and the reason should be visible in the source.

Every program is valid Python ([decision 0004](0004-python-syntax.md)), so the spelling must be
Python syntax. A comment such as `# noqa` would be invisible to the compiler's tree and says
nothing about why. A base such as `class Header(Struct, CLayout)` would mix layout into the bases,
which say what a class *is* ([decision 0015](0015-no-class-inheritance.md)). A decorator reads as
a statement about the class and is how Python attaches such facts; the language already has
`@test` and `@bench`.

## Decision

- `@layout("C")` on a struct, a class with the base `Struct` or `Struct, Copy`, declares that its
  layout is a contract: fields in the order written, each at the next offset its alignment allows,
  with the padding that leaves. The compiler does not warn about padding for that struct.
- `"C"` is the only argument. Any other argument, or none, is a compile error. The string leaves
  room for other layouts later, such as a packed one, without a new name.
- `@layout` on an enum, an error type or a function is a compile error. Enums are tags and error
  types are compiled to error returns, so neither is shared with other programs byte for byte.
- `layout` joins the names the language adds, always in scope.

## Consequences

- An author whose layout is fixed by something outside the program says so once, at the class,
  and the warning stops without hiding padding anywhere else.
- A reader sees which structs are contracts with other programs and must not be reordered.
- The layout of a struct without `@layout("C")` is the same today. If the language ever lets the
  compiler reorder fields, structs that say `@layout("C")` keep their order, so programs written
  now stay correct.
