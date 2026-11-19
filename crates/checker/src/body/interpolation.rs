//! `Array.interp(x, xp, fp)` (decision 0069): `NumPy`'s `interp`, the piecewise linear
//! interpolation of points, called through `Array` as `numpy.loadtxt` became `Array.loadtxt`.
//!
//! The work is the function `interp` in `<std>/arrays.byn`, written in Baryon (decision 0038).
//! The call is checked as a call of that function, so its arguments, keywords and defaults are
//! the function's: `x`, `xp` and `fp` are `Array[f64, 1]`, and `left`, `right` and `period`
//! are optional `f64`s.

use super::Body;
use super::calls::Arguments;
use syntax::NodeId;
use types::Type;

impl<'db> Body<'db> {
    /// `Array.interp(x, xp, fp, left=None, right=None, period=None)`, checked as the library's
    /// `interp`.
    pub(super) fn array_interp(&mut self, node: NodeId, arguments: &Arguments) -> Type<'db> {
        let Some((file, name)) = self.library_function("arrays", "interp") else {
            self.untyped(arguments);
            return self.lowerer.error(
                node,
                "`Array.interp` is missing from the standard library",
                "cannot be typed",
                Some("the compiler adds the library's files to every package (decision 0038)"),
            );
        };
        self.call_item(node, "Array.interp", file, name, arguments)
    }
}
