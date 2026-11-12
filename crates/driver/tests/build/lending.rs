//! Conditional expressions in places that borrow their value, built and run (roadmap 1.9.22):
//! the branch taken is lent for the statement and given back, never freed.

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

/// A conditional expression borrowed by a call, `len`, `print` or a `for` loop, whose branches
/// mix a computed value with a variable, a borrowed or `InOut` parameter, or a field: a `str`,
/// a `list`, an `Array` and a struct's fields, nested conditionals, and the same expression
/// taking each branch in turn in a loop. Each variable still holds its value afterward.
///
/// Before, the temporary holding the value was dropped after the statement even when it held a
/// variable's or the caller's value: `word` printed freed memory and its own drop freed it a
/// second time, which glibc reports as "double free detected".
#[test]
fn borrowed_conditionals_give_back_the_branch_taken() {
    let program = "\
class Bag(Struct):
    items: list[int]
    label: str


def size(text: str) -> int:
    return len(text)


def count(xs: list[int]) -> int:
    return len(xs)


def total(a: Array[f64, 1]) -> f64:
    return a.sum()


def measure(name: str, long: bool) -> int:
    return size(name[0:1] if long else name)


def grow(xs: InOut[list[int]], short: bool) -> int:
    n = count(xs if short else [9, 9, 9])
    xs.append(n)
    return n


def pick(bag: Bag, first: bool) -> int:
    return count(bag.items if first else [0])


def main():
    word = \"ab\" + \"cd\"
    other = \"xyz\" + \"w\"
    print(measure(word, True), measure(word, False), word)
    for i in range(4):
        print(size(word if i % 2 == 0 else (other if i == 1 else word + \"!\")))
    print(word, other)
    xs = [1, 2]
    print(grow(xs, True), grow(xs, False), xs)
    bag = Bag([5, 6, 7], \"b\" + \"c\")
    print(pick(bag, True), pick(bag, False), bag.items)
    print(size(bag.label if len(bag.items) > 1 else \"none\"), bag.label)
    a = Array.fill(3, 1.5)
    b = Array.fill(2, 2.0)
    print(total(a if len(a) > 2 else b), total(b if len(a) > 5 else a + a), a[0], b[0])
    print(len(word if len(word) > 1 else other), word)
    print(word if len(word) > 9 else other)
    for c in (word if len(word) > 1 else other):
        print(c)
    print(word, other)
";
    assert_eq!(
        output("borrowed-conditionals", program).unwrap(),
        "1 4 abcd\n4\n4\n4\n5\nabcd xyzw\n2 3 [1, 2, 2, 3]\n3 1 [5, 6, 7]\n2 bc\n\
         4.5 9.0 1.5 2.0\n4 abcd\nxyzw\na\nb\nc\nd\nabcd xyzw\n"
    );
}
