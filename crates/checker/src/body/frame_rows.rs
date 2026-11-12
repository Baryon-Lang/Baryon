//! Choosing rows of a frame (decision 0045, roadmap 3.13.4): `df[mask]`, `df[i:j]`,
//! `df.head(n)` and `df.tail(n)`, and looping over rows with `df.itertuples()`.
//!
//! Each of the first four gives a new frame with the same columns as the one it reads, so
//! its type is the type of that frame, and it keeps the frame's struct for its rows. A loop
//! over `itertuples()` sees each row as that struct.

use super::calls::Arguments;
use super::{Body, relations};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Integer, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// Whether the subscript's index `index`, whose type is `found` unless it is a slice,
    /// chooses rows of a frame: a slice, `df[1:3]`, or a mask, an `Array[bool, 1]` as in
    /// `df[df.price > 100.0]`. The bounds of a slice are checked here.
    pub(super) fn chooses_rows(&mut self, index: NodeId, found: Option<Type<'db>>) -> bool {
        if self.tree.kind(index) == SyntaxKind::Slice {
            self.index(index);
            return true;
        }
        found.is_some_and(|found| match found.kind(self.db) {
            &TypeKind::Array { element, rank: 1 } => {
                matches!(element.kind(self.db), TypeKind::Primitive(Primitive::Bool))
            }
            _ => false,
        })
    }

    /// `df.head(n)` or `df.tail(n)`, with `callee` the method's name: the first or last `n`
    /// rows as a new frame of the type `base`. `n` is 5 when left out, and may be given by
    /// name, `df.head(n=3)`, as in pandas. `None`, with nothing checked, for any other method.
    pub(super) fn rows_method(
        &mut self,
        function: NodeId,
        base: Type<'db>,
        callee: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let given = arguments.positional.len() + arguments.keywords.len();
        if callee == "itertuples" && given > 0 {
            self.untyped(arguments);
            return Some(self.lowerer.error(
                function,
                "`itertuples` of a `DataFrame` takes no arguments",
                "wrong arguments",
                Some(
                    "each row is the struct of the frame's schema; a frame has no row index to \
                     leave out and its rows need no other name",
                ),
            ));
        }
        if callee == "itertuples" {
            return Some(self.lowerer.error(
                function,
                "`itertuples` gives the rows of a `DataFrame` to a `for` loop",
                "not looped over",
                Some(
                    "write `for row in df.itertuples():`; to keep the rows, build a list, \
                     `[row for row in df.itertuples()]`",
                ),
            ));
        }
        if !matches!(callee, "head" | "tail") {
            return None;
        }
        let count = match (
            arguments.positional.as_slice(),
            arguments.keywords.as_slice(),
        ) {
            ([], []) => None,
            (&[count], []) => Some(count),
            ([], &[(name, count)]) if self.tree.text(name, self.text) == "n" => Some(count),
            _ => {
                self.untyped(arguments);
                let message = format!("`{callee}` of a `DataFrame` takes the number of rows");
                let hint = format!("write `df.{callee}()` for 5 rows, or `df.{callee}(10)`");
                return Some(self.lowerer.error(
                    function,
                    &message,
                    "wrong arguments",
                    Some(&hint),
                ));
            }
        };
        let Some(count) = count else {
            return Some(base);
        };
        let int = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        let found = self.expression(count, Some(int));
        if matches!(
            found.kind(self.db),
            TypeKind::Primitive(Primitive::Integer(_))
        ) {
            return Some(base);
        }
        if relations::is_open(self.db, found) {
            return Some(found);
        }
        let message = format!(
            "`{callee}` takes a number of rows, found `{}`",
            self.show(found)
        );
        Some(self.lowerer.error(count, &message, "not an integer", None))
    }

    /// The frame `df` when `iterator` is `df.itertuples()` with no arguments and `df` has
    /// been typed as a frame: the collection a loop over its rows borrows, so the body
    /// cannot replace `df` while the loop reads it.
    pub(super) fn rows_receiver(&self, iterator: NodeId) -> Option<NodeId> {
        let receiver = self.itertuples_receiver(iterator)?;
        let frame = self.typed.expression(receiver)?;
        matches!(frame.kind(self.db), TypeKind::DataFrame { .. }).then_some(receiver)
    }

    /// The value `df` of the call `df.itertuples()` at `iterator`, whatever its type; `None`
    /// when `iterator` is any other expression or passes arguments.
    fn itertuples_receiver(&self, iterator: NodeId) -> Option<NodeId> {
        if self.tree.kind(iterator) != SyntaxKind::Call {
            return None;
        }
        let function = self.tree.child(iterator, Role::Function)?;
        let attribute = self.tree.child(function, Role::Attribute)?;
        let arguments = self.arguments(iterator);
        let is_rows = self.tree.kind(function) == SyntaxKind::Attribute
            && self.tree.text(attribute, self.text) == "itertuples"
            && arguments.positional.is_empty()
            && arguments.keywords.is_empty();
        is_rows
            .then(|| self.tree.child(function, Role::Value))
            .flatten()
    }

    /// The element type of a `for` loop over `df.itertuples()`: the struct of the frame's
    /// schema, a new value on each pass, as `df[i]` is. `None` when `iterator` is not such
    /// a call on a frame. The frame is typed, so lowering can read its columns.
    pub(super) fn rows_iteration(&mut self, iterator: NodeId) -> Option<Type<'db>> {
        let receiver = self.itertuples_receiver(iterator)?;
        let frame = self.expression(receiver, None);
        let &TypeKind::DataFrame { row, .. } = frame.kind(self.db) else {
            return None;
        };
        Some(row.unwrap_or_else(|| {
            let message = format!("no struct names the rows of `{}`", self.show(frame));
            self.lowerer.error(
                iterator,
                &message,
                "a row needs a type",
                Some(
                    "loop over the columns instead, `for price in df.price:`, or give the frame \
                     a schema: assign it to a variable declared `DataFrame[Trade]`, whose \
                     struct lists these columns",
                ),
            )
        }))
    }
}
