//! Context managers (decision 0053) built and run: `__exit__` runs at the end of a `with` block
//! and on every other way out of it, `return`, `break`, `continue` and an error, innermost
//! first, and an error from `__exit__` replaces the one in flight. The ownership sweep's
//! `managers` sites run the same paths under Valgrind for every kind of value.

use super::examples::prints;

/// Every way out of a block: its end, with a name and without, an early `return`, `break` and
/// `continue` in a loop, two managers in one `with`, a `raise` caught outside, and an
/// `__exit__` that raises.
const EXITS: &str = "\
class Failed(Exception):
    code: int


class Section(Struct):
    name: str
    count: int

    def __enter__(self: Owned[Self]) -> Self:
        print(f\"begin {self.name}\")
        return self

    def __exit__(self):
        print(f\"end {self.name} after {self.count}\")


class Closer(Struct):
    name: str

    def __enter__(self: Owned[Self]) -> Self:
        print(f\"open {self.name}\")
        return self

    def __exit__(self: Owned[Self]) -> Raises[None, Failed]:
        print(f\"close {self.name}\")
        if self.name == \"bad\":
            raise Failed(7)


def early(n: int) -> str:
    with Section(\"early\", 0) as s:
        if n > 1:
            return f\"big {s.name}\"
    return \"small\"


def failing(n: int) -> Raises[int, Failed]:
    with Closer(\"outer\") as outer, Section(\"inner\", n) as inner:
        if n == 3:
            raise Failed(3)
        inner.count += 1
    return n


def looping():
    for i in range(3):
        with Section(\"loop\", i) as s:
            if i == 1:
                continue
            if i == 2:
                break
            print(\"body\")


def main():
    with Section(\"plain\", 1) as log:
        log.count += 1
        print(\"inside\")
    print(log.name)
    with Section(\"hidden\", 5):
        print(\"no name\")
    print(early(2))
    print(early(0))
    looping()
    try:
        print(failing(1))
        print(failing(3))
    except Failed as error:
        print(f\"caught {error.code}\")
    try:
        with Closer(\"bad\"):
            print(\"in bad\")
    except Failed as error:
        print(f\"caught {error.code}\")
";

/// Errors and `__exit__`: an error from `__exit__` replaces the one in flight and the value of
/// a `return`, a `try` inside the block catches its own errors before `__exit__` runs, an
/// error from `__exit__` in a loop leaves through the outer block, and a failed `__enter__`
/// runs no block and no `__exit__`.
const ERRORS: &str = "\
class Failed(Exception):
    code: int


class Other(Exception):
    message: str


class Closer(Struct):
    name: str

    def __enter__(self: Owned[Self]) -> Raises[Self, Other]:
        if self.name == \"refuse\":
            raise Other(\"refused\")
        print(f\"open {self.name}\")
        return self

    def __exit__(self: Owned[Self]) -> Raises[None, Failed]:
        print(f\"close {self.name}\")
        if self.name.startswith(\"bad\"):
            raise Failed(len(self.name))


def replaced() -> Raises[None, Failed | Other]:
    with Closer(\"bad-one\") as c:
        raise Other(f\"from {c.name}\")


def returned() -> Raises[str, Failed | Other]:
    with Closer(\"badly\") as c:
        return c.name.copy()


def nested_try() -> Raises[None, Failed | Other]:
    with Closer(\"outer\"):
        try:
            with Closer(\"inner\"):
                raise Other(\"x\")
        except Other as caught:
            print(f\"handled {caught.message}\")
        for i in range(3):
            with Closer(\"bad\"):
                if i == 0:
                    continue
                break


def main():
    try:
        replaced()
    except Failed as error:
        print(f\"replaced by {error.code}\")
    except Other as other:
        print(f\"not replaced {other.message}\")
    try:
        print(returned())
    except Failed as error:
        print(f\"return replaced {error.code}\")
    except Other as other:
        print(other.message)
    try:
        nested_try()
    except Failed as error:
        print(f\"nested {error.code}\")
    except Other as other:
        print(other.message)
    try:
        with Closer(\"refuse\") as r:
            print(\"never\")
    except Other as other:
        print(f\"enter failed: {other.message}\")
    except Failed as error:
        print(\"no\")
";

/// `__exit__` runs once on every way out of a block, innermost block first, as in Python.
#[test]
fn exit_runs_on_every_way_out() {
    prints(
        "managers-exits",
        EXITS,
        "\
begin plain
inside
end plain after 2
plain
begin hidden
no name
end hidden after 5
begin early
end early after 0
big early
begin early
end early after 0
small
begin loop
body
end loop after 0
begin loop
end loop after 1
begin loop
end loop after 2
open outer
begin inner
end inner after 2
close outer
1
open outer
begin inner
end inner after 3
close outer
caught 3
open bad
in bad
close bad
caught 7
",
    )
    .unwrap();
}

/// Errors from `__enter__` and `__exit__` go where Python sends them.
#[test]
fn errors_pass_through_exit() {
    prints(
        "managers-errors",
        ERRORS,
        "\
open bad-one
close bad-one
replaced by 7
open badly
close badly
return replaced 5
open outer
open inner
close inner
handled x
open bad
close bad
close outer
nested 3
enter failed: refused
",
    )
    .unwrap();
}
