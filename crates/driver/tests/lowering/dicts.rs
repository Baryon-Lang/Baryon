//! Lowering of dictionaries (decision 0035): `d[k]` finds the entry's position, checks it is
//! there, and names the value through an index by position.

use crate::lower;

/// `d[k]` calls `dict lookup` with both borrowed, asserts the position is not negative, and
/// reads the value as field 1 of the entry. The key's text for the panic is built only on the
/// failing path, in the block before the assertion's failure.
#[test]
fn a_lookup_finds_checks_and_indexes() {
    let program = "\
        def mass(d: dict[int, f64], z: int) -> f64:\n\
        \x20   return d[z]\n";
    assert_eq!(
        lower(program, "mass"),
        "\
fn(_1: dict[i64, f64], _2: i64) -> f64 {
    let _0: f64; // return
    let _1: dict[i64, f64]; // parameter d
    let _2: i64; // parameter z
    let _3: i64;
    let _4: bool;
    let _5: str;

    bb0: {
        _3 = call dict lookup(_1, _2) -> [return: bb1];
    }

    bb1: {
        _4 = GreaterEqual(copy _3, 0_i64);
        branch copy _4 -> [true: bb2, false: bb3];
    }

    bb2: {
        _0 = copy _1[_3].1;
        return;
    }

    bb3: {
        _5 = format({_2:});
        assert(copy _4, key present: move _5) -> bb2;
    }
}
"
    );
}
