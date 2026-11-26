//! Parquet files read into a frame and written from one (decision 0045, roadmap 3.14.6.3
//! and 3.14.6.4): `DataFrame.read_parquet(path)` and `frame.to_parquet(path)`.
//!
//! The reader is Baryon code in the library, which knows nothing of the schema, so this
//! module calls it once for the file and once for each column the checker found in the
//! frame's type:
//!
//! 1. `parquet_plan(path, names)` of `<std>/frame_parquet.byn` reads the file and its
//!    metadata, and plans the reading of the columns `names`.
//! 2. For each column, the function of `<std>/frame_parquet_types.byn` or
//!    `<std>/frame_parquet_text.byn` named after the type of its field, `parquet_f64` or
//!    `parquet_optional_str`, decodes it. Each gives the column as the frame stores it, a
//!    text array as its two buffers and a column that may hold `None` with its validity
//!    bitmap, so nothing is put together here.
//! 3. The columns move into the frame's tuple.
//!
//! Every call can fail, so each has the error edge of the `read_parquet` call. A column
//! that is built stays an owning temporary until the last call has returned, so an error in
//! a later column frees the earlier ones.
//!
//! Writing goes the other way. For each column, the function of
//! `<std>/frame_parquet_put.byn` for its type, `parquet_put_f64` or
//! `parquet_put_optional_str`, gives the column's compressed chunks, and `parquet_write` of
//! `<std>/frame_parquet_write.byn` lays them out with the schema, which it is told as one
//! number for each column, its kind.

use crate::builder::Builder;
use crate::frames::erase;
use crate::optional_arrays::{VALIDITY, VALUES};
use mir::{Argument, Constant, ConstantValue, Operand, Place, Projection, Rvalue};
use syntax::NodeId;
use types::{Float, Integer, Primitive, Type, TypeDb, TypeKind};

/// The name the library's Parquet functions use for a column whose elements are `element`s,
/// `f64` in `parquet_f64`. `None` for a type the reader does not have, which the checker has
/// refused.
fn suffix<'db>(db: &'db dyn TypeDb, element: Type<'db>) -> Option<&'static str> {
    Some(match element.kind(db) {
        TypeKind::Str => "str",
        TypeKind::Primitive(Primitive::Bool) => "bool",
        TypeKind::Primitive(Primitive::Float(Float::F64)) => "f64",
        TypeKind::Primitive(Primitive::Float(Float::F32)) => "f32",
        TypeKind::Primitive(Primitive::Integer(integer)) => match integer {
            Integer::I8 => "i8",
            Integer::I16 => "i16",
            Integer::I32 => "i32",
            Integer::I64 => "i64",
            Integer::U8 => "u8",
            Integer::U16 => "u16",
            Integer::U32 => "u32",
            Integer::U64 => "u64",
        },
        _ => return None,
    })
}

/// The kind `<std>/frame_parquet_write.byn` numbers a column of `element`s by. `None` for a
/// type the writer does not have, which the checker has refused.
fn kind<'db>(db: &'db dyn TypeDb, element: Type<'db>) -> Option<i128> {
    Some(match element.kind(db) {
        TypeKind::Primitive(Primitive::Bool) => 0,
        TypeKind::Primitive(Primitive::Integer(integer)) => match integer {
            Integer::I8 => 1,
            Integer::I16 => 2,
            Integer::I32 => 3,
            Integer::I64 => 4,
            Integer::U8 => 5,
            Integer::U16 => 6,
            Integer::U32 => 7,
            Integer::U64 => 8,
        },
        TypeKind::Primitive(Primitive::Float(Float::F32)) => 9,
        TypeKind::Primitive(Primitive::Float(Float::F64)) => 10,
        TypeKind::Str => 11,
        _ => return None,
    })
}

