//! `for` over the program's own classes (decision 0053) built and run: an iterator's
//! `__next__` is called once a pass until it raises `StopIteration`, on a temporary or on a
//! variable that keeps its state after the loop; an iterable's `__iter__` is called once; and
//! another error from `__next__` leaves the loop like any call's error. The ownership sweep's
//! `managers` sites run the same paths under Valgrind for every kind of value.

use super::examples::prints;

/// An iterator over a temporary and over a variable, a `break` and a second loop that goes on
/// where it stopped, a `return` from inside the loop, tuple targets and an error other than
/// `StopIteration`, an iterable whose `__iter__` copies its list, and the last value kept
/// after the loop.
const LOOPS: &str = "\
class Broken(Exception):
    at: int


class Countdown(Struct):
    left: int

    def __iter__(self: Owned[Self]) -> Self:
        return self

    def __next__(self: InOut[Self]) -> Raises[int, StopIteration]:
        if self.left == 0:
            raise StopIteration()
        self.left -= 1
        return self.left + 1


class Words(Struct):
    count: int
    fail_at: int

    def __next__(self: InOut[Self]) -> Raises[tuple[int, str], StopIteration | Broken]:
        if self.count == self.fail_at:
            raise Broken(self.count)
        if self.count == 0:
            raise StopIteration()
        self.count -= 1
        return (self.count, f\"w{self.count}\")


class Shelf(Struct):
    names: list[str]

    def __iter__(self) -> ShelfIterator:
        return ShelfIterator(self.names.copy(), 0)


class ShelfIterator(Struct):
    names: list[str]
    position: int

    def __next__(self: InOut[Self]) -> Raises[str, StopIteration]:
        if self.position == len(self.names):
            raise StopIteration()
        self.position += 1
        return self.names[self.position - 1].copy()


def words(n: int, fail_at: int) -> Raises[int, Broken]:
    total = 0
    for i, word in Words(n, fail_at):
        print(i, word)
        total += i
    return total


def first_big(limit: int) -> int:
    for n in Countdown(10):
        if n < limit:
            return n
    return -1


def main():
    for n in Countdown(3):
        print(n)
    c = Countdown(5)
    for n in c:
        if n == 3:
            break
    print(c.left)
    for n in c:
        print(\"again\", n)
    print(first_big(4))
    try:
        print(words(3, -1))
        print(words(4, 1))
    except Broken as error:
        print(f\"broken at {error.at}\")
    shelf = Shelf([\"a\", \"b\"])
    for name in shelf:
        print(name)
    for name in shelf:
        if name == \"b\":
            continue
        print(\"skip test\", name)
    last = \"\"
    for name in Shelf([\"x\", \"y\"]):
        last = name
    print(last, len(shelf.names))
";

/// Each loop goes over the values Python's would, in the same order.
#[test]
fn user_iterators_build_and_run() {
    prints(
        "user-iterators",
        LOOPS,
        "\
3
2
1
2
again 2
again 1
3
2 w2
1 w1
0 w0
3
3 w3
2 w2
1 w1
broken at 1
a
b
skip test a
y 2
",
    )
    .unwrap();
}
