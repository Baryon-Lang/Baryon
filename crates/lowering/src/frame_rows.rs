//! Choosing rows of a frame (decision 0045, roadmap 3.13.4): `df[mask]`, `df[i:j]`,
//! `df.head(n)` and `df.tail(n)`, each a new frame with the same columns, and the rows a
//! `for` loop over `df.itertuples()` walks, each a new struct of the frame's schema.
//!
//! The work is written in Baryon, in the standard library's `<std>/frames.byn` (decision
//! 0038). A mask is applied to each column by `masked`. For the others one call works out
//! the positions of the rows, and `taken` copies those elements of each column. The frame's
//! columns are known when compiling, so there is one call for each column, with the column's
//! element type as the type argument, and the results are the new frame's columns.

use crate::builder::Builder;
use crate::frames::erase;
use mir::{BinaryOperator, Check, Constant, Local, Operand, Place, Projection, Rvalue};
use mir::{ConstantValue, StatementKind, TerminatorKind};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Column, Type, TypeKind};

/// The rows of a frame that a `for` loop over `df.itertuples()` walks.
pub(crate) struct Rows<'db> {
    /// The borrowed frame.
    frame: Place,
    /// The frame's columns.
    columns: &'db [Column<'db>],
    /// The struct of one row.
    pub(crate) row: Type<'db>,
    /// The local holding the number of rows.
    pub(crate) length: Local,
}

impl<'db> Builder<'db> {
    /// The call `node` of the method `method` of the frame `frame`, with the arguments
    /// `rest`, into `place`: `copy()`, `head(n)` or `tail(n)`, a method that changes the
    /// columns (the `frame_schema` module) or `sort_values` (the `frame_sort` module).
    pub(crate) fn frame_method(
        &mut self,
        place: Place,
        method: &str,
        (frame, rest): (NodeId, &[NodeId]),
        node: NodeId,
    ) {
        if self.schema_method(&place, method, frame, node) {
            return;
        }
        match (method, rest) {
            ("dropna", []) => return self.frame_dropna(place, frame, node),
            ("fillna", &[value]) => return self.frame_fillna(place, frame, value, node),
            _ => {}
        }
        if method == "merge" {
            return self.frame_merge(place, frame, rest, node);
        }
        if method == "describe" {
            return self.frame_describe(place, frame, node);
        }
        if method == "value_counts" {
            return self.frame_counts(place, frame, node);
        }
        if method == "drop_duplicates" {
            return self.frame_distinct(place, frame, node);
        }
        if method == "agg" {
            return self.frame_agg(place, frame, node);
        }
        if method == "sort_values" {
            return self.frame_sort(place, frame, node);
        }
        let function = match (method, rest.len()) {
            ("copy", 0) => return self.frame_copy(place, frame, node),
            ("head", 0 | 1) => "head_positions",
            ("tail", 0 | 1) => "tail_positions",
            _ => return self.unsupported(node, "this method of a `DataFrame`"),
        };
        let int = self.int_type();
        // The number of rows, 5 when left out, given by position or as `n=`.
        let rows = match rest.first() {
            Some(&given) if self.tree.kind(given) == SyntaxKind::Keyword => {
                self.tree.child(given, Role::Value)
            }
            given => given.copied(),
        };
        let rows = match rows {
            Some(rows) => self.operand_as(rows, int),
            None => Operand::Constant(Constant::integer(int, 5)),
        };
        let rows = self.held(int, rows, node);
        let source = self.borrow(frame);
        let count = self.counted_rows(frame, &source, node);
        let arguments = vec![Place::from(count), Place::from(rows)];
        self.taken_rows(place, frame, &source, (function, arguments), node);
    }

