//! Narrowing by `is not None` in built programs, where a read of the narrowed name must reach
//! the value inside the optional, whichever place the name stands for.

use super::{baryon_build, package, stderr};
use std::process::Command;

/// A loop variable that borrows an element of a `list[str | None]` is narrowed by
/// `is not None` as a plain variable is (roadmap 1.9.35): `item.copy()` and `item[:]` read the
/// `str` inside. Before, lowering read the element itself, which the backend could not copy
/// and gave `clang` IR whose types disagreed.
#[test]
fn borrowed_loop_variables_are_narrowed() {
    let program = "\
def present(items: list[str | None]) -> list[str]:
    out: list[str] = []
    for item in items:
        if item is not None:
            out.append(item.copy())
            out.append(item[:1])
    return out


def main():
    items: list[str | None] = [f\"a{1}\", None, f\"b{2}\"]
    for item in items:
        if item is not None:
            kept = item.copy()
            part = item[:]
            print(kept, part, len(item))
        else:
            print(\"none\")
    print(present(items))
";
    let directory = package("narrowed_loop_variables", &[("main.byn", program)]).unwrap();
    let (output, program) = baryon_build(&directory).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let ran = Command::new(&program).output().unwrap();
    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "a1 a1 2\nnone\nb2 b2 2\n['a1', 'a', 'b2', 'b']\n"
    );
}