impl Builder<'_> {
    /// `frame.to_parquet(path)`, which gives nothing, into `place`: each column's chunks
    /// from the library, then the file written by its `parquet_write`.
    ///
    /// Cost at run time: every column's compressed chunks are held until the file is
    /// written.
    pub(crate) fn frame_to_parquet(
        &mut self,
        place: Place,
        frame: NodeId,
        path: NodeId,
        node: NodeId,
    ) {
        let Some(columns) = self.frame_columns(frame) else {
            return self.unsupported(node, "`to_parquet` of a value that is not a frame");
        };
        let text = Type::new(self.db, TypeKind::Str);
        let int = self.int_type();
        let list = |item| Type::new(self.db, TypeKind::List(item));
        let chunk = vec![self.bitmap_type(), list(int)];
        let chunk = Type::new(self.db, TypeKind::Tuple(chunk));
        let first = match self.ty(frame).kind(self.db) {
            TypeKind::Tuple(stored) => stored.first().copied(),
            _ => None,
        };
        let stored = self.borrow(frame);
        let mut chunks = Vec::with_capacity(columns.len());
        let mut names = Vec::with_capacity(columns.len());
        let mut kinds = Vec::with_capacity(columns.len());
        for (position, column) in (0_u32..).zip(columns) {
            let at = stored.clone().project(Projection::Field(position));
            let (element, optional) = match column.element.kind(self.db) {
                TypeKind::Optional(value) => (*value, true),
                _ => (column.element, false),
            };
            let (Some(kind), Some(name)) = (kind(self.db, element), suffix(self.db, element))
            else {
                return self.unsupported(node, "`to_parquet` of a column of this type");
            };
            let kind = if optional { kind + 100 } else { kind };
            let mut given = if optional {
                [VALUES, VALIDITY]
                    .map(|part| at.clone().project(Projection::Field(part)))
                    .to_vec()
            } else {
                vec![at]
            };
            let kind = Operand::Constant(Constant::integer(int, kind));
            given.push(Place::from(self.held(int, kind.clone(), node)));
            let function = if optional {
                format!("parquet_put_optional_{name}")
            } else {
                format!("parquet_put_{name}")
            };
            let Some(encoded) = self.frame_call(&function, Vec::new(), given, chunk, node) else {
                return;
            };
            chunks.push(Operand::Move(Place::from(encoded)));
            kinds.push(kind);
            names.push(Operand::Constant(Constant {
                ty: text,
                value: ConstantValue::Str(column.name.clone()),
            }));
        }
        let mut lists = Vec::with_capacity(3);
        for (item, values) in [(text, names), (int, kinds), (chunk, chunks)] {
            let held = self.temporary(list(item), node);
            self.assign(
                Place::from(held),
                Rvalue::Aggregate(list(item), values),
                node,
            );
            self.drop_later(held);
            lists.push(Place::from(held));
        }
        let rows = stored.project(Projection::Field(0));
        let rows = self.column_length(rows, first, node);
        let rows = Place::from(self.held(int, rows, node));
        let mut given = vec![self.borrow(path)];
        given.extend(lists);
        given.push(rows);
        let given = given.into_iter().map(Argument::Borrow).collect();
        self.csv_call("parquet_write", given, place, node);
    }

    /// `DataFrame.read_parquet(path)` into `place`, the frame's type being the call's own,
    /// which the checker took from the context.
    ///
    /// Cost at run time: the library holds the whole file in memory while the columns are
    /// built, and frees it before the frame is handed over.
    pub(crate) fn frame_read_parquet(&mut self, place: Place, path: NodeId, node: NodeId) {
        let ty = self.ty(node);
        let Some(columns) = self.frame_columns(node) else {
            return self.unsupported(node, "`DataFrame.read_parquet` without the frame's type");
        };
        let text = Type::new(self.db, TypeKind::Str);
        let int = self.int_type();
        let list = |item| Type::new(self.db, TypeKind::List(item));
        let name = |name: &str| Constant {
            ty: text,
            value: ConstantValue::Str(name.to_owned()),
        };
        let names = self.temporary(list(text), node);
        self.drop_later(names);
        let written = columns
            .iter()
            .map(|column| Operand::Constant(name(&column.name)))
            .collect();
        let written = Rvalue::Aggregate(list(text), written);
        self.assign(Place::from(names), written, node);
        let found = vec![self.bitmap_type(), list(int)];
        let file = self.temporary(Type::new(self.db, TypeKind::Tuple(found)), node);
        let path = self.borrow(path);
        let given = [path.clone(), Place::from(names)];
        let given = given.into_iter().map(Argument::Borrow).collect();
        if !self.csv_call("parquet_plan", given, Place::from(file), node) {
            return;
        }
        self.drop_later(file);
        let data = Place::from(file).project(Projection::Field(0));
        let plan = Place::from(file).project(Projection::Field(1));
        // The columns are owning temporaries of this scope while a later one may fail, and
        // the frame's once none can.
        self.enter();
        let mut built = Vec::with_capacity(columns.len());
        for (position, column) in (0_i128..).zip(columns) {
            let (element, optional) = match column.element.kind(self.db) {
                TypeKind::Optional(value) => (*value, true),
                _ => (column.element, false),
            };
            let Some(suffix) = suffix(self.db, element) else {
                self.unsupported(node, "`DataFrame.read_parquet` of a column of this type");
                self.forget();
                return;
            };
            let function = if optional {
                format!("parquet_optional_{suffix}")
            } else {
                format!("parquet_{suffix}")
            };
            let position = Operand::Constant(Constant::integer(int, position));
            let position = Place::from(self.held(int, position, node));
            let label = Operand::Constant(name(&column.name));
            let label = Place::from(self.held(text, label, node));
            let given = [data.clone(), plan.clone(), position, path.clone(), label];
            let given = given.into_iter().map(Argument::Borrow).collect();
            let stored = erase(self.db, column.array(self.db));
            let values = self.temporary(stored, node);
            if !self.csv_call(&function, given, Place::from(values), node) {
                self.forget();
                return;
            }
            self.drop_later(values);
            built.push(Operand::Move(Place::from(values)));
        }
        self.forget();
        self.assign(place, Rvalue::Aggregate(ty, built), node);
    }
}
