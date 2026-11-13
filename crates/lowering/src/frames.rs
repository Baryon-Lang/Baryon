//! Frames (decision 0045): a `DataFrame` is stored as a tuple of its columns, each an `Array`
//! of rank 1, or the two buffers of a text array for a column of `str`, so this module is the
//! only place after the checker that knows a frame from a tuple.
//!
//! [`erase`] replaces every frame type by its tuple. The builder applies it to each type it
//! takes from the checker, so the body it builds, and with it every later stage, stores,
//! moves, borrows and frees a frame as a tuple of arrays. Where lowering must know the columns
//! by name, to find `trades.price` in the tuple, it reads the type the checker gave
//! ([`Builder::frame_columns`]).

use crate::builder::Builder;
use mir::{Constant, ConstantValue, Operand, Place, Projection, Rvalue};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Column, Nominal, Type, TypeDb, TypeKind};

/// `ty` with every `DataFrame` type in it replaced by the tuple of arrays that stores it
/// ([`Type::frame_storage`]), and every text array, `Array[str, 1]`, by the tuple of its two
/// buffers ([`Type::text_storage`]). A type with neither in it comes back unchanged.
///
/// A union is not looked into: its members cannot be frames (the checker rejects them), and
/// rebuilding its list could break the order `types::union` keeps.
pub(crate) fn erase<'db>(db: &'db dyn TypeDb, ty: Type<'db>) -> Type<'db> {
    let map = |ty: Type<'db>| erase(db, ty);
    let all = |types: &[Type<'db>]| types.iter().map(|&ty| map(ty)).collect::<Vec<_>>();
    let nominal = |nominal: &Nominal<'db>| Nominal {
        arguments: all(&nominal.arguments),
        ..nominal.clone()
    };
    let kind = match ty.kind(db) {
        TypeKind::DataFrame { .. } => return ty.frame_storage(db).unwrap_or(ty),
        TypeKind::TextArray => return ty.text_storage(db).unwrap_or(ty),
        TypeKind::Primitive(_)
        | TypeKind::Str
        | TypeKind::None
        | TypeKind::Union(_)
        | TypeKind::Array { .. }
        | TypeKind::Fixed { .. }
        | TypeKind::Function { .. }
        | TypeKind::Standard(_)
        | TypeKind::Parameter { .. }
        | TypeKind::Unknown => return ty,
        TypeKind::Tuple(items) => TypeKind::Tuple(all(items)),
        TypeKind::List(item) => TypeKind::List(map(*item)),
        TypeKind::Dict { key, value } => TypeKind::Dict {
            key: *key,
            value: map(*value),
        },
        TypeKind::Optional(value) => TypeKind::Optional(map(*value)),
        TypeKind::Struct(class) => TypeKind::Struct(nominal(class)),
        TypeKind::Enum(class) => TypeKind::Enum(nominal(class)),
        TypeKind::Callable {
            parameters,
            returns,
        } => TypeKind::Callable {
            parameters: all(parameters),
            returns: map(*returns),
        },
        TypeKind::Fallible { value, errors } => TypeKind::Fallible {
            value: map(*value),
            errors: errors.clone(),
        },
    };
    Type::new(db, kind)
}

