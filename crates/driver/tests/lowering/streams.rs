//! Lowering of the standard streams (decision 0036, part 6): `print(..., file=, flush=)`,
//! `input` and a read of `sys.stdout`.

use crate::lower;

/// A print to standard error is one call with no error edge and no flush, since standard error
/// holds nothing back; a print to a file is a call of the file with the error edge of the
/// statement, and `flush=` branches to a flush with the same edge; a read of `sys.stdout` makes
/// a new handle; and `input` takes its prompt and has an error edge too.
#[test]
fn prints_pick_their_stream_and_input_can_fail() {
    let program = "\
        import sys\n\
        \n\
        def tell(log: InOut[File], loud: bool) -> Raises[str, OSError | EOFError]:\n\
        \x20   print(\"to\", \"stderr\", file=sys.stderr, flush=loud)\n\
        \x20   print(\"to the log\", file=log, flush=loud)\n\
        \x20   out = sys.stdout\n\
        \x20   out.write(\"direct\")\n\
        \x20   return input(\"name: \")\n";
    assert_eq!(lower(program, "tell"), "\
fn(_1: InOut[File], _2: bool) -> str {
    let _0: str; // return
    let _1: File; // parameter log
    let _2: bool; // parameter loud
    let _3: File; // out
    let _4: None;
    let _5: None;
    let _6: Raises[None, OSError];
    let _7: None;
    let _8: Raises[None, OSError];
    let _9: File;
    let _10: i64;
    let _11: Raises[None, OSError];
    let _12: Raises[None, EOFError];

    bb0: {
        _4 = call print to sys.stderr(owned \"to\", owned \" \", owned \"stderr\", owned \"\\n\") -> [return: bb1];
    }

    bb1: {
        _5 = call print to a File(_1, owned \"to the log\", owned \"\\n\") -> [return: bb3, error _6: bb2];
    }

    bb2: {
        drop(_3);
        raise move _6.error[0];
    }

    bb3: {
        branch copy _2 -> [true: bb4, false: bb5];
    }

    bb4: {
        _7 = call File.flush(_1) -> [return: bb7, error _8: bb6];
    }

    bb5: {
        _9 = call standard stream(owned 1_i64) -> [return: bb8];
    }

    bb6: {
        drop(_3);
        raise move _8.error[0];
    }

    bb7: {
        goto -> bb5;
    }

    bb8: {
        drop(_3);
        _3 = move _9;
        _10 = call File.write(_3, owned \"direct\") -> [return: bb10, error _11: bb9];
    }

    bb9: {
        drop(_3);
        raise move _11.error[0];
    }

    bb10: {
        _0 = call input(owned \"name: \") -> [return: bb12, error _12: bb11];
    }

    bb11: {
        drop(_3);
        raise move _12.error[0];
    }

    bb12: {
        drop(_3);
        return;
    }
}
");
}
