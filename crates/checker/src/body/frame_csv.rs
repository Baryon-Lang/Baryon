//! CSV files read into a frame and written from one (decision 0045, roadmap 3.14.4):
//! `DataFrame.read_csv(path)` and `df.to_csv(path)`.
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
//! column of floats, of whole numbers, of `bool` or of `str` is read, and one of those that
//! may be `None`, where an empty cell is `None`. The call can fail with `OSError`, when the
//! file cannot be read, and with `ValueError`, when the header lacks a column or a cell is
//! not a value of its column's type.
//!
//! `df.to_csv(path)` writes a frame as such a file, the bytes
//! `pandas.DataFrame.to_csv(path, index=False)` writes: a frame has no row index (decision
//! 0045), so no index column is written. It can fail with `OSError`.
//!
//! The first version of each takes the path alone: every keyword pandas has is an error that
//! names it, never silently ignored.

use super::Body;
use super::calls::Arguments;
use syntax::NodeId;
use types::{Call, Callee, Key, Primitive, Type, TypeKind};

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
                        "a column of floats, of whole numbers, of `bool` or of `str` is read, \
                         and one of those that may be `None`",
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

    /// Whether `read_csv` reads, and `to_csv` writes, a column whose elements are `element`s:
    /// a float, a whole number, a `bool` or a `str`, or one of them or `None`.
    fn is_csv_element(&self, element: Type<'db>) -> bool {
        let element = match element.kind(self.db) {
            TypeKind::Optional(value) => *value,
            _ => element,
        };
        matches!(
            element.kind(self.db),
            TypeKind::Str
                | TypeKind::Primitive(
                    Primitive::Bool | Primitive::Float(_) | Primitive::Integer(_)
                )
        )
    }

    /// The call `node` of `to_csv` on a frame of type `base`: nothing, after checking the
    /// path and that every column is of a type the writer has, a number that is not
    /// complex, a `bool` or a `str`, or one of them or `None`. `None`, with nothing
    /// checked, for any other method.
    pub(super) fn csv_method(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        callee: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        if callee != "to_csv" {
            return None;
        }
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return None;
        };
        if let Some(&(keyword, _)) = arguments.keywords.first() {
            self.untyped(arguments);
            let message = format!(
                "`to_csv` does not take `{}` yet",
                self.tree.text(keyword, self.text)
            );
            let hint = "this version writes a comma-separated file with a header row and no \
                        index column, given its path alone";
            return Some(
                self.lowerer
                    .error(keyword, &message, "not built yet", Some(hint)),
            );
        }
        let &[path] = arguments.positional.as_slice() else {
            self.untyped(arguments);
            return Some(self.lowerer.error(
                node,
                "`to_csv` takes the path of the file to write",
                "wrong arguments",
                Some("for example `trades.to_csv(\"trades.csv\")`"),
            ));
        };
        let str = Type::new(self.db, TypeKind::Str);
        let found = self.expression(path, Some(str));
        self.expect(path, str, found, "a path is");
        for column in columns {
            if !self.is_csv_element(column.element) {
                let message = format!(
                    "`to_csv` cannot write the column `{}` of type `{}` yet",
                    column.name,
                    self.show(column.element)
                );
                return Some(self.lowerer.error(
                    node,
                    &message,
                    "not built yet",
                    Some(
                        "a column of floats, of whole numbers, of `bool` or of `str` is \
                         written, and one of those that may be `None`",
                    ),
                ));
            }
        }
        let nothing = Type::new(self.db, TypeKind::None);
        Some(self.raises_library_error(node, nothing, "OSError"))
    }
}
