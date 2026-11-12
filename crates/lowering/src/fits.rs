//! The `fits` module (decision 0044): `fits.getheader`, `fits.getdata` and `fits.writeto`,
//! each a call of a [`Builtin::ArrayFile`] that can fail with `OSError`, and the reads and
//! writes of a `Header`'s values, each a call of a library function.
//!
//! - `fits.getheader(filename, ext)` writes the HDU's cards straight into the `Header`'s one
//!   field, a `list[str]`. An `ext` left out is 0, the primary HDU.
//! - `fits.getdata(filename, ext)` passes the path and the HDU; the element type and rank are
//!   the destination's.
//! - `fits.writeto(filename, data, header, overwrite)` borrows the array and the header's
//!   cards; an `overwrite` left out is `False`, and a header left out passes no cards, which
//!   tells the runtime to write `AstroPy`'s `EXTEND` card.
//! - `header[key]`, `key in header` and `header[key] = value` are calls the checker recorded
//!   on the subscript, the comparison or the target, of functions of `<std>/fits.byn`:
//!   [`Builder::is_library_access`] finds them, and [`Builder::call`] lowers each.
//!
//! Each call's error edge goes where any failing call's does (the `errors` module).

use crate::builder::Builder;
use mir::{Argument, ArrayFile, Builtin, Constant, ConstantValue, Operand, Place, Projection};
use syntax::{NodeId, SyntaxKind};
use types::{Primitive, Type, TypeKind};

impl Builder<'_> {
    /// The call `node` of the `fits` built-in `name` into `place`; `arguments` are as the
    /// checker recorded them, `None` for one left out.
    pub(crate) fn fits_call(
        &mut self,
        place: Place,
        name: &str,
        arguments: &[Option<NodeId>],
        node: NodeId,
    ) {
        let int = self.int_type();
        let (method, given, place) = match (name, arguments) {
            ("fits.getheader" | "fits.getdata", &[Some(path), ext]) => {
                let path = self.text_argument(path);
                let ext = Argument::Owned(ext.map_or_else(
                    || Operand::Constant(Constant::integer(int, 0)),
                    |ext| self.operand_as(ext, int),
                ));
                if name == "fits.getdata" {
                    (ArrayFile::FitsData, vec![path, ext], place)
                } else {
                    // The `Header`'s one field, its cards.
                    let cards = place.project(Projection::Field(0));
                    (ArrayFile::FitsHeader, vec![path, ext], cards)
                }
            }
            ("fits.writeto", &[Some(path), Some(data), overwrite, header]) => {
                let path = self.text_argument(path);
                let data = Argument::Borrow(self.borrow(data));
                let bool = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
                let overwrite = Argument::Owned(overwrite.map_or_else(
                    || {
                        Operand::Constant(Constant {
                            ty: bool,
                            value: ConstantValue::Bool(false),
                        })
                    },
                    |overwrite| self.operand(overwrite),
                ));
                let mut given = vec![path, data, overwrite];
                if let Some(header) = header {
                    let cards = self.borrow(header).project(Projection::Field(0));
                    given.push(Argument::Borrow(cards));
                }
                (ArrayFile::FitsWrite, given, place)
            }
            _ => return self.unsupported(node, "these arguments of a `fits` function"),
        };
        self.raising_call(Builtin::ArrayFile(method), given, place, node);
    }

    /// Whether `node`, a subscript or a comparison, is a read of a `Header`, or the target of a
    /// write to one: a call of a library function the checker recorded at it.
    pub(crate) fn is_library_access(&self, node: NodeId) -> bool {
        matches!(
            self.tree.kind(node),
            SyntaxKind::Subscript | SyntaxKind::Compare
        ) && self
            .typed
            .call(node)
            .is_some_and(|call| matches!(call.callee, types::Callee::Function(_)))
    }

    /// `header[key] = value`, the assignment to the subscript `target`, as the call of a
    /// library function the checker recorded at the target, whose result is `None`.
    pub(crate) fn library_store(&mut self, target: NodeId) {
        let nothing = Type::new(self.db, TypeKind::None);
        let result = self.temporary(nothing, target);
        self.call(Place::from(result), target);
    }
}