    /// `df[mask]` or `df[i:j]`, the subscript `node` of the frame `value` with the index
    /// `index`, into `place`: a new frame of the rows chosen.
    ///
    /// A mask of another length than the frame panics here, at the mask, before any column
    /// is read. A slice follows Python: its bounds are clamped to the frame, and a step of 0
    /// panics.
    pub(crate) fn frame_rows(&mut self, place: Place, value: NodeId, index: NodeId, node: NodeId) {
        let source = self.borrow(value);
        let count = self.counted_rows(value, &source, node);
        if self.tree.kind(index) != SyntaxKind::Slice {
            let mask = self.borrow(index);
            let chosen = self.length(&mask, self.ty(index), 0, index);
            self.same_length(0, Operand::Copy(Place::from(count)), chosen, index);
            return self.each_column(place, value, &source, ("masked", &mask), node);
        }
        let Some(chosen) = self.slice_positions(count, index, node) else {
            return;
        };
        self.each_column(place, value, &source, ("taken", &Place::from(chosen)), node);
    }

    /// The positions the slice `index` chooses from `count` rows or elements, as Python
    /// chooses them: an `Array[i64, 1]` the library's `slice_positions` makes, dropped with
    /// the statement's temporaries, so 8 bytes for each position. A step of 0 panics. `None`
    /// after a report.
    pub(crate) fn slice_positions(
        &mut self,
        count: Local,
        index: NodeId,
        node: NodeId,
    ) -> Option<Local> {
        let int = self.int_type();
        let boolean = self.boolean_type();
        let mut arguments = vec![Place::from(count)];
        let mut given = Vec::with_capacity(2);
        for (role, default) in [(Role::Lower, 0), (Role::Upper, 0), (Role::Step, 1)] {
            let bound = self.tree.child(index, role);
            let operand = match bound {
                Some(bound) => self.operand_as(bound, int),
                None => Operand::Constant(Constant::integer(int, default)),
            };
            arguments.push(Place::from(self.held(int, operand, index)));
            given.push(bound.is_some());
        }
        let step = arguments[3].clone();
        let nonzero = self.temporary(boolean, index);
        let zero = Operand::Constant(Constant::integer(int, 0));
        let compared = Rvalue::Binary(BinaryOperator::NotEqual, Operand::Copy(step), zero);
        self.assign(Place::from(nonzero), compared, index);
        self.then(index, |target| TerminatorKind::Assert {
            condition: Operand::Copy(Place::from(nonzero)),
            check: Check::NonZeroStep { slice: true },
            target,
        });
        for is_given in &given[..2] {
            let flag = Operand::Constant(Constant {
                ty: boolean,
                value: ConstantValue::Bool(*is_given),
            });
            arguments.push(Place::from(self.held(boolean, flag, index)));
        }
        let positions = self.positions_type();
        let chosen = self.frame_call("slice_positions", Vec::new(), arguments, positions, node)?;
        self.drop_later(chosen);
        Some(chosen)
    }

    /// A new frame into `place` of the rows of the frame `value`, borrowed at `source`, at
    /// the positions the library function `positions.0` gives for the arguments
    /// `positions.1`. The positions are one `Array[i64, 1]`, dropped with the statement's
    /// temporaries, so the cost beyond the copy is 8 bytes for each row chosen.
    fn taken_rows(
        &mut self,
        place: Place,
        value: NodeId,
        source: &Place,
        (function, arguments): (&str, Vec<Place>),
        node: NodeId,
    ) {
        let int = self.int_type();
        let positions = Type::new(
            self.db,
            TypeKind::Array {
                element: int,
                rank: 1,
            },
        );
        let Some(chosen) = self.frame_call(function, Vec::new(), arguments, positions, node) else {
            return;
        };
        self.drop_later(chosen);
        self.each_column(place, value, source, ("taken", &Place::from(chosen)), node);
    }

    /// A new frame into `place` whose columns are the library function `kernel.0` of each
    /// column of the frame `value`, borrowed at `source`, and of `kernel.1`: `masked` with a
    /// mask, or `taken` with positions. A column of `str` goes through `text_masked` or
    /// `text_taken`, which give a text array.
    pub(crate) fn each_column(
        &mut self,
        place: Place,
        value: NodeId,
        source: &Place,
        (function, by): (&str, &Place),
        node: NodeId,
    ) {
        let (ty, Some(columns)) = (self.ty(node), self.frame_columns(value)) else {
            return;
        };
        let mut chosen = Vec::with_capacity(columns.len());
        for (position, column) in (0_u32..).zip(columns) {
            // A column of `str` has its own functions, `text_masked` and `text_taken`.
            let text = matches!(column.element.kind(self.db), TypeKind::Str);
            let name = if text {
                format!("text_{function}")
            } else {
                function.to_owned()
            };
            let types = if text {
                Vec::new()
            } else {
                vec![column.element]
            };
            let array = erase(self.db, column.array(self.db));
            let from = source.clone().project(Projection::Field(position));
            let arguments = vec![from, by.clone()];
            let Some(result) = self.frame_call(&name, types, arguments, array, node) else {
                return;
            };
            chosen.push(Operand::Move(Place::from(result)));
        }
        self.assign(place, Rvalue::Aggregate(ty, chosen), node);
    }

