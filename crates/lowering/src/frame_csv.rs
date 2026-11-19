//! CSV files read into a frame (decision 0045, roadmap 3.14.4): `DataFrame.read_csv(path)`.
//!
//! The reader is Baryon code in `<std>/frame_csv.byn`, which knows nothing of the schema, so
//! this module calls it once for the file and once or more for each column the checker
//! found in the frame's type:
//!
//! 1. `csv_cells(path, names)` reads the file and finds the cells of the columns `names`.
//! 2. For a column of numbers or `bool`s, `csv_floats`, `csv_integers` or `csv_bools` reads
//!    its cells. For a column of `str`, `csv_text_bytes` and `csv_text_offsets` give the two
//!    buffers of a text array. For a column that may hold `None`, `csv_validity` gives the
//!    bitmap that goes beside the values.
//! 3. The columns move into the frame's tuple.
//!
//! The first two steps can fail, so each call has the error edge of the `read_csv` call. A
//! column that is built stays an owning temporary until the last call that can fail has
//! returned, so an error in a later column frees the earlier ones.

use crate::builder::Builder;
use crate::frames::erase;
use mir::{Argument, Callee, Constant, ConstantValue, Local, Operand, Place, Projection, Rvalue};
use syntax::NodeId;
use types::{Column, Primitive, Type, TypeKind};

impl<'db> Builder<'db> {
    /// `DataFrame.read_csv(path)` into `place`, the frame's type being the call's own, which
    /// the checker took from the context.
    pub(crate) fn frame_read_csv(&mut self, place: Place, path: NodeId, node: NodeId) {
        let ty = self.ty(node);
        let Some(columns) = self.frame_columns(node) else {
            return self.unsupported(node, "`DataFrame.read_csv` without the frame's type");
        };
        let text = Type::new(self.db, TypeKind::Str);
        let int = self.int_type();
        let list = |item| Type::new(self.db, TypeKind::List(item));
        let name = |name: &str| {
            Operand::Constant(Constant {
                ty: text,
                value: ConstantValue::Str(name.to_owned()),
            })
        };
        let names = self.temporary(list(text), node);
        self.drop_later(names);
        let written = columns.iter().map(|column| name(&column.name)).collect();
        let written = Rvalue::Aggregate(list(text), written);
        self.assign(Place::from(names), written, node);
        let found = vec![self.bitmap_type(), list(int)];
        let cells = self.temporary(Type::new(self.db, TypeKind::Tuple(found)), node);
        let path = self.borrow(path);
        let given = vec![Argument::Borrow(path), Argument::Borrow(Place::from(names))];
        if !self.csv_call("csv_cells", given, cells, node) {
            return;
        }
        self.drop_later(cells);
        let count = i128::try_from(columns.len()).unwrap_or(i128::MAX);
        let count = Operand::Constant(Constant::integer(int, count));
        let count = Place::from(self.held(int, count, node));
        // The columns are owning temporaries of this scope while a later one may fail, and
        // the frame's once none can.
        self.enter();
        let mut built = Vec::with_capacity(columns.len());
        for (position, column) in (0_i128..).zip(columns) {
            let position = Operand::Constant(Constant::integer(int, position));
            let position = Place::from(self.held(int, position, node));
            let Some(column) = self.csv_column(cells, (position, count.clone()), column, node)
            else {
                self.forget();
                return;
            };
            self.drop_later(column);
            built.push(Operand::Move(Place::from(column)));
        }
        self.forget();
        self.assign(place, Rvalue::Aggregate(ty, built), node);
    }

    /// One column of the file whose cells `cells` holds, as the frame stores it: the column
    /// at `at.0` of the `at.1` columns asked for, with `column`'s name and type. `None` after
    /// reporting a type the reader does not have, which the checker has refused.
    fn csv_column(
        &mut self,
        cells: Local,
        at: (Place, Place),
        column: &Column<'db>,
        node: NodeId,
    ) -> Option<Local> {
        let data = Place::from(cells).project(Projection::Field(0));
        let bounds = Place::from(cells).project(Projection::Field(1));
        let (element, optional) = match column.element.kind(self.db) {
            TypeKind::Optional(value) => (*value, true),
            _ => (column.element, false),
        };
        let places = vec![bounds.clone(), at.0, at.1];
        let values = if matches!(element.kind(self.db), TypeKind::Str) {
            let mut given = vec![data];
            given.extend(places.iter().cloned());
            let bytes = self.frame_call(
                "csv_text_bytes",
                Vec::new(),
                given,
                self.bitmap_type(),
                node,
            )?;
            let offsets = self.array_of(self.int_type());
            let offsets = self.frame_call(
                "csv_text_offsets",
                Vec::new(),
                places.clone(),
                offsets,
                node,
            )?;
            let stored = erase(self.db, Type::new(self.db, TypeKind::TextArray));
            let parts = [bytes, offsets].map(|part| Operand::Move(Place::from(part)));
            let values = self.temporary(stored, node);
            let parts = Rvalue::Aggregate(stored, parts.to_vec());
            self.assign(Place::from(values), parts, node);
            values
        } else {
            let function = match element.kind(self.db) {
                TypeKind::Primitive(Primitive::Float(_)) => "csv_floats",
                TypeKind::Primitive(Primitive::Integer(_)) => "csv_integers",
                TypeKind::Primitive(Primitive::Bool) => "csv_bools",
                _ => {
                    self.unsupported(node, "`DataFrame.read_csv` of a column of this type");
                    return None;
                }
            };
            let boolean = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
            let gaps = Operand::Constant(Constant {
                ty: boolean,
                value: ConstantValue::Bool(optional),
            });
            let gaps = Place::from(self.held(boolean, gaps, node));
            let text = Type::new(self.db, TypeKind::Str);
            let name = Operand::Constant(Constant {
                ty: text,
                value: ConstantValue::Str(column.name.clone()),
            });
            let name = Place::from(self.held(text, name, node));
            let mut given = vec![data];
            given.extend(places.iter().cloned());
            given.extend([gaps, name]);
            let given = given.into_iter().map(Argument::Borrow).collect();
            let values = self.temporary(self.array_of(element), node);
            if !self.csv_call(function, given, values, node) {
                return None;
            }
            values
        };
        if !optional {
            return Some(values);
        }
        let bits = self.frame_call("csv_validity", Vec::new(), places, self.bitmap_type(), node)?;
        let stored = erase(self.db, column.array(self.db));
        let parts = [values, bits].map(|part| Operand::Move(Place::from(part)));
        let whole = self.temporary(stored, node);
        self.assign(
            Place::from(whole),
            Rvalue::Aggregate(stored, parts.to_vec()),
            node,
        );
        Some(whole)
    }

    /// Calls the function `name` of `<std>/frame_csv.byn`, which can fail with the errors of
    /// the `read_csv` call `node`, its result written into `result`. `false` after reporting
    /// that the library's file or the function is missing.
    fn csv_call(
        &mut self,
        name: &str,
        arguments: Vec<Argument<'db>>,
        result: Local,
        node: NodeId,
    ) -> bool {
        let Some(item) = self.frame_function(name) else {
            self.unsupported(node, "`DataFrame.read_csv` without the standard library");
            return false;
        };
        let callee = Callee::Function {
            item,
            type_arguments: Vec::new(),
        };
        self.raising_callee(callee, arguments, Place::from(result), node);
        true
    }
}
