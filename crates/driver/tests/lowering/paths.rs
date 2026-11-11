//! Lowering of `os.environ`, `pathlib.Path` and the directory functions of `os` (decision 0036,
//! parts 1, 5 and 7).

use crate::lower;

/// `name in os.environ` asks the runtime for the variable and keeps only whether it is set;
/// `os.environ[name]` asks again and moves the value out, with the check that panics, as a
/// missing dict key does, when it is not set.
#[test]
fn environ_is_tested_and_indexed() {
    let program = "\
        import os\n\
        \n\
        \n\
        def editor(name: str) -> str:\n\
        \x20   if name in os.environ:\n\
        \x20       return os.environ[name]\n\
        \x20   return \"vi\"\n";
    assert_eq!(
        lower(program, "editor"),
        "\
fn(_1: str) -> str {
    let _0: str; // return
    let _1: str; // parameter name
    let _2: bool;
    let _3: bool;
    let _4: str;
    let _5: bool;
    let _6: str;
    let _7: str;

    bb0: {
        _4 = call os.environ.get(_1, inout _3) -> [return: bb1];
    }

    bb1: {
        _2 = copy _3;
        drop(_4);
        branch copy _2 -> [true: bb2, false: bb3];
    }

    bb2: {
        _6 = call os.environ.get(_1, inout _5) -> [return: bb4];
    }

    bb3: {
        _0 = \"vi\";
        return;
    }

    bb4: {
        branch copy _5 -> [true: bb5, false: bb6];
    }

    bb5: {
        _0 = move _6;
        drop(_6);
        return;
    }

    bb6: {
        _7 = format(\"'\", {_1:}, \"'\");
        assert(copy _5, key present: move _7) -> bb5;
    }
}
"
    );
}

/// `os.environ.copy()` gets every name and value in one list, then pops a value and a name for
/// each variable into a new `dict`.
#[test]
fn environ_copy_fills_a_dict() {
    let program = "\
        import os\n\
        \n\
        \n\
        def environment() -> dict[str, str]:\n\
        \x20   return os.environ.copy()\n";
    assert_eq!(
        lower(program, "environment"),
        "\
fn() -> dict[str, str] {
    let _0: dict[str, str]; // return
    let _1: list[str];
    let _2: i64;
    let _3: i64;
    let _4: i64;
    let _5: bool;
    let _6: str;
    let _7: str;
    let _8: i64;

    bb0: {
        _1 = call os.environ.copy() -> [return: bb1];
    }

    bb1: {
        _0 = dict[str, str]();
        _2 = len(_1, 0);
        _3 = FloorDivide(copy _2, 2_i64);
        _4 = 0_i64;
        goto -> bb2;
    }

    bb2: {
        _5 = Less(copy _4, copy _3);
        branch copy _5 -> [true: bb3, false: bb4];
    }

    bb3: {
        _6 = call list.pop(inout _1) -> [return: bb5];
    }

    bb4: {
        drop(_1);
        return;
    }

    bb5: {
        _7 = call list.pop(inout _1) -> [return: bb6];
    }

    bb6: {
        _8 = call dict insert(inout _0, owned move _7) -> [return: bb7];
    }

    bb7: {
        _0[_8].1 = move _6;
        _4 = Add(copy _4, 1_i64);
        goto -> bb2;
    }
}
"
    );
}

