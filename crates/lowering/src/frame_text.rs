//! A frame as text (decision 0045): what `print(df)` writes and what a frame in an f-string
//! becomes, which is the table pandas prints for the same frame.
//!
//! The formatting is written in Baryon, in the standard library's `<std>/frames.byn`
//! (decision 0038), so this module only calls it: one call for each column, which gives the
//! column's cells as text, then one call that lays the cells out. A frame's columns are known
//! when compiling, so the calls are chosen here by each column's element type, and MIR, the
//! passes and the backend see only calls of Baryon functions.

use crate::builder::Builder;
use mir::{Argument, Callee, Constant, ConstantValue, Operand, Place, Projection, Rvalue};
use mir::{Local, TerminatorKind};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Float, Item, Primitive, Type, TypeKind};

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
        for (position, column) in (0..).zip(columns) {
            let (function, type_arguments) = match column.element.kind(self.db) {
                TypeKind::Primitive(Primitive::Float(Float::F64)) => ("float_cells", Vec::new()),
                TypeKind::Primitive(Primitive::Float(Float::F32)) => ("single_cells", Vec::new()),
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
        let table_type = Type::new(self.db, TypeKind::List(list));
        let table = self.temporary(table_type, node);
        let aggregate = Rvalue::Aggregate(table_type, cells);
        self.assign(Place::from(table), aggregate, node);
        self.drop_later(table);
        let count = self.temporary(self.int_type(), node);
        let first = frame.clone().project(Projection::Field(0));
        self.assign(Place::from(count), Rvalue::Length(first, 0), node);
        let arguments = vec![
            Place::from(headings),
            Place::from(table),
            Place::from(count),
        ];
        let result = self.frame_call("frame_text", Vec::new(), arguments, text, node)?;
        self.drop_later(result);
        Some(Place::from(result))
    }

    /// Calls the function `name` of `<std>/frames.byn` with `type_arguments`, borrowing each
    /// of `arguments`, and gives the temporary of type `returns` that holds its result. The
    /// functions called this way take every parameter borrowed and raise nothing. `None`
    /// after reporting that the library's file or the function is missing, which happens only
    /// when the library's files were left out of the package.
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

    /// The function `name` that `<std>/frames.byn` defines at its top level. Costs one pass
    /// over the file's top-level statements.
    fn frame_function(&self, name: &str) -> Option<Item<'db>> {
        let file = database::modules(self.db, self.package).library("frames")?;
        let tree = self.db.parse(file).tree();
        let text = file.source(self.db).text();
        let function = tree
            .children_with(tree.root(), Role::Body)
            .filter(|&statement| tree.kind(statement) == SyntaxKind::FunctionDefinition)
            .filter_map(|function| tree.child(function, Role::Name))
            .find(|&function| tree.text(function, text) == name)?;
        Some(Item::new(self.db, self.package, file, function))
    }
}
