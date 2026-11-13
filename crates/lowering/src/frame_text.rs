//! A frame as text (decision 0045): what `print(df)` writes and what a frame in an f-string
//! becomes, which is the table pandas prints for the same frame.
//!
//! The formatting is written in Baryon, in the standard library's `<std>/frames.byn`
//! (decision 0038), so this module only calls it: one call for each column, which gives the
//! column's cells as text, a column of `str` among them, then one call that lays the cells
//! out. A frame's columns are known when compiling, so the calls are chosen here by each
//! column's element type, and MIR, the passes and the backend see only calls of Baryon
//! functions.

use crate::builder::Builder;
use crate::frames::erase;
use mir::{Argument, Callee, Constant, ConstantValue, Operand, Place, Projection, Rvalue};
use mir::{Local, TerminatorKind};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Column, Float, Item, Primitive, Type, TypeKind};

impl<'db> Builder<'db> {
    /// The place to read as text where the expression `node`, of type `ty` and borrowed at
    /// `place`, is printed or formatted in an f-string: a frame's table, a `Path`'s text
    /// ([`Builder::text_place`]), and any other value as it is.
    pub(crate) fn shown_place(&mut self, node: NodeId, place: Place, ty: Type<'db>) -> Place {
        match self.frame_text(node, &place) {
            Some(text) => text,
            None => self.text_place(place, ty),
        }
    }

    /// The place of the text of the frame the expression `node` gives, which `frame` holds: a
    /// new `str`, dropped with the statement's temporaries. `None`, with nothing lowered, when
    /// `node` is not a frame.
    ///
    /// Cost at run time: the library formats only the rows it shows, at most 60, so the text
    /// costs the same for a frame of any length.
    fn frame_text(&mut self, node: NodeId, frame: &Place) -> Option<Place> {
        let columns = self.frame_columns(node)?;
        let text = Type::new(self.db, TypeKind::Str);
        let list = Type::new(self.db, TypeKind::List(text));
        let mut cells = Vec::with_capacity(columns.len());
        let mut names = Vec::with_capacity(columns.len());
        let mut flags = Vec::with_capacity(columns.len());
        let boolean = self.boolean_type();
        for (position, column) in (0..).zip(columns) {
            flags.push(Operand::Constant(Constant {
                ty: boolean,
                value: ConstantValue::Bool(matches!(column.element.kind(self.db), TypeKind::Str)),
            }));
            let (function, type_arguments) = match column.element.kind(self.db) {
                TypeKind::Primitive(Primitive::Float(Float::F64)) => ("float_cells", Vec::new()),
                TypeKind::Primitive(Primitive::Float(Float::F32)) => ("single_cells", Vec::new()),
                TypeKind::Str => ("text_cells", Vec::new()),
                _ => ("cells", vec![column.element]),
            };
            let values = frame.clone().project(Projection::Field(position));
            let formatted = self.frame_call(function, type_arguments, vec![values], list, node)?;
            cells.push(Operand::Move(Place::from(formatted)));
            names.push(Operand::Constant(Constant {
                ty: text,
                value: ConstantValue::Str(column.name.clone()),
            }));
        }
        let headings = self.temporary(list, node);
        self.assign(Place::from(headings), Rvalue::Aggregate(list, names), node);
        self.drop_later(headings);
        // Which columns hold `str`: the library gives their headings no room for a sign.
        let flag_list = Type::new(self.db, TypeKind::List(boolean));
        let text_columns = self.temporary(flag_list, node);
        let aggregate = Rvalue::Aggregate(flag_list, flags);
        self.assign(Place::from(text_columns), aggregate, node);
        self.drop_later(text_columns);
        let table_type = Type::new(self.db, TypeKind::List(list));
        let table = self.temporary(table_type, node);
        let aggregate = Rvalue::Aggregate(table_type, cells);
        self.assign(Place::from(table), aggregate, node);
        self.drop_later(table);
        let count = self.temporary(self.int_type(), node);
        let first = frame.clone().project(Projection::Field(0));
        let stored = match self.ty(node).kind(self.db) {
            TypeKind::Tuple(stored) => stored.first().copied(),
            _ => None,
        };
        let rows = self.column_length(first, stored, node);
        self.assign(Place::from(count), Rvalue::Use(rows), node);
        let arguments = vec![
            Place::from(headings),
            Place::from(table),
            Place::from(text_columns),
            Place::from(count),
        ];
        let result = self.frame_call("frame_text", Vec::new(), arguments, text, node)?;
        self.drop_later(result);
        Some(Place::from(result))
    }

