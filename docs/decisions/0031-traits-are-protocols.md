# 0031. A trait is a `Protocol` class, and a type implements it by naming it as a base

Status: Accepted
Date: 2026-11-09
Roadmap: D.9

## Context

[Decision 0015](0015-no-class-inheritance.md) left inheritance out of the language and promised
that types share behavior through traits. A trait is a named set of methods: "anything with
`energy()` and `mass()`". A generic function bounded by a trait works on every type that has it,
and the compiler makes one copy of the function per type, so the call costs nothing at run time.

Today only the standard library's traits exist (`Number`, `Float`, `VectorSpace`, `Copy`; see
[Traits](../reference/types.md#traits)), and a bound is written with Python 3.12's syntax,
`def rk4_step[S: VectorSpace](...)`. User code cannot declare a trait. Roadmap item 1.7.10 waits on
this record, and so does every physics program that wants one integrator, one neighbour list or one
force loop to work on several particle types.

Three questions must be answered, in valid Python syntax
([decision 0004](0004-python-syntax.md)):

1. How a trait is declared.
2. How a type says it has a trait: by naming it (nominal, as in Rust and Swift) or by merely having
   the methods (structural, as Python's `typing.Protocol` and Go's interfaces do).
3. What replaces the two things Python programmers use inheritance for: one collection holding
   several types, and a type reusing another type's fields.

Two facts about Python 3.12 constrain the spelling. `where` clauses are not Python, so a bound must
use the `[T: Bound]` form the language already has. And `[T: (A, B)]` already means something in
Python: `T` is exactly `A` or exactly `B`, not "has both", so it cannot spell two bounds.

## Decision

### Declaring a trait

A trait is a class whose bases include `Protocol`. Its methods have signatures and the body `...`.

```python
class Body(Protocol):
    """A point mass that knows its own energy."""

    def energy(self) -> f64: ...

    def mass(self) -> f64: ...
```

- `Protocol` is a built-in name, like `Struct` and `Enum`; no import is needed.
- A trait has methods only: no fields, no constructors, no type parameters and no associated types
  in version 0. A method may take `self` in any mode (`InOut[Self]`, `Owned[Self]`) and may name
  `Self`, which stands for the implementing type.
- A trait may require other traits by listing them before `Protocol`, as Python requires:
  `class ChargedBody(Body, Charged, Protocol): pass`. A type that implements `ChargedBody` must
  implement `Body` and `Charged` too, and a `T: ChargedBody` may call all their methods.
- A method in a trait with a body other than `...` is an error in version 0. Default method bodies
  are a later extension (Ideas I.31) and need no change to this spelling.

### Implementing a trait: nominal

A struct, enum or error type implements a trait by naming it among its bases:

```python
class Particle(Struct, Copy, Body):
    position: Vec3
    velocity: Vec3
    m: f64

    def energy(self) -> f64:
        return 0.5 * self.m * self.velocity.dot(self.velocity)

    def mass(self) -> f64:
        return self.m
```

- This is not inheritance. `Body` adds no fields and no code to `Particle`; it is a promise that
  `Particle` defines every method `Body` lists, which the compiler checks at the class.
- A missing method, or one whose parameters, modes, return type or `Raises` differ from the trait's,
  is an error at the class that names the trait and shows both signatures.
- A base that is a user class but not a `Protocol` stays the error decision 0015 defines. Its message
  gains a note: to share behavior, declare the base as `class Base(Protocol)`; to share fields,
  hold it as a field or `Embed` it (below).
- Only the type's own declaration can implement a trait for it. Implementing a user trait for
  `f64`, `Fixed` or a type from another package is not in version 0 (Ideas I.30).
- A type with the methods but without the base does not satisfy the trait, unlike Python's
  `Protocol`. Every program accepted here also type-checks in Python, because Python accepts a
  `Protocol` named explicitly as a base.

### Using a trait: generic functions, compiled per type

A bound is written as today:

```python
def total_energy[T: Body](bodies: Array[T, 1]) -> f64:
    total = 0.0
    for body in bodies:
        total += body.energy()
    return total
```

The function is compiled once per type it is called with, as every generic is
([decision 0018](0018-mid-level-ir.md)). After monomorphization `body.energy()` is a direct call to
`Particle.energy`, which the backend can inline and the loop can vectorize: the same machine code as
writing `total_energy` for `Particle` by hand.

A type parameter has one bound. Two traits are combined by declaring a trait that requires both.
`[T: (Body, Charged)]` is a compile error that says so, because in Python it means a choice between
two types.

### Mixed collections: explicit `Dyn[Body]`

A collection of values of different types, such as a `list` holding both `Particle` and `Ion`
values, needs a run-time table of methods. Its type is written `Dyn[Body]`:

```python
bodies: list[Dyn[Body]] = []
```

The table lookup only ever happens where `Dyn` is written, so slow dispatch cannot creep into a hot
loop unnoticed. This record reserves the spelling. Its rules (which traits can be `Dyn`, how a
`Dyn` value is owned and moved, its layout) come with roadmap item 3.5, trait objects, in a record
of their own. Until then `Dyn` is an error that says it is not available yet. A closed set of types
known in advance is often better served by an enum with data (decision D.10) and a `match`, which
stays contiguous in an array and needs no table.

### Reusing fields: `Embed`

A struct field annotated `Embed[T]`, where `T` is a struct, is stored exactly like a field of type
`T`, and its fields and methods can also be reached directly through the outer struct:

```python
class Ion(Struct, Copy, ChargedBody):
    particle: Embed[Particle]
    q: f64

    def charge(self) -> f64:
        return self.q


ion = Ion(Particle(p, v, 39.9), 1.0)
ion.m             # ion.particle.m
ion.energy()      # ion.particle.energy()
```

- Layout does not change: the embedded struct sits inline at the field's position, and the padding
  warning and `@layout("C")` treat it as one field. Reaching `ion.m` compiles to the same offset as
  `ion.particle.m`; there is no run-time cost.
- The outer struct is built and moved like any struct holding a field: `Ion(Particle(...), 1.0)`.
- A name the outer struct defines itself hides an embedded one. Two embedded structs that both
  provide a name, at the same depth, is an error at the class that names both, never a silent pick
  at the use site. Embedding is transitive: an embedded struct's own embedded fields are reached
  too, one level deeper.
- A promoted method satisfies a trait the outer struct names. `Ion` above names `ChargedBody`, and
  `energy` and `mass` come from `Particle`. Embedding alone implements nothing: the trait must be
  named.
- A promoted method still receives the embedded struct as `self`, not the outer one. That is the
  difference from inheritance: `Particle.energy` cannot call a method that `Ion` redefines. A type
  that needs that pattern takes the varying part as a trait-bounded parameter instead.
- Only structs can be embedded, and a struct cannot embed itself, directly or through another.

## Consequences

- A generic physics kernel written once runs at the speed of one written per type. Traits add no
  run-time cost unless `Dyn` is written, and `Dyn` is always visible in the source.
- Errors stay local. A type that does not have a trait's method is reported at its own class, not
  at some distant call, and a bound violation is reported at the call that violates it.
- Python programmers write a familiar `class Body(Protocol)`, and tools such as Pyright still
  understand trait declarations, implementations and bounds. They do not understand `Embed`'s field
  promotion or `Dyn`, which are this language's own.
- There are no inherited fields beyond `Embed`, and no overriding. Porting a Python class hierarchy
  means turning the base into a `Protocol` and the shared fields into an embedded struct.
- Each type a generic function is used with gets its own compiled copy, so code size and compile
  time grow with the number of types. Scientific programs use few types per kernel; roadmap 3.4
  deduplicates identical instances.
- Nominal implementation keeps a door open: accepting structural satisfaction later would only
  accept more programs, while moving from structural to nominal would break programs that rely on
  matching by accident.
- Not in version 0, each an error that names this record: default method bodies, traits with type
  parameters or associated types, implementing a trait outside the type's declaration, more than one
  bound per type parameter, and `Dyn` until roadmap 3.5.

## Alternatives considered

### Structural traits, as Python's `Protocol`

A type would satisfy `Body` by having `energy` and `mass` with the right signatures, without naming
it. This is Python's own meaning of `Protocol` and needs no base. Rejected because a type can
satisfy a trait by accident (two unrelated `energy` methods with different units), a missing method
is reported at whichever call first needs it rather than at the type, and the compiler must check
satisfaction at every instance instead of once per class. It can still be added later without
breaking a program.

### A marker of our own: `class Body(Trait)`

Avoids reusing Python's name with a stricter meaning. Rejected because Python tools would not
recognise it, and the stricter meaning costs nothing in practice: explicitly naming a `Protocol` is
already valid, common Python.

### Implementing traits with a decorator: `@implements(Body)`

Would allow implementing a trait for a type declared elsewhere, such as `f64`. Rejected for version
0 because Python's type checkers ignore it, and it raises coherence questions (two packages
implementing one trait for one type) that the narrow version avoids. Ideas I.30 tracks it.

