//! CSV files read into a frame and written from one (decision 0045, roadmap 3.14.4):
//! `DataFrame.read_csv(path)` and `df.to_csv(path)`, each with `sep=` for the character
//! between fields, and with `na_values=` and `na_rep=` for how a missing value is spelled
//! (roadmap 3.14.4.2).
//!
//! The reader is Baryon code in `<std>/frame_csv.byn`, which knows nothing of the schema, so
//! this module calls it once for the file and once or more for each column the checker
//! found in the frame's type:
//!
//! 1. `csv_cells(path, names, sep, missing)` reads the file and finds the cells of the
//!    columns `names`.
//! 2. For a column of numbers or `bool`s, `csv_floats`, `csv_integers` or `csv_bools` reads
//!    its cells, or the function of a narrower number type in `<std>/frame_csv_types.byn`.
//!    For a column of `str`, `csv_text_bytes` and `csv_text_offsets` give the two buffers of
//!    a text array. For a column that may hold `None`, `csv_validity` gives the bitmap that
//!    goes beside the values.
//! 3. The columns move into the frame's tuple.
//!
//! The first two steps can fail, so each call has the error edge of the `read_csv` call. A
//! column that is built stays an owning temporary until the last call that can fail has
//! returned, so an error in a later column frees the earlier ones.
//!
//! `to_csv` goes the other way, with `<std>/frame_csv_write.byn`: one call for each column
//! gives its cells as text, by the column's type, and `csv_write` writes them with the
//! header. Only the last call can fail.

use crate::builder::Builder;
use crate::frames::erase;
use crate::optional_arrays::{VALIDITY, VALUES};
use mir::{Argument, Callee, Constant, ConstantValue, Local, Operand, Place, Projection, Rvalue};
use syntax::NodeId;
use types::{Column, Float, Integer, Primitive, Type, TypeDb, TypeKind};

