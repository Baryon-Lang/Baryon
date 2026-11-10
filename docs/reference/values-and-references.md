# Values and references

[What builds today](status.md#values-and-references) says which parts of this chapter the compiler
builds into programs yet.

This chapter is what makes the language as safe as Rust without a garbage collector, and it is the
one with no Python equivalent. It is written as a decision procedure: a finite list of checks the
compiler runs, each with the error it reports. The Phase 0 gate requires exactly this.

The rules come from [decision 0006](../decisions/0006-parameter-passing.md) and the design plan's
memory model. Three ideas carry them:

1. Every value has one owner, and is destroyed when its owner goes away.
2. A function borrows its arguments for the length of the call, unless it says otherwise.
3. While a function can change a value, nothing else can see that value.

## Ownership

Every value has exactly one owner: a variable, a field of a struct, an element of a container, or a
temporary inside an expression.

A value is destroyed when its owner is destroyed:

- a local variable's value, when the function returns, on every path, including through a `raise`;
- a variable's old value, when a new value is assigned to it;
- a field's or element's value, when the struct or container holding it is destroyed.

Destroying an array frees its memory. There is no garbage collector and no reference counting, so
memory is freed at a point the source shows, and a loop that allocates frees as it goes.

## Copy types

Some types copy instead of moving. Using a value of a copy type leaves the original untouched.

The copy types are: every number type, `bool`, `None`, enums without data, `Fixed` arrays,
tuples whose elements are all copy types, and structs declared with the `Copy` base whose fields
are all copy types.

```python
class Vec2(Struct, Copy):
    x: f64
    y: f64
```

Structs are not copy types by default because a copy of a large struct costs time that the source
should show. That copy is written `.copy()`, which every value that moves has: an `Array`, a
`list`, a `str`, and a struct, whose `copy` copies each field unless the struct defines its own.
`Fixed` arrays are copy types because they are small by design; the compiler warns when one has
more than 64 elements ([decision 0007](../decisions/0007-array-types.md)).

## Moves

A value of any other type (an `Array`, a `list`, a `str`, a struct without `Copy`) moves when it is:

- assigned to a variable, field or element: `b = a`;
- passed as an `Owned` argument;
- returned from a function;
- placed in a struct, tuple or list being built: `Atoms(positions, velocities, forces)`.

A conditional expression passes its move on to each branch: `b = x if ok else y` moves `x` or
`y`, so both count as moved after it, as on the two branches of an `if`.

After a move, the old owner holds nothing. Using it is an error until a new value is assigned:

```text
error: `a` was moved
  --> heat.byn:12:13
   |
12 |     total = a.sum()
   |             ^ read after the move
  ::: heat.byn:11:9
   |
11 |     b = a
   |         - `a` moved here
   = note: to keep both, move a copy: `a.copy()`
```

A variable moved on only one branch of an `if` counts as moved after it. A variable moved inside
a loop, and not assigned a new value later in the same pass, is an error at the move, because the
next pass would read it.

A read-only or `InOut` parameter belongs to the caller, so it cannot be moved, returned included:

```text
error: `u` is borrowed from the caller, so it cannot be moved
  --> heat.byn:23:12
   |
23 |     return u
   |            ^ moved here
   = note: move a copy instead: `u.copy()`, or take the parameter as `Owned[...]`
```

Python programmers expect `b = a` to make two names for one list. Here it moves, and the error
appears at the first use that would have shared data, which is where a Python bug of the same shape
would have been introduced.

Moving a value out of a field ends the struct that held it, because a struct with a hole in it
cannot be used. So a field may be moved out only of a struct owned by a local variable or an
`Owned` parameter, and that struct cannot be used again unless it is assigned a new value. Its
other fields are destroyed when the function returns. In any other case, such as
`x = atoms.positions` with `atoms` used later, write `x = atoms.positions.copy()`, or pass
`atoms.positions` straight to the function that needs it. Values cannot be moved out of array or
list elements at all; elements of arrays are copy types anyway.

### Tuple assignment

`a, b = e1, e2` evaluates every expression on the right, in order, before assigning anything on the
left. The values on the right move into temporaries first, then into the targets. So swapping two
arrays moves two pointers and copies no data:

```python
u, out = out, u
```

## Parameters

A function says, for each parameter, how it receives its argument
([decision 0006](../decisions/0006-parameter-passing.md)):

| Annotation | Mode | The caller | The function |
| --- | --- | --- | --- |
| `x: T` | read-only borrow | keeps `x`, unchanged | may read `x`, never change it |
| `x: InOut[T]` | exclusive borrow | keeps `x`, and sees the function's changes | may read and change `x`, or assign it a new value |
| `x: Owned[T]` | move | loses `x`, as in a move | owns `x`, and may keep or return it |

Call sites look the same in every mode: `saxpy(2.0, x, y)`.

A borrow lasts exactly as long as the call. A function cannot store a borrowed parameter in a
struct, return it, or put it in a list; it can copy it, or pass it on to another call. This is what
lets the language skip Rust's lifetime annotations: every borrow ends where the call ends, so the
compiler never has to reason about how long one lives.

Returning a read-only parameter is therefore an error that suggests `return x.copy()`.

A read-only parameter is never changed: assigning it, its fields or its elements, using it as the
target of `+=`, passing it as `InOut` and calling a method that changes it are all errors. The
exception is a copy type, such as an `f64`, a `Fixed` vector or a `Copy` struct. It is passed by
value, so the function holds its own copy and may change it, as in `n = abs(n)`, without the caller
seeing.

```text
error: `values` is a read-only parameter, so it cannot be changed
 --> stats.byn:3:5
  |
3 |     values.append(0.0)
  |     ^^^^^^ changed here
  = note: take it as `InOut[...]` to change the caller's value, or change a copy: `work = values.copy()`
```

## Methods

A method's first parameter is `self`. Its mode follows the same table:

```python
class Histogram(Struct):
    counts: Array[i64, 1]
    width: f64

    def total(self) -> i64:                       # reads the histogram
        return self.counts.sum()

    def add(self: InOut[Self], value: f64):       # changes the histogram
        self.counts[i64(value / self.width)] += 1

    def into_counts(self: Owned[Self]) -> Array[i64, 1]:  # consumes it
        return self.counts
```

`Self` is Python's `typing.Self`: the struct the method belongs to. A method called on a place,
`histogram.add(2.5)`, borrows that place in the method's mode, so the exclusivity check below
treats `self` like any other argument.

## The exclusivity check

The check guarantees that while a function can change a value, no other argument of the same call
refers to it. That guarantee is what lets the compiler tell LLVM that arrays never alias, so loops
vectorize with no run-time overlap tests, as Fortran's do.

### Places and overlap

A **place** is a variable followed by zero or more steps, each either `.field` or `[...]`:
`atoms`, `atoms.forces`, `atoms.forces[i]`, `grid[1:n, 0]`.

Two places **overlap** when they start at the same variable and, step by step, one is a prefix of
the other, where:

- `.a` and `.a` match, and `.a` and `.b` do not;
- any `[...]` matches any `[...]`, whatever the indices or slices.

So `atoms.positions` and `atoms.forces` do not overlap, `atoms` and `atoms.forces` do, and
`a[0:5]` and `a[5:10]` do, because the check does not compare index values. Comparing indices
would make the check depend on arithmetic it cannot always do; version 0 keeps it simple and
predictable. Disjoint parts of one array go through separate parameters by splitting with a library
function, planned with the array views work (roadmap 1.14).

### The procedure

For every call, including method calls and operators that change their left side, the compiler:

1. Lists the call's arguments that are places, each with its mode. An argument that is not a place,
   such as `x + 1`, is a temporary and cannot conflict.
2. For every `InOut` or `Owned` argument, checks that its place overlaps no other argument's place.
   If one does, it reports both arguments:

   ```text
   error: `atoms` is changed by this call and also passed to it
    --> md.byn:40:5
      |
   40 |     integrate(atoms, atoms.forces)
      |               -----  ^^^^^^^^^^^^ `atoms.forces` is part of `atoms`
      |               |
      |               `atoms` is passed as `InOut`
   ```

3. For every `Owned` argument, marks its place as moved after the call.
4. Checks the arguments' types against the parameters.

Reading the same place twice is always allowed: `dot(residual, residual)`.

Because a borrow cannot outlive its call, this per-call check is the whole check. There is no
analysis across statements, and no case where a borrow made in one line blocks a use on another.

### Compound assignment

`target op= value` changes `target`, and `value` may read it: `residual -= alpha * residual`. The
result is as if `value` were computed in full before `target` changes. On arrays, the compiler
writes the result in place, with no temporary array, when every read of `target` in `value` is at
the same index being written, which is the common case. Otherwise it computes `value` into a
temporary array first. NumPy behaves the same way.

## Loops over collections

`for v in values:` gives `v` each element in turn. Elements of arrays are copy types, so `v` is a
copy. For a `list` of a non-copy type, `v` is a read-only borrow of the element for one iteration.

The collection is borrowed for the whole loop, so the loop body cannot change it: no `append`, no
assignment to its elements. To change elements while looping, loop over the indices:
`for i in range(len(values)):`.

```text
error: `values` cannot be changed while a loop goes over it
 --> stats.byn:3:9
  |
3 |         values.append(v * 2.0)
  |         ^^^^^^ changed here
 ::: stats.byn:2:14
  |
2 |     for v in values:
  |              ------ the loop goes over `values` here
  = note: loop over the indices instead: `for i in range(len(values)):`
```

A loop variable that borrows a non-copy element cannot be changed or moved either; keep a copy,
`v.copy()`, to hold on to one. A tuple target borrows each part the same way: in
`for name, mass in species:` over a `list[tuple[str, f64]]`, `name` is a borrow and `mass` a
copy. A loop over a temporary, such as `range(n)` or a call's result,
borrows nothing.

A list comprehension loops the same way, and its value moves into the new list. So
`[w for w in words]` over a `list[str]` is the same error as `kept = w` in a loop, with the same
fix, `[w.copy() for w in words]`; `[len(w) for w in words]` copies nothing and needs no fix. A
variable from outside the comprehension, `[name for _ in range(n)]`, would move on every pass,
so it needs `name.copy()` too.

## Slices

A slice of an array, `a[1:n]` or `grid[:, 0]`, is a view: it refers to the array's own memory and
copies nothing. A view is a borrow, so in version 0 it can be used only where a borrow can:

- as an argument: `total = sum_of(a[1:n])`;
- as the target of an assignment or compound assignment: `a[1:n] = 0.0`, `a[1:n] += b[0:n - 1]`;
- inside an array expression that builds a new array: `c = a[1:n] - a[0:n - 1]`.

Assigning into a view writes the array's own elements. When the right side reads the same array
in another place, as in `a[1:n] = a[0:n - 1]`, every element is read before any is written, as
NumPy does, so the result is the shifted array rather than one value smeared along it.

A view passed as an `InOut` argument behaves as the array's own memory: the function's changes
reach those elements. A function may also give an `InOut` parameter a whole new array; a view
cannot take one of another shape, so that panics, as whole-array arithmetic on arrays of different
lengths does. Today the compiler passes such a view as a copy that it writes back after the call,
which costs one pass over the view each way; roadmap idea I.35 removes the copy.

Binding a view to a name, `edge = grid[:, 0]`, is an error that suggests `.copy()`. Python
programmers expect the name to share memory with the array (NumPy's rule), and silently copying
would break that expectation; allowing the name would make the view a stored borrow, which the
rules above exclude. Named views may come with the reference work planned for Phase 3.
