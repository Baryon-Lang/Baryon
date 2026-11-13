# Tests

[What builds today](status.md#tests) says which parts of this chapter the compiler builds into
programs yet.

A test is a function the compiler runs for you, to check that the code it calls still gives the
answers it should. `baryon test <directory>` finds every test in the package, runs each, and
reports which passed.

## Writing a test

A test is a top-level function decorated `@test` that takes no parameters and returns `None`. It
passes when it returns, and fails when a check inside it fails or it raises an error.

```python
def flux(a: f64, b: f64) -> f64:
    return a - b


@test
def test_flux():
    assert flux(3.0, 1.0) == 2.0


class Unstable(Exception):
    step: i64


def advance(step: i64) -> Raises[i64, Unstable]:
    if step > 3:
        raise Unstable(step)
    return step + 1


@test
def test_advance() -> Raises[None, Unstable]:
    assert advance(4) == 5  # advance raises Unstable, which fails the test
```

- **No parameters, no result.** The runner has nothing to pass in and nothing to compare a result
  against, so a test states what it expects with `assert`.
- **It may raise.** A test whose code can raise declares it, like any function, with
  `Raises[None, ...]`. An error that reaches the end of the test fails it, and the report shows
  the error as it would end a program: `error: Unstable(step=4)`.
- **Top level only.** A method decorated `@test` is an error: the runner would need a value to
  call it on.
- **Not generic.** A test with type parameters is an error, since the runner has no types to
  call it at.
- Tests can sit in any module of the package, next to the code they check. A package that only
  has tests needs no `main`.

## Running tests

```text
$ baryon test heat
running 2 tests
test heat.test_flux ... ok
test heat.test_advance ... FAILED

failures:

---- heat.test_advance ----
error: Unstable(step=4)

test result: FAILED. 1 passed; 1 failed
```

Each test is named by its module and its own name, and the tests run in file and source order,
one after another, so the report is the same on every run.

Each test runs in its own process. A failed `assert`, an index out of range or any other
[panic](errors.md) stops the program it happens in, and running tests apart means it stops only
that test. What a test prints is kept back; a failed test's output, then its panic or error
report, is shown after the list.

`baryon test` exits with status 0 when every test passed, including when there are none, and 1
when a test failed or the package does not compile.

To run only some tests, give `-k` and part of their name: `baryon test -k test_flux heat` runs
the tests whose full name contains `test_flux` ([`baryon test`](commands.md#baryon-test)). In
VS Code, the **Run test** link above each test does the same for that test.

This is the prototype of the runner (roadmap 1.11.6). Running tests in parallel and `@bench`
come later.
