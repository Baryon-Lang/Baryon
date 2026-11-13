//! Tests of the messages failed checks report: the lengths of a whole-array length check and
//! the exponent of integer `**`.

use super::{Database, end, primitive, span};
use crate::{BlockId, Body, Check, LocalKind, Operand, TerminatorKind};
use types::{Integer, Primitive};

/// `a + b` over arrays checks that each dimension of `b` has `a`'s length, and the check reports
/// both lengths.
#[test]
fn same_length_check_reports_both_lengths() {
    let database = Database::default();
    let bool = primitive(&database, Primitive::Bool);
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let mut body = Body::new(bool, span());
    let expected = body.push_local(i64, LocalKind::Temporary, span());
    let found = body.push_local(i64, LocalKind::Temporary, span());
    let same = body.push_local(bool, LocalKind::Temporary, span());
    let check = Check::SameLength {
        dimension: 0,
        expected: Operand::Copy(expected.into()),
        found: Operand::Copy(found.into()),
    };
    assert_eq!(check.operands().len(), 2);
    body.push_block(end(TerminatorKind::Assert {
        condition: Operand::Copy(same.into()),
        check,
        target: BlockId::new(1),
    }));
    body.push_block(end(TerminatorKind::Return));

    assert_eq!(body.validate(&database), Vec::new());
    assert!(
        body.display(&database)
            .to_string()
            .contains("assert(copy _3, same length in dimension 0: copy _2 == copy _1) -> bb1;")
    );
}

/// Integer `x ** n` checks that `n` is not negative, and the check reports the exponent; its
/// multiplications check for overflow as `**`, which a release build leaves out.
#[test]
fn exponent_check_reports_the_exponent() {
    let database = Database::default();
    let bool = primitive(&database, Primitive::Bool);
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let mut body = Body::new(bool, span());
    let exponent = body.push_local(i64, LocalKind::Temporary, span());
    let fits = body.push_local(bool, LocalKind::Temporary, span());
    let check = Check::NonNegativeExponent {
        exponent: Operand::Copy(exponent.into()),
    };
    assert_eq!(check.operands().len(), 1);
    assert!(!check.is_overflow() && Check::NoPowerOverflow.is_overflow());
    body.push_block(end(TerminatorKind::Assert {
        condition: Operand::Copy(fits.into()),
        check,
        target: BlockId::new(1),
    }));
    body.push_block(end(TerminatorKind::Assert {
        condition: Operand::Copy(fits.into()),
        check: Check::NonZeroStep { slice: false },
        target: BlockId::new(2),
    }));
    body.push_block(end(TerminatorKind::Return));

    assert_eq!(body.validate(&database), Vec::new());
    let text = body.display(&database).to_string();
    assert!(text.contains("assert(copy _2, non-negative exponent: copy _1) -> bb1;"));
    assert!(text.contains("assert(copy _2, non-zero step) -> bb2;"));
}
