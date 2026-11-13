//! The `netCDF4` module (decision 0047): `Dataset(path)`, `var[:]` and `var[:] = values`, each
//! a call of a [`Builtin::ArrayFile`] that can fail with `OSError`, and `path._apply(line)`,
//! which the library's own functions call to change a file. Everything else a program asks of
//! a `Dataset`, a `Variable` or a `Dimension` is a call of a library function the checker
//! recorded where the program wrote it, lowered as any call is.
//!
//! - `Dataset(path)` writes the header's description lines straight into the `Dataset`'s one
//!   field, a `list[str]`. `Dataset(path, "w", format=...)` does the same after making a new
//!   file: its change is `C` and the format's version, which the format's text gives.
//! - `var[:]` passes the path and the name the `Variable` holds, its fields 2 and 0; the element
//!   type and rank are the destination's. `var[:] = values` passes them with the array,
//!   borrowed. A subscript that picks part of the variable, `var[0, :, ::2]`, adds four `int`s
//!   for each item.
//! - `path._apply(line)` passes both `str`s; its result is the new description lines.
//!
//! Each call's error edge goes where any failing call's does (the `errors` module).

use crate::builder::Builder;
use mir::{Argument, ArrayFile, Builtin, Constant, Operand, Place, Projection};
use syntax::{NodeId, Role, SyntaxKind};

/// The position of a `Variable`'s `name` field in `<std>/netCDF4.byn`.
const NAME: u32 = 0;

/// The position of a `Variable`'s `_path` field in `<std>/netCDF4.byn`.
const PATH: u32 = 2;

impl<'db> Builder<'db> {
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
            ("netCDF4.create", &[Some(path), Some(format)]) => {
                let path = self.text_argument(path);
                // `NETCDF3_CLASSIC` is version 1; `NETCDF3_64BIT_OFFSET`, version 2.
                let version = if self.text_of(format).contains("CLASSIC") {
                    1
                } else {
                    2
                };
                let change = self.text_constant(&format!("C\t{version}"));
                let lines = place.project(Projection::Field(0));
                (ArrayFile::NetcdfDefine, vec![path, change], lines)
            }
            ("netCDF4.apply", &[Some(path), Some(line)]) => {
                let path = self.text_argument(path);
                let line = self.text_argument(line);
                (ArrayFile::NetcdfDefine, vec![path, line], place)
            }
            ("netCDF4.read", [Some(variable), items @ ..]) => {
                let variable = self.borrow(*variable);
                let path = Argument::Borrow(variable.clone().project(Projection::Field(PATH)));
                let name = Argument::Borrow(variable.project(Projection::Field(NAME)));
                let mut given = vec![path, name];
                given.extend(self.subscript_words(items));
                (ArrayFile::NetcdfRead, given, place)
            }
            ("netCDF4.write", [Some(variable), Some(array), items @ ..]) => {
                let variable = self.borrow(*variable);
                let path = Argument::Borrow(variable.clone().project(Projection::Field(PATH)));
                let name = Argument::Borrow(variable.project(Projection::Field(NAME)));
                let array = Argument::Borrow(self.borrow(*array));
                let mut given = vec![path, name, array];
                given.extend(self.subscript_words(items));
                (ArrayFile::NetcdfWrite, given, place)
            }
            _ => return self.unsupported(node, "these arguments of a `netCDF4` call"),
        };
        self.raising_call(Builtin::ArrayFile(method), given, place, node);
    }

    /// The four `int` words of each subscript item in `items`, as the runtime reads them: `0`
    /// and the index for an integer, `1` and the start, stop and step for a slice, with
    /// `i64::MIN` for a part the program left out.
    pub(crate) fn subscript_words(&mut self, items: &[Option<NodeId>]) -> Vec<Argument<'db>> {
        let int = self.int_type();
        let left_out = || Operand::Constant(Constant::integer(int, i128::from(i64::MIN)));
        let mut words = Vec::with_capacity(items.len() * 4);
        for &item in items.iter().flatten() {
            if self.tree.kind(item) == SyntaxKind::Slice {
                words.push(Operand::Constant(Constant::integer(int, 1)));
                for role in [Role::Lower, Role::Upper, Role::Step] {
                    let part = self.tree.child(item, role);
                    words.push(part.map_or_else(left_out, |part| self.operand_as(part, int)));
                }
            } else {
                words.push(Operand::Constant(Constant::integer(int, 0)));
                words.push(self.operand_as(item, int));
                words.extend([left_out(), left_out()]);
            }
        }
        words.into_iter().map(Argument::Owned).collect()
    }
}
