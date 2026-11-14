//! `__del__` built and run (decision 0063): it runs once for each value, when the value is
//! dropped, before the value's fields are dropped, and an error that escapes it prints and
//! panics. The ownership sweep's `destructor` kinds run the same drops under Valgrind through
//! every site.

use super::examples::prints;
use super::{baryon_build, package, panic_report, stderr};
use std::process::Command;

/// Every way a value is dropped: at the end of its function, on an early `return`, when an
/// error leaves the function, as a list's element, as another struct's field, when its
/// variable is given a new value, as a temporary nothing keeps, and after a borrow, which does
/// not drop it.
const DROPS: &str = "\
class Log(Struct):
    name: str
    lines: list[str]

    def __del__(self: InOut[Self]):
        print(f\"del {self.name} {len(self.lines)}\")


class Plain(Struct):
    tag: int

    def __del__(self: InOut[Self]):
        print(f\"del plain {self.tag}\")


class Outer(Struct):
    inner: Log
    other: Plain

    def __del__(self: InOut[Self]):
        print(\"del outer\")


def make(name: str) -> Log:
    return Log(name.copy(), [\"a\"])


def early(flag: bool) -> int:
    log = make(\"early\")
    if flag:
        return 1
    return 2


def raising() -> Raises[None, ValueError]:
    log = make(\"raise\")
    raise ValueError(\"boom\")


def take(log: Log):
    print(f\"took {log.name}\")


def main():
    a = make(\"scope\")
    early(True)
    try:
        raising()
    except ValueError:
        print(\"caught\")
    items = [make(\"one\"), make(\"two\")]
    print(len(items))
    b = make(\"moved\")
    take(b)
    p = Plain(7)
    o = Outer(make(\"inner\"), Plain(8))
    a = make(\"replacement\")
    make(\"temporary\")
    print(\"end of main\")
";

/// A value moved into a function and back runs `__del__` once, where its last owner drops it,
/// and so does one held in an optional or a tuple.
const MOVES: &str = "\
class Log(Struct):
    name: str

    def __del__(self: InOut[Self]):
        print(f\"del {self.name}\")


class Bad(Struct):
    name: str

    def __del__(self: InOut[Self]) -> Raises[None, ValueError]:
        if self.name == \"bad\":
            raise ValueError(\"cannot finish\")
        print(f\"del {self.name}\")


def keep(log: Owned[Log]) -> Log:
    print(\"kept\")
    return log


def main():
    a = Log(\"moved\")
    b = keep(a)
    maybe: Log | None = Log(\"optional\")
    t = (Log(\"tuple\"), 3)
    g = Bad(\"good\")
    print(\"end\")
";

/// A `__del__` that raises prints the error as an uncaught error is printed, then panics.
const RAISES: &str = "\
class Bad(Struct):
    name: str

    def __del__(self: InOut[Self]) -> Raises[None, ValueError]:
        if self.name == \"bad\":
            raise ValueError(\"cannot finish\")
        print(f\"del {self.name}\")


def main():
    g = Bad(\"good\")
    h = Bad(\"bad\")
    print(\"end\")
";

/// `__del__` runs at every drop, once, outer value first, variables last declared first.
#[test]
fn del_runs_once_at_every_drop() {
    prints(
        "destructors-drops",
        DROPS,
        "\
del early 1
del raise 1
caught
2
took moved
del scope 1
del temporary 1
end of main
del outer
del inner 1
del plain 8
del plain 7
del moved 1
del one 1
del two 1
del replacement 1
",
    )
    .unwrap();
}

/// A moved value runs `__del__` once, at its last owner.
#[test]
fn a_moved_value_runs_del_once() {
    prints(
        "destructors-moves",
        MOVES,
        "\
kept
end
del good
del tuple
del optional
del moved
",
    )
    .unwrap();
}

/// An error from `__del__` is printed, then the program panics naming the class; values
/// dropped after it never run theirs.
#[test]
fn an_error_from_del_panics() {
    let directory = package("destructors-raises", &[("main.byn", RAISES)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "end\n");
    assert_eq!(ran.status.code(), Some(101));
    assert_eq!(
        panic_report(&ran),
        "error: ValueError(message='cannot finish')\n\
         panic: `Bad.__del__` raised the error above when its value was dropped\n\
         \x20 --> main.byn:10:1\n"
    );
}
