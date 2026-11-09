# 0032. A value that is one of several kinds is a union of structs, taken apart with `match`

Status: Accepted
Date: 2026-11-09
Roadmap: D.10

## Context

A sum type is a type whose value is exactly one of a fixed set of kinds, each kind with its own
data. Rust calls it an enum with data, Swift an enum with associated values, Haskell an algebraic
data type. Physics code needs one wherever a choice carries numbers:

- a boundary condition is periodic, or Dirichlet with a value, or Neumann with a flux, or Robin with
  three coefficients;
- an event in event-driven molecular dynamics is a collision between particles `i` and `j` at time
  `t`, or particle `i` hitting wall `w` at time `t`;
- a solver step converged after `n` iterations with residual `r`, or diverged at step `k`;
- a shape in a collision or ray-tracing code is a sphere with a radius or a box with two corners.

Today's language has `class Boundary(Enum)`, whose members are plain constants
([Enums](../reference/types.md#enums)). A program that needs data per kind must use a struct with a
kind field and every field of every kind, most of them unused, and nothing stops it from reading
the flux of a Dirichlet boundary. Roadmap D.10 asks how to spell the real thing.

The constraints:

1. Every file is valid Python 3.12 syntax ([decision 0004](0004-python-syntax.md)), and Python
   tools such as Pyright should understand as much of the program as they can, as
   [decision 0031](0031-traits-are-protocols.md) required of traits.
2. There is no inheritance ([decision 0015](0015-no-class-inheritance.md)), so a kind cannot be a
   subclass of the sum type.
3. `match` must stay exhaustive: adding a kind must point at every `match` that does not handle it,
   as adding an enum member does today.
4. It must fit decision 0031's traits. 0031 sends a closed set of types known in advance to "an enum
   with data and a `match`" rather than to `Dyn`, so a sum type of types that implement `Body` should
   be usable where a `Body` is expected, without a table of methods.
5. A sum type should be a plain value: storable contiguously in an `Array`, `Copy` when its parts
   are, and with no allocation.

The language already has two unions written with `|`: `T | None` for optional values
([Optional values](../reference/types.md#optional-values)) and `Raises[T, ParseError | IOError]` for
several error types ([Errors](../reference/errors.md)). Both are sum types already, of a narrow kind.

## Decision

**A union of structs** (option A, chosen by Collins on 2026-11-09). Each kind is an ordinary struct. The sum type is the
union of those structs, written with `|` and usually named with a type alias. A value is taken apart
with `match` and Python's class patterns.

### Declaring

```python
class Periodic(Struct, Copy):
    pass


class Dirichlet(Struct, Copy):
    value: f64


class Neumann(Struct, Copy):
    flux: f64


class Robin(Struct, Copy):
    a: f64
    b: f64
    g: f64


Boundary = Periodic | Dirichlet | Neumann | Robin
```

- A union's members are structs, error types, `Enum` types and `None`. A number, array, string,
  type parameter or another union written inside it is an error in version 0; a nested union is
  flattened instead (`(A | B) | C` is `A | B | C`), as Python does.
- Order does not matter and repeats collapse: `Dirichlet | Periodic` is the same type as
  `Periodic | Dirichlet`, as in Python. The alias is a name, not a new type, as every alias is
  ([Type aliases](../reference/types.md#type-aliases)).
- A kind with no data is a struct with no fields, constructed as `Periodic()`.
- The existing `T | None` is the special case of one struct and `None`; nothing about it changes.

### Making a value

A member's value is accepted wherever the union is expected, as a `T` is accepted where a
`T | None` is:

```python
def solve(left: Boundary, right: Boundary) -> Array[f64, 1]: ...

temperature = solve(Dirichlet(300.0), Neumann(0.0))
```

The reverse needs a check. A `Boundary` is never used as a `Dirichlet` until a `match` has shown it
is one.

### Taking it apart

`match` uses Python's class patterns, by keyword or by position in field order (the order Python's
dataclasses give `__match_args__`):

```python
def ghost_value(boundary: Boundary, inside: f64, dx: f64) -> f64:
    """The temperature one cell outside the edge that enforces `boundary`."""
    match boundary:
        case Periodic():
            return inside
        case Dirichlet(value=v):
            return 2.0 * v - inside
        case Neumann(flux):
            return inside + flux * dx
        case Robin(a, b, g):
            return (g * dx + (b - 0.5 * a * dx) * inside) / (b + 0.5 * a * dx)
```

- Exhaustiveness extends the existing rule: a `case` for each member, or a final `case _:`. Leaving
  out `Robin` is a compile error that names `Robin`, so adding a kind to `Boundary` points at every
  `match` that must handle it. Pyright reports the same missing case.
- A capture binds by the subject's mode: a `Copy` field is copied, other fields are read-only
  borrows when the subject is borrowed and moved when it is owned, as for any field access
  ([Values and references](../reference/values-and-references.md)).
- Class patterns work on a plain struct subject too, which is useful by itself.
- In version 0 `match` is the only way to narrow a union. `isinstance` is not added.

### Methods and traits

A union has no declaration of its own, so it has no methods of its own. It reaches its members'
methods in two ways, both resolved at compile time to a `match` over the kinds, never a table:

1. **A shared method.** `x.m(...)` on a union compiles when every member defines `m` with the same
   parameters, modes, return type and `Raises`. It runs the member's `m`. Pyright accepts the same
   call, because it allows an attribute that every member of a union has.
2. **A shared trait.** A union satisfies a bound `[T: Body]` when every member names `Body` as a
   base. The generic function is compiled once for the union, and each `body.energy()` inside it
   becomes a `match` that calls the member's method.

```python
class Atom(Struct, Copy, Body):
    position: Vec3
    velocity: Vec3
    m: f64

    def energy(self) -> f64:
        return 0.5 * self.m * self.velocity.dot(self.velocity)

    def mass(self) -> f64:
        return self.m


class Dimer(Struct, Copy, Body):
    first: Atom
    second: Atom
    bond_length: f64

    def energy(self) -> f64:
        return self.first.energy() + self.second.energy()

    def mass(self) -> f64:
        return self.first.m + self.second.m


Molecule = Atom | Dimer

molecules: Array[Molecule, 1] = ...
kinetic = total_energy(molecules)   # decision 0031's total_energy[T: Body], no Dyn
```

This is the closed-set alternative to `Dyn[Body]` that decision 0031 points to: the kinds are known
when the program is compiled, values sit inline in the array, and each call is a branch the
processor predicts well rather than an indirect call the compiler cannot inline.

### Layout and cost

- A union value is a tag followed by room for its largest member, aligned for its most aligned
  member. The tag is the smallest unsigned integer that counts the members, and members are
  numbered in a fixed order (by their full path), so the same union has the same layout in every
  module. Version 0 does not promise the layout to C; `@layout("C")` on a union is an error.
- A union of `Copy` members is `Copy`; otherwise it moves, as a struct does.
- An `Array[Molecule, 1]` stores each element at the size of `Dimer`, the larger member. Mixing a
  1-field kind with a 20-field kind wastes the difference in every small element. The reference
  will say so, and that a hot loop over millions of values of one kind is faster as its own array.
- A union cannot contain itself, directly or through a member's field: its size would be infinite.
  Recursive data, such as an expression tree, waits for heap boxes.

### Errors reuse the same rule

`Raises[T, SingularMatrix | NotConverged]` is already a union of error types. Under this record it
is the same union, so `except` and `match` agree. An outcome that is not an error is a union of
plain structs, returned as a value:

```python
Outcome = Converged | Diverged

def integrate(state: InOut[State], steps: i64) -> Outcome: ...

match integrate(state, 10_000):
    case Converged(iterations, residual):
        print(iterations, residual)
    case Diverged(step):
        print("diverged at", step)
```

## Consequences

- No new class base, keyword or type constructor. The feature reuses `Struct`, `|`, type aliases
  and Python's class patterns, and generalises the two unions the language already has. The
  checker gains union types with struct members, class patterns, and the two method rules; the
  backend gains one tagged layout, which `T | None` already approximates.
- Pyright understands the whole spelling: construction, assignment to the union, `match` narrowing,
  exhaustiveness, and shared method calls. Only the trait-bound rule is this language's own.
- A kind is a type on its own. A function can take just a `Dirichlet`, and one struct can belong to
  several unions (`Converged` in `Outcome` and in a `SweepOutcome`). This is how typed Python
  models sum types today, so Python programmers already know it.
- There is no namespace: the kinds are `Dirichlet`, not `Boundary.Dirichlet`. A module that needs
  the prefix imports the module, `bc.Dirichlet`. Two unions that want a kind called `Converged` with
  different fields need two struct names.
- A union cannot hold a method that is not on its members. Logic about the whole union is a free
  function with a `match`, as `ghost_value` above.
- Small elements of a union array waste the size difference between kinds; the reference documents
  the cost.
- Not in version 0, each an error that names this record: numbers, arrays, strings and type
  parameters as members; generic unions; `isinstance` narrowing; recursive unions; a C layout.

## Alternatives considered

### B. Kinds as classes nested in an `Enum`

```python
class Boundary(Enum, Copy):
    class Periodic:
        pass

    class Dirichlet:
        value: f64

    class Neumann:
        flux: f64

    def ghost_value(self, inside: f64, dx: f64) -> f64:
        match self:
            case Boundary.Dirichlet(value=v):
                return 2.0 * v - inside
            case _:
                return inside

left = Boundary.Dirichlet(300.0)
```

The closest to Rust and Swift: one declaration, namespaced kinds, and methods and trait bases on
the sum type itself (`class Boundary(Enum, Copy, Body)`). Valid syntax, and the strongest option if
Python tools did not matter. Not recommended, because:

- Pyright types `Boundary.Dirichlet(300.0)` as a `Boundary.Dirichlet`, which is not a `Boundary`
  without inheritance, so every `solve(Boundary.Dirichlet(300.0), ...)` is a Pyright error. Python
  itself (since 3.13) treats a nested class in an `Enum` as a plain attribute, not a member.
- It adds a third union beside `T | None` and error unions, with its own rules for construction,
  patterns and exhaustiveness, where option A extends the ones that exist.
- A nested class is a new kind of class (no `Struct` base, only legal inside an `Enum`) to teach and
  check.

Its advantages, namespacing and methods on the sum type, are real but small: a module prefix gives
the first, and a free function or a shared trait gives the second.

### C. A closed trait whose implementers are the kinds

```python
@final
class Boundary(Protocol):
    def ghost_value(self, inside: f64, dx: f64) -> f64: ...


class Dirichlet(Struct, Copy, Boundary):
    value: f64

    def ghost_value(self, inside: f64, dx: f64) -> f64:
        return 2.0 * self.value - inside
```

The trait marked closed, and used as a type, would mean "one of the types in this module that name
it", as Scala's and Kotlin's sealed classes do. Methods are declared once on the trait and Pyright
accepts every assignment. Not recommended, because:

- It blurs the line decision 0031 drew: there a trait name used as a value type is an error, and
  `Dyn[Body]` is required so that dispatch is visible. Here `Boundary` as a type would be legal and
  silently mean a tagged union.
- The set of kinds is implicit: it is every type in the module that names the trait, so a reader
  must search the module to know what a `match` must handle, and Pyright cannot check exhaustiveness
  without that set.
- `@final` on a `Protocol` already means something else in Python (it cannot be subclassed), so the
  spelling would mislead.

### D. Members whose values describe the data

```python
class Boundary(Enum):
    PERIODIC = ()
    DIRICHLET = (f64,)
    ROBIN = (f64, f64, f64)
```

Keeps the existing `Enum` form and adds data as tuples of types. Rejected outright: fields have no
names, types appear where Python expects values, and `Boundary.DIRICHLET(300.0)` is a call of an
enum member, which Pyright and Python both reject.
