//! Parquet files read into a frame (decision 0045, roadmap 3.14.6.3):
//! `DataFrame.read_parquet(path)`.
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

use crate::builder::Builder;
use crate::frames::erase;
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

impl Builder<'_> {
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
