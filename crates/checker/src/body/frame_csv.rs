//! CSV files read into a frame (decision 0045, roadmap 3.14.4): `DataFrame.read_csv(path)`.
//!
//! A compiled program cannot learn a file's columns when it runs, so the schema comes from
//! the context, as the rank of `Array.loadtxt` does (decision 0043):
//!
//! ```text
//! trades: DataFrame[Trade] = DataFrame.read_csv("trades.csv")
//! ```
//!
//! The file's header names its columns; each column of the schema is read from the column
//! of that name, wherever it stands, and columns the schema does not name are skipped. A
//! column of `f64`, `i64`, `bool` or `str` is read, and one of those that may be `None`,
//! where an empty cell is `None`. The call can fail with `OSError`, when the file cannot be
//! read, and with `ValueError`, when the header lacks a column or a cell is not a value of
//! its column's type.
//!
//! The first version takes the path alone: every keyword `pandas.read_csv` has is an error
//! that names it, never silently ignored.

use super::Body;
use super::calls::Arguments;
use syntax::NodeId;
use types::{Call, Callee, Float, Integer, Key, Primitive, Type, TypeKind};

/// How a call is written, for the hints.
const EXAMPLE: &str = "for example `trades: DataFrame[Trade] = DataFrame.read_csv(\"trades.csv\")`";

impl<'db> Body<'db> {
    /// The call `node` of `DataFrame.name(...)`, a function found on the built-in name
    /// `DataFrame`: `read_csv` alone. `expected` is the type the context needs.
    pub(super) fn frame_function(
        &mut self,
        node: NodeId,
        name: &str,
        arguments: &Arguments,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        if name != "read_csv" {
            self.untyped(arguments);
            let message = format!("`DataFrame` has no function `{name}`");
            let hint = "a frame is made with `DataFrame({...})`, `DataFrame(rows)` or \
                        `DataFrame.read_csv(path)`";
            return self
                .lowerer
                .error(node, &message, "not defined", Some(hint));
        }
        if let Some(&(keyword, _)) = arguments.keywords.first() {
            self.untyped(arguments);
            let message = format!(
                "`DataFrame.read_csv` does not take `{}` yet",
                self.tree.text(keyword, self.text)
            );
            let hint = "this version reads a comma-separated file with a header row, given \
                        its path alone";
            return self
                .lowerer
                .error(keyword, &message, "not built yet", Some(hint));
        }
        let &[path] = arguments.positional.as_slice() else {
            self.untyped(arguments);
            return self.lowerer.error(
                node,
                "`DataFrame.read_csv` takes the path of the file",
                "wrong arguments",
                Some(EXAMPLE),
            );
        };
        let str = Type::new(self.db, TypeKind::Str);
        let found = self.expression(path, Some(str));
        self.expect(path, str, found, "a path is");
        let Some(frame) =
            expected.filter(|ty| matches!(ty.kind(self.db), TypeKind::DataFrame { .. }))
        else {
            return self.lowerer.error(
                node,
                "`DataFrame.read_csv` needs the frame's type from the context",
                "which columns, of which types?",
                Some(
                    "a compiled program cannot learn the columns from the file: write the \
                     schema as a struct and annotate the variable, \
                     `trades: DataFrame[Trade] = DataFrame.read_csv(path)`",
                ),
            );
        };
        let TypeKind::DataFrame { columns, .. } = frame.kind(self.db) else {
            return self.lowerer.unknown();
        };
        for column in columns {
            if !self.is_csv_element(column.element) {
                let message = format!(
                    "`DataFrame.read_csv` cannot read the column `{}` of type `{}` yet",
                    column.name,
                    self.show(column.element)
                );
                return self.lowerer.error(
                    node,
                    &message,
                    "not built yet",
                    Some(
                        "a column of `f64`, `i64`, `bool` or `str` is read, and one of those \
                         that may be `None`; read the column as one of them and convert it",
                    ),
                );
            }
        }
        let call = Call {
            callee: Callee::Builtin("DataFrame.read_csv".to_owned()),
            type_arguments: Vec::new(),
            arguments: vec![Some(Key(path))],
        };
        self.typed.calls.insert(Key(node), call);
        self.raises_library_errors(node, frame, &["OSError", "ValueError"])
    }

    /// Whether `read_csv` reads a column whose elements are `element`s: an `f64`, an `i64`, a
    /// `bool` or a `str`, or one of them or `None`.
    fn is_csv_element(&self, element: Type<'db>) -> bool {
        let element = match element.kind(self.db) {
            TypeKind::Optional(value) => *value,
            _ => element,
        };
        matches!(
            element.kind(self.db),
            TypeKind::Str
                | TypeKind::Primitive(
                    Primitive::Bool
                        | Primitive::Float(Float::F64)
                        | Primitive::Integer(Integer::I64)
                )
        )
    }
}