/// A `Path` is a struct around its text: `/` and `parent` call the runtime on the texts and wrap
/// the result, `suffix` is a `str`, and `==` compares the two texts.
#[test]
fn paths_are_their_text() {
    let program = "\
        from pathlib import Path\n\
        \n\
        \n\
        def output(root: Path, name: str) -> Path:\n\
        \x20   out = root / name\n\
        \x20   if out.suffix == \".csv\" and out == root:\n\
        \x20       return out.parent\n\
        \x20   return Path(\"runs\")\n";
    assert_eq!(
        lower(program, "output"),
        "\
fn(_1: Path, _2: str) -> Path {
    let _0: Path; // return
    let _1: Path; // parameter root
    let _2: str; // parameter name
    let _3: Path; // out
    let _4: Path;
    let _5: str;
    let _6: bool;
    let _7: str;
    let _8: str;
    let _9: i64;
    let _10: i64;
    let _11: str;
    let _12: str;

    bb0: {
        _5 = call Path /(_1.0, _2) -> [return: bb1];
    }

    bb1: {
        _4 = Path(move _5);
        drop(_3);
        _3 = move _4;
        _7 = call Path.suffix(_3.0) -> [return: bb2];
    }

    bb2: {
        _8 = \".csv\";
        _9 = call str comparison(_7, _8) -> [return: bb3];
    }

    bb3: {
        _6 = Equal(copy _9, 0_i64);
        branch copy _6 -> [true: bb4, false: bb5];
    }

    bb4: {
        _10 = call str comparison(_3.0, _1.0) -> [return: bb6];
    }

    bb5: {
        drop(_8);
        drop(_7);
        branch copy _6 -> [true: bb7, false: bb8];
    }

    bb6: {
        _6 = Equal(copy _10, 0_i64);
        goto -> bb5;
    }

    bb7: {
        _11 = call Path.parent(_3.0) -> [return: bb9];
    }

    bb8: {
        _12 = call pathlib.Path(owned \"runs\") -> [return: bb10];
    }

    bb9: {
        _0 = Path(move _11);
        drop(_3);
        return;
    }

    bb10: {
        _0 = Path(move _12);
        drop(_3);
        return;
    }
}
"
    );
}

/// Each directory call and `iterdir` has an error edge that raises the `OSError`, and a path
/// is passed by borrowing its text, an element's included.
#[test]
fn path_methods_and_directories_can_fail() {
    let program = "\
        import os\n\
        from pathlib import Path\n\
        \n\
        \n\
        def clean(root: Path) -> Raises[None, OSError]:\n\
        \x20   os.makedirs(root, exist_ok=True)\n\
        \x20   for entry in root.iterdir():\n\
        \x20       if entry.is_file():\n\
        \x20           os.remove(entry)\n";
    assert_eq!(
        lower(program, "clean"),
        "\
fn(_1: Path) -> None {
    let _0: None; // return
    let _1: Path; // parameter root
    let _2: Path; // entry
    let _3: None;
    let _4: Raises[None, OSError];
    let _5: list[Path];
    let _6: Raises[None, OSError];
    let _7: i64;
    let _8: i64;
    let _9: i64;
    let _10: bool;
    let _11: bool;
    let _12: None;
    let _13: Raises[None, OSError];

    bb0: {
        _3 = call os.makedirs(_1.0, owned True) -> [return: bb2, error _4: bb1];
    }

    bb1: {
        drop(_2);
        raise move _4.error[0];
    }

    bb2: {
        _5 = call Path.iterdir(_1.0) -> [return: bb4, error _6: bb3];
    }

    bb3: {
        drop(_2);
        raise move _6.error[0];
    }

    bb4: {
        _7 = len(_5, 0);
        _8 = copy _7;
        _9 = 0_i64;
        goto -> bb5;
    }

    bb5: {
        _10 = Less(copy _9, copy _8);
        branch copy _10 -> [true: bb6, false: bb8];
    }

    bb6: {
        _11 = call Path.is_file(_5[_9].0) -> [return: bb9];
    }

    bb7: {
        _9 = Add(copy _9, 1_i64);
        goto -> bb5;
    }

    bb8: {
        drop(_5);
        _0 = None;
        drop(_2);
        return;
    }

    bb9: {
        branch copy _11 -> [true: bb10, false: bb11];
    }

    bb10: {
        _12 = call os.remove(_5[_9].0) -> [return: bb13, error _13: bb12];
    }

    bb11: {
        goto -> bb7;
    }

    bb12: {
        drop(_5);
        drop(_2);
        raise move _13.error[0];
    }

    bb13: {
        goto -> bb11;
    }
}
"
    );
}
