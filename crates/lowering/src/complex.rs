//! Complex numbers beyond the operators (language reference, "Types", "Complex numbers"):
//! `z.real` and `z.imag`, `z.conjugate()`, `abs(z)` and `complex(re, im)`.
//!
//! A complex number's two parts are places of their own, [`Field`](Projection::Field) 0 for the
//! real part and 1 for the imaginary part, as `mir` defines them, so reading a part is a field
//! read, `complex(re, im)` writes each part, and `z.conjugate()` copies `z` and negates the
//! copy's imaginary part. `abs(z)` calls [`MathFunction::Hypot`] of the two parts, as Python's
//! `abs` of a `complex` does. The operators lower as on floats, through `Binary` rvalues, and `**`
//! calls [`MathFunction::Power`] (the `operators` module).

use crate::builder::Builder;
use mir::{MathFunction, Operand, Place, Projection, Rvalue, UnaryOperator};
use syntax::NodeId;
use types::{Primitive, Type, TypeKind};

/// The field of a complex number that holds its real part.
const REAL: u32 = 0;

/// The field of a complex number that holds its imaginary part.
const IMAGINARY: u32 = 1;

impl<'db> Builder<'db> {
    /// Whether `ty` is `complex64` or `complex128`.
    pub(crate) fn is_complex(&self, ty: Type<'db>) -> bool {
        matches!(ty.kind(self.db), TypeKind::Primitive(Primitive::Complex(_)))
    }

    /// The field `name` names on a complex number: [`REAL`] for `real`, [`IMAGINARY`] for
    /// `imag`. `None` for any other name, which the checker has already reported.
    pub(crate) fn complex_part(name: &str) -> Option<u32> {
        match name {
            "real" => Some(REAL),
            "imag" => Some(IMAGINARY),
            _ => None,
        }
    }

    /// The float type of the parts of the complex type `ty`.
    fn part_type(&self, ty: Type<'db>) -> Type<'db> {
        match ty.kind(self.db) {
            TypeKind::Primitive(Primitive::Complex(complex)) => Type::new(
                self.db,
                TypeKind::Primitive(Primitive::Float(complex.part())),
            ),
            _ => ty,
        }
    }

    /// `complex(re, im)` into `place`, of the complex type `ty`: each part converted to the
    /// parts' float type and written to its field.
    pub(crate) fn complex_from_parts(
        &mut self,
        place: &Place,
        ty: Type<'db>,
        (real, imaginary): (NodeId, NodeId),
        node: NodeId,
    ) {
        let part = self.part_type(ty);
        for (field, argument) in [(REAL, real), (IMAGINARY, imaginary)] {
            let operand = self.operand_as(argument, part);
            let target = place.clone().project(Projection::Field(field));
            self.assign(target, Rvalue::Use(operand), node);
        }
    }

    /// `z.conjugate()` into `place`: a copy of `z` whose imaginary part is negated, so its sign
    /// flips even when it is zero or NaN, as Python's `complex(z.real, -z.imag)` gives.
    pub(crate) fn conjugate(&mut self, place: Place, value: NodeId, node: NodeId) {
        let operand = self.operand(value);
        self.assign(place.clone(), Rvalue::Use(operand), node);
        let imaginary = place.project(Projection::Field(IMAGINARY));
        let negated = Rvalue::Unary(UnaryOperator::Negate, Operand::Copy(imaginary.clone()));
        self.assign(imaginary, negated, node);
    }

    /// `abs(z)` of a complex number into `place`: its modulus, `hypot(z.real, z.imag)`, which
    /// neither overflows nor underflows where squaring the parts would.
    pub(crate) fn complex_absolute(&mut self, place: Place, argument: NodeId, node: NodeId) {
        let value = self.borrow(argument);
        let parts = [REAL, IMAGINARY]
            .map(|field| Operand::Copy(value.clone().project(Projection::Field(field))))
            .to_vec();
        self.math_call(place, MathFunction::Hypot, parts, node);
    }
}
