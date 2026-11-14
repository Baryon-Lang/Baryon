//! Checking `for` over the program's own classes (decision 0053), end to end: programs that
//! misuse an iterator or an iterable go through the parser, the syntax check, the resolver and
//! the checker, and the rendered diagnostics are compared in full.

use driver::Session;

/// Checks a package of one file, `main.byn`, and returns every diagnostic, rendered and joined
/// by blank lines. The program's first newline, which only lets a test start it on its own
/// line, is dropped.
fn check_one(text: &str) -> String {
    let mut session = Session::new();
    if let Err(error) = session.add_file("main.byn", text.strip_prefix('\n').unwrap_or(text)) {
        return error.to_string();
    }
    session
        .check()
        .iter()
        .map(|diagnostic| diagnostics::render(diagnostic, &session).to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// An iterator the loop body changes or moves, a `__next__` that cannot end the loop, an
/// `__iter__` that returns no iterator, a comprehension over an iterator, and an error from
/// `__next__` that nothing handles are each explained.
#[test]
fn misused_iterators_are_explained() {
    let program = r"
class Countdown(Struct):
    left: int

    def __next__(self: InOut[Self]) -> Raises[int, StopIteration]:
        if self.left == 0:
            raise StopIteration()
        self.left -= 1
        return self.left


class Endless(Struct):
    left: int

    def __next__(self: InOut[Self]) -> int:
        return self.left


class Box(Struct):
    left: int

    def __iter__(self) -> int:
        return self.left


class Broken(Exception):
    at: int


class Fragile(Struct):
    left: int

    def __next__(self: InOut[Self]) -> Raises[int, StopIteration | Broken]:
        raise Broken(1)


def main():
    c = Countdown(3)
    for n in c:
        c.left = 0
        moved = c
    for n in Endless(1):
        pass
    for n in Box(1):
        pass
    squares = [n * n for n in Countdown(3)]
    for n in Fragile(2):
        pass
";
    assert_eq!(
        check_one(program),
        r"error: `c` cannot be changed while a loop goes over it
  --> main.byn:39:9
   |
39 |         c.left = 0
   |         ^^^^^^ changed here
  ::: main.byn:38:14
   |
38 |     for n in c:
   |              - the loop goes over `c` here
   = note: the loop calls `__next__` on `c` on each pass; change it before or after the loop (decision 0053)

error: `c` cannot be moved while a loop goes over it
  --> main.byn:40:17
   |
40 |         moved = c
   |                 ^ moved here
  ::: main.byn:38:14
   |
38 |     for n in c:
   |              - the loop goes over `c` here
   = note: the loop calls `__next__` on `c` on each pass; change it before or after the loop (decision 0053)

error: `Endless.__next__` must take `self: InOut[Self]` and return `Raises[T, StopIteration]` for `for`
  --> main.byn:41:14
   |
41 |     for n in Endless(1):
   |              ^^^^^^^^^^ looped over here
   = note: `__next__` changes the iterator and raises `StopIteration` when it has no value left, which ends the loop (decision 0053)

error: `__iter__` returns `i64`, which has no `__next__`
  --> main.byn:43:14
   |
43 |     for n in Box(1):
   |              ^^^^^^ not an iterator
   = note: `__iter__` returns an iterator: a class with `def __next__(self: InOut[Self]) -> Raises[T, StopIteration]` (decision 0053)

error: a comprehension over a class with `__next__` or `__iter__` is not built yet
  --> main.byn:45:31
   |
45 |     squares = [n * n for n in Countdown(3)]
   |                               ^^^^^^^^^^^^ not built yet
   = note: write a `for` statement that appends to a list (decision 0053)

error: `__next__` can fail with `Broken`, which is not handled here
  --> main.byn:46:5
   |
46 |     for n in Fragile(2):
   |     ^^^^^^^^^^^^^^^^^^^^ can fail with `Broken`
   = note: catch it with `try:` and `except Broken:`, or let it pass on by declaring `-> Raises[None, Broken]`"
    );
}