impl<'db> Builder<'db> {
    /// The columns of the frame the expression `node` gives, from the type the checker found;
    /// `None` when it is not a frame.
    pub(crate) fn frame_columns(&self, node: NodeId) -> Option<&'db [Column<'db>]> {
        match self.typed.expression(node)?.kind(self.db) {
            TypeKind::DataFrame { columns, .. } => Some(columns),
            _ => None,
        }
    }

    /// The position, in the tuple that stores the frame `value`, of the column that `node`
    /// reads from it: `value.price` or `value["price"]`. `None` when `value` is not a frame or
    /// `node` reads no column of it.
    pub(crate) fn column_position(&self, value: NodeId, node: NodeId) -> Option<u32> {
        let columns = self.frame_columns(value)?;
        let name = match self.tree.kind(node) {
            SyntaxKind::Attribute => self.text_of(self.tree.child(node, Role::Attribute)?),
            SyntaxKind::Subscript => {
                let index = self.tree.child(node, Role::Index)?;
                let text = self.text_of(index);
                (self.tree.kind(index) == SyntaxKind::StringLiteral)
                    .then(|| text.get(1..text.len().checked_sub(1)?))??
            }
            _ => return None,
        };
        // `shape` and `columns` are the frame's own attributes, which hide a column of the
        // same name, as in pandas; `df["shape"]` reaches the column.
        if self.is_frame_attribute(node) {
            return None;
        }
        let position = columns.iter().position(|column| column.name == name)?;
        u32::try_from(position).ok()
    }

    /// The place of the column that `node` reads from a frame, `df.price` or `df["price"]`:
    /// that item of the frame's tuple. `None` when `node` reads no column.
    pub(crate) fn column(&mut self, node: NodeId) -> Option<Place> {
        let value = self.tree.child(node, Role::Value)?;
        let position = self.column_position(value, node)?;
        let frame = self.borrow(value);
        Some(frame.project(Projection::Field(position)))
    }

    /// Whether `node` reads `shape` or `columns` of a frame: values computed at each read,
    /// which name no place.
    pub(crate) fn is_frame_attribute(&self, node: NodeId) -> bool {
        self.tree.kind(node) == SyntaxKind::Attribute
            && self
                .tree
                .child(node, Role::Value)
                .is_some_and(|value| self.frame_columns(value).is_some())
            && self
                .tree
                .child(node, Role::Attribute)
                .is_some_and(|name| matches!(self.text_of(name), "shape" | "columns"))
    }

    /// `df.shape`, the tuple of the number of rows and the number of columns, or `df.columns`,
    /// a new list of the column names, into `place`.
    pub(crate) fn frame_attribute(&mut self, place: Place, node: NodeId) {
        let ty = self.ty(node);
        let (Some(value), Some(name)) = (
            self.tree.child(node, Role::Value),
            self.tree.child(node, Role::Attribute),
        ) else {
            return;
        };
        let Some(columns) = self.frame_columns(value) else {
            return;
        };
        if self.text_of(name) == "columns" {
            let text = Type::new(self.db, TypeKind::Str);
            let names = columns
                .iter()
                .map(|column| {
                    Operand::Constant(Constant {
                        ty: text,
                        value: ConstantValue::Str(column.name.clone()),
                    })
                })
                .collect();
            return self.assign(place, Rvalue::Aggregate(ty, names), node);
        }
        let rows = self.frame_length(value, node);
        let count = i128::try_from(columns.len()).unwrap_or(i128::MAX);
        let count = Operand::Constant(Constant::integer(self.int_type(), count));
        self.assign(place, Rvalue::Aggregate(ty, vec![rows, count]), node);
    }

    /// The number of rows of the frame `value`: the length of its first column, since all
    /// its columns have one length.
    pub(crate) fn frame_length(&mut self, value: NodeId, node: NodeId) -> Operand<'db> {
        let stored = self.stored_columns(self.ty(value));
        let frame = self.borrow(value);
        let first = frame.project(Projection::Field(0));
        self.column_length(first, stored.first().copied(), node)
    }

    /// The types of the columns of a frame as it is stored, `ty` being its erased type: the
    /// items of its tuple. Empty when `ty` is not a tuple.
    fn stored_columns(&self, ty: Type<'db>) -> Vec<Type<'db>> {
        match ty.kind(self.db) {
            TypeKind::Tuple(columns) => columns.clone(),
            _ => Vec::new(),
        }
    }

    /// `DataFrame({"price": prices, "volume": volumes})` into `place`: the arrays move into
    /// the frame's tuple in the order written, then each column's length is checked against
    /// the first one's, so a frame never holds columns of two lengths. A column of another
    /// length panics, as arrays of two lengths do in arithmetic.
    pub(crate) fn frame_new(&mut self, place: &Place, table: NodeId, node: NodeId) {
        let ty = self.ty(node);
        let entries: Vec<NodeId> = self.tree.children_with(table, Role::Item).collect();
        let mut columns = Vec::with_capacity(entries.len());
        for entry in entries {
            let Some(value) = self.tree.child(entry, Role::Value) else {
                return self.unsupported(entry, "this column");
            };
            columns.push(self.operand(value));
        }
        self.assign(place.clone(), Rvalue::Aggregate(ty, columns), node);
        let stored = self.stored_columns(ty);
        let mut first = None;
        for (position, &column) in (0_u32..).zip(&stored) {
            let field = place.clone().project(Projection::Field(position));
            let found = self.column_length(field, Some(column), node);
            match &first {
                Some(first) => self.same_length(0, Operand::clone(first), found, node),
                None => first = Some(found),
            }
        }
    }

    /// Whether `node` reads a column of a frame, `df.price` or `df["price"]`.
    pub(crate) fn is_column(&self, node: NodeId) -> bool {
        self.tree
            .child(node, Role::Value)
            .is_some_and(|value| self.column_position(value, node).is_some())
    }

    /// `df["price"] = values` or `df.price = values`: the new array replaces the column, after
    /// a check that it has the frame's number of rows, so the columns keep one length. The
    /// value is computed first and the old column is dropped, as in any assignment.
    pub(crate) fn column_store(&mut self, target: NodeId, value: NodeId) {
        let ty = self.ty(target);
        let Some(column) = self.column(target) else {
            return;
        };
        let new = self.temporary(ty, value);
        self.write(Place::from(new), ty, value);
        let expected = self.column_length(column.clone(), Some(ty), target);
        let found = self.column_length(Place::from(new), Some(ty), target);
        self.same_length(0, expected, found, target);
        self.replace(column, Operand::Move(Place::from(new)), target);
    }

    /// `df.copy()` into `place`: a new frame holding a copy of each column of `frame`.
    pub(crate) fn frame_copy(&mut self, place: Place, frame: NodeId, node: NodeId) {
        let ty = self.ty(node);
        let TypeKind::Tuple(arrays) = ty.kind(self.db) else {
            return self.unsupported(node, "`copy` of this value");
        };
        let source = self.borrow(frame);
        let mut columns = Vec::with_capacity(arrays.len());
        for (position, &array) in (0_u32..).zip(arrays) {
            let column = source.clone().project(Projection::Field(position));
            columns.push(self.column_copy(column, array, node));
        }
        self.assign(place, Rvalue::Aggregate(ty, columns), node);
    }

    /// A copy of the column at `column`, stored as `stored`: a copy of the array, or of both
    /// buffers of a text column, which is stored as a tuple of the two.
    fn column_copy(&mut self, column: Place, stored: Type<'db>, node: NodeId) -> Operand<'db> {
        let TypeKind::Tuple(buffers) = stored.kind(self.db) else {
            let copy = self.copy_view(column, stored, node);
            return Operand::Move(Place::from(copy));
        };
        let mut copies = Vec::with_capacity(buffers.len());
        for (position, &buffer) in (0_u32..).zip(buffers) {
            let source = column.clone().project(Projection::Field(position));
            let copy = self.copy_view(source, buffer, node);
            copies.push(Operand::Move(Place::from(copy)));
        }
        let whole = self.temporary(stored, node);
        self.assign(Place::from(whole), Rvalue::Aggregate(stored, copies), node);
        Operand::Move(Place::from(whole))
    }

    /// Whether `node` reads a row of a frame, `df[i]`: a subscript of a frame that names no
    /// column. It is a new struct at each read, so it names no place.
    pub(crate) fn is_frame_row(&self, node: NodeId) -> bool {
        self.tree.kind(node) == SyntaxKind::Subscript
            && self
                .tree
                .child(node, Role::Value)
                .is_some_and(|value| self.frame_columns(value).is_some())
            && !self.is_column(node)
    }

    /// `df[i]` into `place`: a struct holding element `i` of each column, in the schema's
    /// order, which is the struct's field order. The index is checked against the number of
    /// rows as an array index is, and a negative literal counts from the end.
    pub(crate) fn frame_row(&mut self, place: Place, node: NodeId) {
        let ty = self.ty(node);
        let (Some(value), Some(index)) = (
            self.tree.child(node, Role::Value),
            self.tree.child(node, Role::Index),
        ) else {
            return;
        };
        let Some(columns) = self.frame_columns(value) else {
            return;
        };
        if self.tree.kind(index) == SyntaxKind::List {
            return self.frame_select(place, value, node);
        }
        if self.tree.kind(index) == SyntaxKind::Slice || self.is_array(self.ty(index)) {
            return self.frame_rows(place, value, index, node);
        }
        let frame = self.borrow(value);
        let first = frame.clone().project(Projection::Field(0));
        let Some(first_type) = columns.first().map(|column| self.column_type(column)) else {
            return;
        };
        let row = self.index(&first, first_type, 0, index, false);
        let fields = (0_u32..)
            .zip(columns)
            .map(|(position, _)| {
                let column = frame.clone().project(Projection::Field(position));
                Operand::Copy(column.project(Projection::Index(vec![row])))
            })
            .collect();
        self.assign(place, Rvalue::Aggregate(ty, fields), node);
    }

    /// `DataFrame(rows)` into `place`, where `rows` is a list of the struct that names the
    /// frame's rows: one new array per field, of the list's length, filled in one pass over
    /// the list, so the cost is one read of the list and one write of the frame.
    pub(crate) fn frame_from_rows(&mut self, place: Place, rows: NodeId, node: NodeId) {
        let ty = self.ty(node);
        let Some(columns) = self.frame_columns(node) else {
            return self.unsupported(node, "this `DataFrame`");
        };
        let list = self.borrow(rows);
        let length = self.temporary(self.int_type(), node);
        self.assign(Place::from(length), Rvalue::Length(list.clone(), 0), node);
        let length = Operand::Copy(Place::from(length));
        let arrays: Vec<_> = columns
            .iter()
            .map(|column| {
                let array = self.column_type(column);
                let local = self.temporary(array, node);
                let lengths = vec![length.clone()];
                self.assign(Place::from(local), Rvalue::NewArray(array, lengths), node);
                local
            })
            .collect();
        self.loop_nest(&[length], &mut Vec::new(), node, &mut |builder, indices| {
            let row = list.clone().project(Projection::Index(indices.to_vec()));
            for (position, &array) in (0_u32..).zip(&arrays) {
                let field = row.clone().project(Projection::Field(position));
                let element = Place::from(array).project(Projection::Index(indices.to_vec()));
                builder.assign(element, Rvalue::Use(Operand::Copy(field)), node);
            }
        });
        let columns = arrays
            .into_iter()
            .map(|array| Operand::Move(Place::from(array)))
            .collect();
        self.assign(place, Rvalue::Aggregate(ty, columns), node);
    }

    /// The type of a column as lowering stores it: an array of rank 1 of the column's
    /// elements.
    fn column_type(&self, column: &Column<'db>) -> Type<'db> {
        let element = column.element;
        Type::new(self.db, TypeKind::Array { element, rank: 1 })
    }
}
