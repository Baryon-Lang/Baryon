//! Changing calls on temporaries, built and run (roadmap 1.9.24): `numbers(3).pop()` changes
//! the new list and discards it, as Python does.

use super::{baryon_build, package, stderr};
use std::io;
use std::process::Command;

/// Builds the program `text` as the `main.byn` of the package `name`, runs it, and returns what
/// it printed, after checking that the build succeeded and the program exited cleanly with
/// nothing on standard error.
///
/// # Errors
///
/// When writing the package or starting `baryon` or the program fails.
fn output(name: &str, text: &str) -> io::Result<String> {
    let directory = package(name, &[("main.byn", text)])?;
    let (built, program) = baryon_build(&directory)?;
    assert!(built.status.success(), "{}", stderr(&built));
    let ran = Command::new(&program).output()?;
    assert_eq!(stderr(&ran), "");
    assert!(ran.status.success(), "{:?}", ran.status);
    Ok(String::from_utf8_lossy(&ran.stdout).into_owned())
}

/// A call that changes its receiver or an `InOut` argument works on a temporary: `pop` of a
/// list or a dict a function returned, an `InOut[Self]` method of a struct just built, a user
/// function's `InOut` parameter given a call's result, `append` and `d[k] = v` on a temporary,
/// and `append` on a list slice, which is a new list in Python too, so `xs` keeps its values.
/// Before, lowering skipped each call without an error: `numbers(3).pop()` printed whatever
/// its destination held, such as `1180512`, in place of `20`.
#[test]
fn changing_calls_work_on_temporaries() {
    let program = "\
class Counter(Struct):
    counts: list[int]

    def bump(self: InOut[Self]) -> int:
        self.counts.append(1)
        return len(self.counts)


def numbers(n: int) -> list[int]:
    result: list[int] = []
    for i in range(n):
        result.append(i * 10)
    return result


def table(n: int) -> dict[int, str]:
    d: dict[int, str] = {}
    for i in range(n):
        d[i] = f\"v{i}\"
    return d


def grow(xs: InOut[list[int]]) -> int:
    xs.append(7)
    return len(xs)


def main():
    print(numbers(3).pop())
    print(table(3).pop(1))
    print(Counter([5]).bump())
    print(grow(numbers(3)))
    numbers(2).append(4)
    table(2)[5] = \"x\"
    a = Array.zeros(3)
    print(a[0:2].sum())
    xs = [1, 2, 3]
    xs[0:2].append(9)
    print(xs)
";
    assert_eq!(
        output("changing-temporaries", program).unwrap(),
        "20\nv1\n2\n4\n0.0\n[1, 2, 3]\n"
    );
}

/// Changing a conditional expression would change the variable its branch names, which
/// lowering does not build yet. It reports so rather than change a moved-out copy and free
/// the variable's value.
#[test]
fn changing_a_conditional_is_reported() {
    let program = "\
def main():
    a = [1, 2]
    b = [3]
    print((a if len(a) > 5 else b).pop())
";
    let directory = package("changing-conditional", &[("main.byn", program)]).unwrap();
    let (built, _) = baryon_build(&directory).unwrap();
    assert!(!built.status.success());
    assert_eq!(
        stderr(&built),
        "error: lowering does not handle changing the value of a conditional expression yet\n --> main.byn:4:12\n  |\n4 |     print((a if len(a) > 5 else b).pop())\n  |            ^^^^^^^^^^^^^^^^^^^^^^ not lowered yet\n  = note: the compiler lowers more of the language as roadmap item 1.9.3 continues\n\n"
    );
}
