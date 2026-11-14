//! The `DataFrame[S]` type form (decision 0045): the struct `S` lists the frame's columns, one
//! field for each, and a row of the frame is an `S`.

use crate::embed;
use crate::lower::Lowerer;
use syntax::NodeId;
use types::{Column, Type, TypeDb, TypeKind};

impl<'db> Lowerer<'db> {
    /// `DataFrame[schema]`, written at `node` with the type arguments of `subscript`: a frame
    /// with one column for each field of the struct `schema` names, in the order the struct
    /// declares them.
    pub(crate) fn frame(&mut self, node: NodeId, subscript: Option<NodeId>) -> Type<'db> {
        let Some(subscript) = subscript else {
            return self.error(
                node,
                "`DataFrame` needs type arguments",
                "no type arguments",
                Some("for example `DataFrame[Trade]`"),
            );
        };
        let &[schema] = self.arguments(subscript).as_slice() else {
            let found = self.arguments(subscript).len();
            return self.arity(node, "DataFrame", 1, found);
        };
        let row = self.lower(schema);
        self.frame_of(schema, row)
    }

    /// The frame type whose rows are the struct `row`: one column for each of its fields. An
    /// error, reported at `schema`, when `row` is not a struct of columns.
    pub(crate) fn frame_of(&mut self, schema: NodeId, row: Type<'db>) -> Type<'db> {
        let TypeKind::Struct(nominal) = row.kind(self.db) else {
            if matches!(row.kind(self.db), TypeKind::Unknown) {
                return row;
            }
            let message = format!(
                "a `DataFrame`'s schema is a struct that lists its columns, found `{}`",
                row.display(self.db)
            );
            return self.error(
                schema,
                &message,
                "not a struct",
                Some(
                    "declare the columns as the fields of a struct, `class Trade(Struct):` with \
                     `price: f64` and `volume: i64`, then write `DataFrame[Trade]`",
                ),
            );
        };
        let fields = embed::fields(self.db, self.package, nominal);
        if fields.is_empty() {
            let message = format!(
                "`{}` has no fields, so a `DataFrame` of it would have no columns",
                row.display(self.db)
            );
            return self.error(schema, &message, "no columns", None);
        }
        let mut columns = Vec::new();
        for (name, element) in fields {
            if matches!(element.kind(self.db), TypeKind::Unknown) {
                return element;
            }
            if !is_column(self.db, element) {
                let message = format!(
                    "a column of a `DataFrame` cannot hold `{}` yet",
                    element.display(self.db)
                );
                let label = format!("the field `{name}` would be such a column");
                return self.error(schema, &message, &label, Some(COLUMN_TYPES));
            }
            columns.push(Column { name, element });
        }
        let row = Some(row);
        Type::new(self.db, TypeKind::DataFrame { columns, row })
    }

    /// `ty`, the type of a struct field written at `annotation`, or the unknown type after an
    /// error when it is or holds a `DataFrame` or a text array, `Array[str, 1]`. Lowering
    /// stores each as a tuple of arrays in every body it builds, but a struct's declaration is
    /// read by the stages after it too, so a field cannot hold one until they learn that.
    pub(crate) fn without_frame(&mut self, annotation: NodeId, ty: Type<'db>) -> Type<'db> {
        if crate::body::relations::holds_text_array(self.db, ty) {
            return self.error(
                annotation,
                "a struct field cannot hold an array of `str` yet",
                "holds an array of `str`",
                Some(
                    "keep the array in a variable and pass it beside the struct, or hold a \
                     `list[str]`; like a frame in a struct field, this is roadmap idea I.49",
                ),
            );
        }
        if crate::body::relations::holds_optional_array(self.db, ty) {
            return self.error(
                annotation,
                "a struct field cannot hold an array with missing values yet",
                "holds an array that may hold `None`",
                Some(
                    "keep the array in a variable and pass it beside the struct; like a frame \
                     in a struct field, this is roadmap idea I.49",
                ),
            );
        }
        if !crate::body::relations::holds_frame(self.db, ty) {
            return ty;
        }
        self.error(
            annotation,
            "a struct field cannot hold a `DataFrame` yet",
            "holds a frame",
            Some(
                "keep the frame in a variable and pass it beside the struct; frames in struct \
                 fields are roadmap idea I.49",
            ),
        )
    }
}

/// What a column may hold today, for the note of an error about one that holds something else.
pub(crate) const COLUMN_TYPES: &str = "a column holds numbers, `bool` or `str` today, and \
    numbers or `bool`s that may be `None`";

/// Whether a column of a frame may hold elements of type `ty` today: a number, a `bool`, a
/// `str`, whose column is a text array, or a number or `bool` that may be `None`, whose
/// column is an array with missing values.
pub(crate) fn is_column<'db>(db: &'db dyn TypeDb, ty: Type<'db>) -> bool {
    match ty.kind(db) {
        TypeKind::Primitive(_) | TypeKind::Str => true,
        TypeKind::Optional(element) => matches!(element.kind(db), TypeKind::Primitive(_)),
        _ => false,
    }
}