    /// A temporary of type `ty` holding `operand`, for an argument a library function
    /// borrows.
    pub(crate) fn held(&mut self, ty: Type<'db>, operand: Operand<'db>, node: NodeId) -> Local {
        let local = self.temporary(ty, node);
        self.assign(Place::from(local), Rvalue::Use(operand), node);
        local
    }

    /// The frame `df` and the struct of its rows when `node` is the call `df.itertuples()`,
    /// whose rows a `for` loop walks (the `iterators` module); `None` for any other
    /// expression. The checker has reported a frame with no struct for its rows.
    pub(crate) fn itertuples_frame(&self, node: NodeId) -> Option<(NodeId, Type<'db>)> {
        if self.tree.kind(node) != SyntaxKind::Call {
            return None;
        }
        let function = self.tree.child(node, Role::Function)?;
        let name = self.tree.child(function, Role::Attribute)?;
        let frame = self.tree.child(function, Role::Value)?;
        match self.typed.expression(frame)?.kind(self.db) {
            &TypeKind::DataFrame { row: Some(row), .. } if self.text_of(name) == "itertuples" => {
                Some((frame, row))
            }
            _ => None,
        }
    }

    /// The rows a loop over the call `node`, `df.itertuples()`, walks: the frame computed
    /// once and borrowed for the loop, and its rows counted. `None`, with nothing lowered,
    /// when `node` is any other expression.
    pub(crate) fn rows(&mut self, node: NodeId) -> Option<Rows<'db>> {
        let (value, row) = self.itertuples_frame(node)?;
        let columns = self.frame_columns(value).unwrap_or_default();
        let frame = self.borrow(value);
        let length = self.counted_rows(value, &frame, node);
        Some(Rows {
            frame,
            columns,
            row,
            length,
        })
    }

    /// A temporary holding row `at` of `rows` as the schema's struct: a copy of element `at`
    /// of each column, in the order of the struct's fields, a field of `str` owning a copy
    /// of its text ([`Builder::row_field`]). `at` is a row the loop has counted, so it is in
    /// range.
    pub(crate) fn row_at(&mut self, rows: &Rows<'db>, at: Local, node: NodeId) -> Place {
        let fields = (0_u32..)
            .zip(rows.columns)
            .map(|(position, column)| {
                let values = rows.frame.clone().project(Projection::Field(position));
                self.row_field(values, column, at, node)
            })
            .collect();
        let value = self.temporary(rows.row, node);
        // A row that owns text is dropped when the next row replaces it, and the last row
        // when the loop's statement ends.
        if self.needs_drop(rows.row) {
            self.push(StatementKind::Drop(Place::from(value)), node);
            self.drop_later(value);
        }
        self.assign(
            Place::from(value),
            Rvalue::Aggregate(rows.row, fields),
            node,
        );
        Place::from(value)
    }

    /// The number of rows of the frame the expression `frame` gives, borrowed at `source`,
    /// in a temporary: the length of its first column, counted from the offsets when that
    /// column holds `str`.
    pub(crate) fn counted_rows(&mut self, frame: NodeId, source: &Place, node: NodeId) -> Local {
        let stored = match self.ty(frame).kind(self.db) {
            TypeKind::Tuple(columns) => columns.first().copied(),
            _ => None,
        };
        let first = source.clone().project(Projection::Field(0));
        let rows = self.column_length(first, stored, node);
        self.held(self.int_type(), rows, node)
    }
}