/// The function of the library that reads a column of numbers or `bool`s whose elements are
/// `element`s from a CSV file: one for each type, since each checks its own range. `None`
/// for a type the reader does not have, which the checker has refused.
fn reader<'db>(db: &'db dyn TypeDb, element: Type<'db>) -> Option<&'static str> {
    let TypeKind::Primitive(primitive) = element.kind(db) else {
        return None;
    };
    Some(match primitive {
        Primitive::Bool => "csv_bools",
        Primitive::Float(Float::F64) => "csv_floats",
        Primitive::Float(Float::F32) => "csv_f32",
        Primitive::Integer(Integer::I8) => "csv_i8",
        Primitive::Integer(Integer::I16) => "csv_i16",
        Primitive::Integer(Integer::I32) => "csv_i32",
        Primitive::Integer(Integer::I64) => "csv_integers",
        Primitive::Integer(Integer::U8) => "csv_u8",
        Primitive::Integer(Integer::U16) => "csv_u16",
        Primitive::Integer(Integer::U32) => "csv_u32",
        Primitive::Integer(Integer::U64) => "csv_u64",
        Primitive::Complex(_) => return None,
    })
}

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
        let sep = self.csv_separator(node);
        let Some(missing) = self.csv_spellings(node) else {
            return;
        };
        let given = [path, Place::from(names), sep, missing];
        let given = given.into_iter().map(Argument::Borrow).collect();
        if !self.csv_call("csv_cells", given, Place::from(cells), node) {
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
            let Some(function) = reader(self.db, element) else {
                self.unsupported(node, "`DataFrame.read_csv` of a column of this type");
                return None;
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
            if !self.csv_call(function, given, Place::from(values), node) {
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

    /// `frame.to_csv(path)`, which gives nothing, into `place`: each column's cells as text
    /// from the library, then the file written by its `csv_write`.
    ///
    /// Cost at run time: one text for each cell of the frame, all held until the file is
    /// written.
    pub(crate) fn frame_to_csv(&mut self, place: Place, frame: NodeId, path: NodeId, node: NodeId) {
        let Some(columns) = self.frame_columns(frame) else {
            return self.unsupported(node, "`to_csv` of a value that is not a frame");
        };
        let text = Type::new(self.db, TypeKind::Str);
        let list = Type::new(self.db, TypeKind::List(text));
        let table_type = Type::new(self.db, TypeKind::List(list));
        let int = self.int_type();
        let first = match self.ty(frame).kind(self.db) {
            TypeKind::Tuple(stored) => stored.first().copied(),
            _ => None,
        };
        let stored = self.borrow(frame);
        let sep = self.csv_separator(node);
        let Some(missing) = self.csv_missing(&sep, node) else {
            return;
        };
        let mut cells = Vec::with_capacity(columns.len());
        let mut names = Vec::with_capacity(columns.len());
        for (position, column) in (0..).zip(columns) {
            let at = stored.clone().project(Projection::Field(position));
            let (element, optional) = match column.element.kind(self.db) {
                TypeKind::Optional(value) => (*value, true),
                _ => (column.element, false),
            };
            let mut given = if optional {
                [VALUES, VALIDITY]
                    .map(|part| at.clone().project(Projection::Field(part)))
                    .to_vec()
            } else {
                vec![at]
            };
            // A cell of NaN or of `None` is the text for a missing value.
            if optional
                || matches!(
                    element.kind(self.db),
                    TypeKind::Primitive(Primitive::Float(_))
                )
            {
                given.push(missing.clone());
            }
            // Text is quoted where it holds the separator, so its cells need it.
            if matches!(element.kind(self.db), TypeKind::Str) {
                given.push(sep.clone());
            }
            let (function, types) = match (element.kind(self.db), optional) {
                (TypeKind::Str, false) => ("csv_text_cells", Vec::new()),
                (TypeKind::Str, true) => ("csv_optional_text_cells", Vec::new()),
                (TypeKind::Primitive(Primitive::Float(Float::F64)), false) => {
                    ("csv_float_cells", Vec::new())
                }
                (TypeKind::Primitive(Primitive::Float(Float::F64)), true) => {
                    ("csv_optional_float_cells", Vec::new())
                }
                (TypeKind::Primitive(Primitive::Float(Float::F32)), false) => {
                    ("csv_single_cells", Vec::new())
                }
                (TypeKind::Primitive(Primitive::Float(Float::F32)), true) => {
                    ("csv_optional_single_cells", Vec::new())
                }
                (_, false) => ("csv_plain_cells", vec![element]),
                (_, true) => ("csv_optional_plain_cells", vec![element]),
            };
            let Some(formatted) = self.frame_call(function, types, given, list, node) else {
                return;
            };
            cells.push(Operand::Move(Place::from(formatted)));
            names.push(Operand::Constant(Constant {
                ty: text,
                value: ConstantValue::Str(column.name.clone()),
            }));
        }
        let table = self.temporary(table_type, node);
        self.assign(
            Place::from(table),
            Rvalue::Aggregate(table_type, cells),
            node,
        );
        self.drop_later(table);
        let headings = self.temporary(list, node);
        self.assign(Place::from(headings), Rvalue::Aggregate(list, names), node);
        self.drop_later(headings);
        let rows = stored.project(Projection::Field(0));
        let rows = self.column_length(rows, first, node);
        let rows = Place::from(self.held(int, rows, node));
        let path = self.borrow(path);
        let given = [path, Place::from(headings), Place::from(table), rows, sep];
        let given = given.into_iter().map(Argument::Borrow).collect();
        self.csv_call("csv_write", given, place, node);
    }

    /// The place of the text that separates the fields of the file the call `node`,
    /// `read_csv` or `to_csv`, reads or writes: what `sep=` gives, one character the checker
    /// has seen written out, and `","` without it.
    fn csv_separator(&mut self, node: NodeId) -> Place {
        if let Some(sep) = self.keyword_of(node, "sep") {
            return self.borrow(sep);
        }
        let text = Type::new(self.db, TypeKind::Str);
        let comma = Operand::Constant(Constant {
            ty: text,
            value: ConstantValue::Str(",".to_owned()),
        });
        Place::from(self.held(text, comma, node))
    }

    /// The place of the list of texts that the call `node` of `read_csv` reads as missing
    /// cells, besides the empty one: what `na_values=` gives, a list or one text that the
    /// library's `csv_spelling` puts in a list, and an empty list without it. `None` after
    /// reporting that the library is missing.
    fn csv_spellings(&mut self, node: NodeId) -> Option<Place> {
        let text = Type::new(self.db, TypeKind::Str);
        let list = Type::new(self.db, TypeKind::List(text));
        let Some(written) = self.keyword_of(node, "na_values") else {
            let empty = self.temporary(list, node);
            self.assign(
                Place::from(empty),
                Rvalue::Aggregate(list, Vec::new()),
                node,
            );
            self.drop_later(empty);
            return Some(Place::from(empty));
        };
        let given = self.borrow(written);
        if !matches!(self.ty(written).kind(self.db), TypeKind::Str) {
            return Some(given);
        }
        let one = self.frame_call("csv_spelling", Vec::new(), vec![given], list, node)?;
        self.drop_later(one);
        Some(Place::from(one))
    }

    /// The place of the cell that the call `node` of `to_csv` writes for a missing value, in
    /// a file split by `sep`: what `na_rep=` gives, quoted by the library's `csv_missing`
    /// where it must be, and an empty text without it. `None` after reporting that the
    /// library is missing.
    fn csv_missing(&mut self, sep: &Place, node: NodeId) -> Option<Place> {
        let text = Type::new(self.db, TypeKind::Str);
        let Some(written) = self.keyword_of(node, "na_rep") else {
            let empty = Operand::Constant(Constant {
                ty: text,
                value: ConstantValue::Str(String::new()),
            });
            return Some(Place::from(self.held(text, empty, node)));
        };
        let given = vec![self.borrow(written), sep.clone()];
        let cell = self.frame_call("csv_missing", Vec::new(), given, text, node)?;
        self.drop_later(cell);
        Some(Place::from(cell))
    }

    /// Calls the function `name` of `<std>/frame_csv.byn`, which can fail with the errors of
    /// the `read_csv` or `to_csv` call `node`, its result written into `result`. `false` after
    /// reporting that the library's file or the function is missing.
    fn csv_call(
        &mut self,
        name: &str,
        arguments: Vec<Argument<'db>>,
        result: Place,
        node: NodeId,
    ) -> bool {
        let Some(item) = self.frame_function(name) else {
            self.unsupported(node, "a CSV file without the standard library");
            return false;
        };
        let callee = Callee::Function {
            item,
            type_arguments: Vec::new(),
        };
        self.raising_callee(callee, arguments, result, node);
        true
    }
}