    /// The column of `str` at `column` as whole numbers that stand for its text in keys:
    /// equal texts have one number, and a text that comes first by `<` has the smaller one
    /// (the library's `text_codes`). A sort, `drop_duplicates` and the like then work on the
    /// numbers as on any column of `i64`. The new array is dropped when the statement ends.
    /// Cost: a sort of the column, comparing text, and the array of 8 bytes for each row.
    /// `None` after a report when the library is missing.
    pub(crate) fn text_codes(&mut self, column: Place, node: NodeId) -> Option<Place> {
        let positions = self.positions_type();
        let codes = self.frame_call("text_codes", Vec::new(), vec![column], positions, node)?;
        self.drop_later(codes);
        Some(Place::from(codes))
    }

    /// A new array of the elements of `column`'s values at `from` that stand at the
    /// positions `rows`, in that order: the library's `taken`, or `text_taken` for a column
    /// of `str`. The caller owns the result. `None` after a report.
    pub(crate) fn taken_column(
        &mut self,
        from: Place,
        column: &Column<'db>,
        rows: Place,
        node: NodeId,
    ) -> Option<Local> {
        let (function, types) = match column.element.kind(self.db) {
            TypeKind::Str => ("text_taken", Vec::new()),
            _ => ("taken", vec![column.element]),
        };
        let array = erase(self.db, column.array(self.db));
        self.frame_call(function, types, vec![from, rows], array, node)
    }

    /// Calls the function `name` of `<std>/frames.byn` or `<std>/frame_joins.byn` with
    /// `type_arguments`, borrowing each of `arguments`, and gives the temporary of type
    /// `returns` that holds its result. The functions called this way take every parameter
    /// borrowed and raise nothing. `None` after reporting that the library's file or the
    /// function is missing, which happens only when the library's files were left out of the
    /// package.
    pub(crate) fn frame_call(
        &mut self,
        name: &str,
        type_arguments: Vec<Type<'db>>,
        arguments: Vec<Place>,
        returns: Type<'db>,
        node: NodeId,
    ) -> Option<Local> {
        let Some(item) = self.frame_function(name) else {
            self.unsupported(node, "a `DataFrame` as text without the standard library");
            return None;
        };
        let result = self.temporary(returns, node);
        self.then(node, |next| TerminatorKind::Call {
            callee: Callee::Function {
                item,
                type_arguments,
            },
            arguments: arguments.into_iter().map(Argument::Borrow).collect(),
            destination: Place::from(result),
            target: next,
            error: None,
        });
        Some(result)
    }

    /// The function `name` that `<std>/frames.byn` or `<std>/frame_joins.byn` defines at its
    /// top level. Costs one pass over the top-level statements of each file looked in.
    fn frame_function(&self, name: &str) -> Option<Item<'db>> {
        ["frames", "frame_joins"].into_iter().find_map(|module| {
            let file = database::modules(self.db, self.package).library(module)?;
            let tree = self.db.parse(file).tree();
            let text = file.source(self.db).text();
            let function = tree
                .children_with(tree.root(), Role::Body)
                .filter(|&statement| tree.kind(statement) == SyntaxKind::FunctionDefinition)
                .filter_map(|function| tree.child(function, Role::Name))
                .find(|&function| tree.text(function, text) == name)?;
            Some(Item::new(self.db, self.package, file, function))
        })
    }
}
