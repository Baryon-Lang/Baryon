//! Lowering of `sys`, `os` and array files (decision 0036): `sys.argv` builds a new list,
//! `sys.exit` is a call that never returns, `os.getenv` branches on whether the variable is set,
//! and `Array.fromfile` and `tofile` are calls with an error edge.

use crate::lower;

/// `sys.argv` appends each argument to a new list, in a loop over the argument count.
#[test]
fn argv_builds_a_new_list_of_the_arguments() {
    let program = "\
        import sys\n\
        \n\
        \n\
        def arguments() -> list[str]:\n\
        \x20   return sys.argv\n";
    assert_eq!(
        lower(program, "arguments"),
        "\
fn() -> list[str] {
    let _0: list[str]; // return
    let _1: i64;
    let _2: i64;
    let _3: bool;
    let _4: str;
    let _5: None;

    bb0: {
        _1 = call len of sys.argv() -> [return: bb1];
    }

    bb1: {
        _0 = list[str]();
        _2 = 0_i64;
        goto -> bb2;
    }

    bb2: {
        _3 = Less(copy _2, copy _1);
        branch copy _3 -> [true: bb3, false: bb4];
    }

    bb3: {
        _4 = call sys.argv item(owned copy _2) -> [return: bb5];
    }

    bb4: {
        return;
    }

    bb5: {
        _5 = call list.append(inout _0, owned move _4) -> [return: bb6];
    }

    bb6: {
        _2 = Add(copy _2, 1_i64);
        goto -> bb2;
    }
}
"
    );
}

/// `sys.exit()` passes 0, and `os.getenv(name, default)` moves the value out when it is set
/// and the default in when it is not.
#[test]
fn exit_and_getenv_are_runtime_calls() {
    let program = "\
        import os\n\
        import sys\n\
        \n\
        \n\
        def threads() -> str:\n\
        \x20   value = os.getenv(\"OMP_NUM_THREADS\", \"1\")\n\
        \x20   if value == \"0\":\n\
        \x20       sys.exit()\n\
        \x20   return value\n";
    assert_eq!(
        lower(program, "threads"),
        "\
fn() -> str {
    let _0: str; // return
    let _1: str; // value
    let _2: str;
    let _3: bool;
    let _4: str;
    let _5: bool;
    let _6: str;
    let _7: i64;
    let _8: None;

    bb0: {
        _4 = call os.environ.get(owned \"OMP_NUM_THREADS\", inout _3) -> [return: bb1];
    }

    bb1: {
        branch copy _3 -> [true: bb2, false: bb3];
    }

    bb2: {
        _2 = move _4;
        goto -> bb4;
    }

    bb3: {
        _2 = \"1\";
        goto -> bb4;
    }

    bb4: {
        drop(_1);
        _1 = move _2;
        drop(_4);
        _6 = \"0\";
        _7 = call str comparison(_1, _6) -> [return: bb5];
    }

    bb5: {
        _5 = Equal(copy _7, 0_i64);
        drop(_6);
        branch copy _5 -> [true: bb6, false: bb7];
    }

    bb6: {
        _8 = call sys.exit(owned 0_i64) -> [return: bb8];
    }

    bb7: {
        _0 = move _1;
        drop(_1);
        return;
    }

    bb8: {
        goto -> bb7;
    }
}
"
    );
}

/// `Array.fromfile` passes the path, the count and the offset, -1 and 0 where left out, and
/// `tofile` borrows the array; a failure of either raises its `OSError` to the caller.
#[test]
fn array_files_are_calls_that_can_fail() {
    let program = "\
        def copy(source: str, target: str) -> Raises[None, OSError]:\n\
        \x20   field = Array.fromfile(source, f32, offset=64)\n\
        \x20   field.tofile(target)\n";
    assert_eq!(
        lower(program, "copy"),
        "\
fn(_1: str, _2: str) -> None {
    let _0: None; // return
    let _1: str; // parameter source
    let _2: str; // parameter target
    let _3: Array[f32, 1]; // field
    let _4: Array[f32, 1];
    let _5: Raises[None, OSError];
    let _6: None;
    let _7: Raises[None, OSError];

    bb0: {
        _4 = call Array.fromfile(_1, owned -1_i64, owned 64_i64) -> [return: bb2, error _5: bb1];
    }

    bb1: {
        drop(_3);
        raise move _5.error[0];
    }

    bb2: {
        drop(_3);
        _3 = move _4;
        _6 = call Array.tofile(_3, _2) -> [return: bb4, error _7: bb3];
    }

    bb3: {
        drop(_3);
        raise move _7.error[0];
    }

    bb4: {
        _0 = None;
        drop(_3);
        return;
    }
}
"
    );
}
