//! Array files: `Array.fromfile` and `a.tofile` (decision 0036, part 4), and `Array.load` and
//! `Array.save` of `.npy` files (decision 0043), each a call of a [`Builtin::ArrayFile`] that can
//! fail with `OSError`.
//!
//! - `Array.fromfile(path, dtype, count, offset)` passes the path, the count and the offset; a
//!   count left out is -1, which reads to the end of the file, and an offset left out is 0. The
//!   element type is the destination's, so `dtype` passes nothing.
//! - `a.tofile(path)` borrows the array, so a view or the value of an expression writes the
//!   elements it sees.
//! - `Array.load(path)` passes the path; the element type and rank are the destination's.
//! - `Array.save(path, a)` passes the path and borrows the array, as `tofile` does.
//!
//! Each call's error edge goes where any failing call's does (the `errors` module).

use crate::builder::Builder;
use mir::{Argument, ArrayFile, Builtin, Constant, Operand, Place};
use syntax::NodeId;

impl Builder<'_> {
    /// `Array.fromfile(...)` into `place`, a new array; `arguments` are the path, the count and
    /// the offset as the checker recorded them, `None` for one left out.
    pub(crate) fn array_fromfile(
        &mut self,
        place: Place,
        arguments: &[Option<NodeId>],
        node: NodeId,
    ) {
        let &[Some(path), count, offset] = arguments else {
            return self.unsupported(node, "these arguments of `Array.fromfile`");
        };
        let int = self.int_type();
        let path = self.text_argument(path);
        let [count, offset] = [(count, -1), (offset, 0)].map(|(given, default)| {
            Argument::Owned(given.map_or_else(
                || Operand::Constant(Constant::integer(int, default)),
                |given| self.operand_as(given, int),
            ))
        });
        let builtin = Builtin::ArrayFile(ArrayFile::FromFile);
        self.raising_call(builtin, vec![path, count, offset], place, node);
    }

    /// `array.tofile(path)` into `place`, which has type `None`.
    pub(crate) fn array_tofile(&mut self, place: Place, array: NodeId, path: NodeId, node: NodeId) {
        let array = Argument::Borrow(self.borrow(array));
        let path = self.text_argument(path);
        let builtin = Builtin::ArrayFile(ArrayFile::ToFile);
        self.raising_call(builtin, vec![array, path], place, node);
    }

    /// `Array.load(path)` or `Array.save(path, a)`, the built-in `name`, into `place`;
    /// `arguments` are the path, then the array for `save`.
    pub(crate) fn array_npy(
        &mut self,
        place: Place,
        name: &str,
        arguments: &[NodeId],
        node: NodeId,
    ) {
        let (method, given) = match (name, arguments) {
            ("Array.load", &[path]) => (ArrayFile::Load, vec![self.text_argument(path)]),
            ("Array.save", &[path, array]) => {
                let path = self.text_argument(path);
                (
                    ArrayFile::Save,
                    vec![path, Argument::Borrow(self.borrow(array))],
                )
            }
            _ => return self.unsupported(node, "these arguments of a `.npy` function"),
        };
        self.raising_call(Builtin::ArrayFile(method), given, place, node);
    }
}