### Inheritance, reopening decision 0015

`class Ion(Particle)` with inherited fields and overridable methods, as Python, C++ and Java do.

- **Run-time cost.** When a `list[Particle]` may hold an `Ion`, each element is a pointer to a value
  of unknown size and each `energy()` call goes through a table of methods. The table load is cheap;
  the loss is that the compiler cannot inline the call, so the loop cannot vectorize. A loop over
  `f64` values loses up to 2x on Apple silicon's 128-bit NEON, 4x with AVX2 and 8x with AVX-512,
  plus a cache miss per element once the values are scattered on the heap instead of contiguous.
- **A free version exists but misleads.** Fields copied at compile time, and overrides compiled per
  exact type, cost nothing. But then a `list[Particle]` still cannot hold an `Ion`, which is exactly
  what a Python programmer expects inheritance to allow. It is `Embed` plus traits with a spelling
  that promises more than it delivers.
- **Reasoning.** A change to a base class changes every subclass, and method lookup order across
  several bases is a well-known source of bugs.

Rejected, for the reasons 0015 gives and these.

### No `Embed`: composition only

Holding a `Particle` as a plain field already shares its data; `ion.particle.m` works today. Kept
`Embed` because field reuse is the part of inheritance Python programmers will miss most, and
writing `.particle.` on every access, or a forwarding method per trait method, is the boilerplate
that pushes people back toward inheritance. `Embed` costs a name-lookup rule in the checker and
nothing at run time. Go has had the same feature since its first release.
