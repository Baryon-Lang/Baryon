//! The `netCDF4` module (decision 0047): `Dataset(path)` and `var[:]`, each a call of a
//! [`Builtin::ArrayFile`] that can fail with `OSError`. Everything else a program asks of a
//! `Dataset`, a `Variable` or a `Dimension` is a call of a library function the checker
//! recorded where the program wrote it, lowered as any call is.
//!
//! - `Dataset(path)` writes the header's description lines straight into the `Dataset`'s one
//!   field, a `list[str]`.
//! - `var[:]` passes the path and the name the `Variable` holds, its fields 2 and 0; the element
//!   type and rank are the destination's.
//!
//! Each call's error edge goes where any failing call's does (the `errors` module).

use crate::builder::Builder;
use mir::{Argument, ArrayFile, Builtin, Place, Projection};
use syntax::NodeId;

/// The position of a `Variable`'s `name` field in `<std>/netCDF4.byn`.
const NAME: u32 = 0;

/// The position of a `Variable`'s `_path` field in `<std>/netCDF4.byn`.
const PATH: u32 = 2;

impl Builder<'_> {
    /// The call `node` of the `netCDF4` built-in `name` into `place`; `arguments` are as the
    /// checker recorded them.
    pub(crate) fn netcdf_call(
        &mut self,
        place: Place,
        name: &str,
        arguments: &[Option<NodeId>],
        node: NodeId,
    ) {
        let (method, given, place) = match (name, arguments) {
            ("netCDF4.Dataset", &[Some(path)]) => {
                let path = self.text_argument(path);
                // The `Dataset`'s one field, its description lines.
                let lines = place.project(Projection::Field(0));
                (ArrayFile::NetcdfOpen, vec![path], lines)
            }
            ("netCDF4.read", &[Some(variable)]) => {
                let variable = self.borrow(variable);
                let path = Argument::Borrow(variable.clone().project(Projection::Field(PATH)));
                let name = Argument::Borrow(variable.project(Projection::Field(NAME)));
                (ArrayFile::NetcdfRead, vec![path, name], place)
            }
            _ => return self.unsupported(node, "these arguments of a `netCDF4` call"),
        };
        self.raising_call(Builtin::ArrayFile(method), given, place, node);
    }
}
