# 0027. When a default parameter value is evaluated

Status: Proposed
Date: 2026-11-08
Roadmap: D.15

## Context

A parameter may have a default value, as in Python (the reference,
[Default parameter values](../reference/types.md#default-parameter-values)). The checker accepts
any default of the parameter's type, and since pull request #77 a call that leaves the argument out
builds when the default is a literal or a `Final` constant. Those have one value however often they
are evaluated. Any other default, such as `Array.zeros(n_bins)` or a call to a function, can give
a different value, or a different array, each time, so the language must say when it is
evaluated. Until it does, `baryon build` reports a call that leaves such an argument out as not
built yet.

Python evaluates a default once, when the `def` statement runs, and every call that leaves the
argument out shares that one object:

```python
def record(value: f64, history: list[f64] = []) -> list[f64]:
    history.append(value)
    return history
```

In Python the second call to `record(2.0)` returns `[1.0, 2.0]`: the first call's item is still in
the shared default. This is one of the best-known surprises in Python, and linters warn about it.

Three facts about this language bear on the choice:

1. **There is no module-level code and there are no module-level variables**
   ([Syntax](../reference/syntax.md#source-files)). A value evaluated once would be a hidden
   global, created before `main` or on first use, which the language otherwise has none of.
2. **Values have one owner** ([Values and references](../reference/values-and-references.md)).
   A shared default array would need an owner outside every call. An `Owned` parameter takes
   ownership of its argument and may destroy it, so a shared default could not be passed to one
   without a copy, and an `InOut` parameter would write into it, which is exactly Python's
   surprise (roadmap idea I.20 asks whether an `InOut` default should be allowed at all).
3. **Parallel loops may call any function**
   ([Parallel and vector loops](../reference/parallel-loops.md)). A default evaluated once and shared would be shared between threads.

The options:

- **A. At each call that leaves the argument out.** The default is evaluated afresh, in the
  callee's module, every time it is needed, as in C++, Kotlin, Swift, Julia and Ruby. Every call
  gets its own value, owned by that call. A default that allocates allocates on every such call.
- **B. Once, as Python does.** The value is computed once and shared. It matches Python's meaning,
  so code copied from Python behaves the same, including the surprise. It needs a hidden global
  with an owner, thread-safe initialization, and a rule for `Owned` and `InOut` parameters.
- **C. Only compile-time values.** A default must be a literal, a `Final` constant or arithmetic
  on them, which is all `baryon build` handles today. Nothing else is accepted, so the question
  never arises. A function that wants a fresh array by default takes `T | None` and creates the
  array when it gets `None`.

## Decision

Proposed: **A**, evaluate a default at each call that leaves the argument out.

It is the only option with no shared state, so it fits the ownership model, parallel loops and the
absence of globals without new rules. It also removes Python's surprise rather than copying it: a
program whose meaning depends on a default being shared is almost always a bug. A program that
relies on the sharing on purpose, such as a cache, is rare in numeric code and is clearer with the
state passed explicitly.

The default is evaluated after the arguments that were passed, in the order the parameters are
declared, and it sees only names visible at module level in the function's own module, never the
caller's names or the function's other parameters (the checker already enforces the second).

## Consequences

- Code copied from Python that relies on a shared mutable default behaves differently. The
  [Differences from Python](../reference/differences-from-python.md) chapter must list it.
- A default that allocates, such as `Array.zeros(n)`, costs an allocation on every call that
  leaves it out. The cost is visible in the signature.
- Lowering can evaluate a non-constant default in a small function generated next to the callee,
  called by each call site that leaves the argument out, so the callee's module still owns the
  expression and its names.
- Rejected B gives up exact Python compatibility for defaults. Rejected C would be simpler still,
  but forbids a fresh-array default that physics code commonly wants, such as a work buffer.
- Collins decides. Until then the reference leaves the question open and `baryon build` keeps
  reporting non-constant defaults as not built.
