//! Choosing rows of a frame (decision 0045, roadmap 3.13.4): `df[mask]`, `df[i:j]`,
//! `df.head(n)` and `df.tail(n)`.
//!
//! Each gives a new frame with the same columns as the one it reads, so its type is the type
//! of that frame, and it keeps the frame's struct for its rows.

use super::calls::Arguments;
use super::{Body, relations};
use syntax::{NodeId, SyntaxKind};
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
}
