//! Lowering of loops over a text file (decision 0036, part 4): `for line in f` reads a line at
//! the top of each pass and leaves at the end of the file.

use crate::lower;

/// Each pass calls `File.readline` with the error edge of the loop, compares the line with the
/// empty string, which only the end of the file reads as, and moves the line into the loop
/// variable, dropping the line before it.
#[test]
fn a_loop_over_a_file_reads_a_line_each_pass() {
    let program = "\
        def count(source: InOut[File]) -> Raises[int, OSError]:\n\
        \x20   lines = 0\n\
        \x20   for line in source:\n\
        \x20       lines += 1\n\
        \x20   return lines\n";
    assert_eq!(
        lower(program, "count"),
        "\
fn(_1: InOut[File]) -> i64 {
    let _0: i64; // return
    let _1: File; // parameter source
    let _2: i64; // lines
    let _3: str; // line
    let _4: str;
    let _5: Raises[None, OSError];
    let _6: i64;
    let _7: bool;
    let _8: tuple[i64, bool];
    let _9: bool;

    bb0: {
        _2 = 0_i64;
        goto -> bb1;
    }

    bb1: {
        _4 = call File.readline(_1) -> [return: bb3, error _5: bb2];
    }

    bb2: {
        drop(_3);
        raise move _5.error[0];
    }

    bb3: {
        _6 = call str comparison(_4, owned \"\") -> [return: bb4];
    }

    bb4: {
        _7 = NotEqual(copy _6, 0_i64);
        branch copy _7 -> [true: bb5, false: bb6];
    }

    bb5: {
        drop(_3);
        _3 = move _4;
        _8 = CheckedAdd(copy _2, 1_i64);
        _9 = Not(copy _8.1);
        assert(copy _9, no overflow: Add) -> bb8;
    }

    bb6: {
        drop(_4);
        goto -> bb7;
    }

    bb7: {
        _0 = copy _2;
        drop(_3);
        return;
    }

    bb8: {
        _2 = copy _8.0;
        goto -> bb1;
    }
}
"
    );
}
