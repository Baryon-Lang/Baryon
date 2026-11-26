//! `DataFrame.read_parquet(path)` (decision 0045, roadmap 3.14.6.3): a Parquet file read into
//! a frame.
//!
//! A Parquet file carries its own schema, but a compiled program must know the frame's
//! columns before it runs, so, as with `read_csv`, the frame's type comes from the context:
//! an annotation, a parameter or a return type that names `DataFrame[Trade]`. Each field is
//! read from the file's column of its name, and the field's type says what that column may
//! hold. The call can fail with `OSError`, when the file cannot be read or a page cannot be
//! decompressed, and with `ValueError`, when the file is not Parquet, lacks a column, or
//! holds something a column's type does not read.
//!
//! It takes the path alone: every keyword pandas has is an error that names it, never
//! silently ignored.
//!
//! `frame.to_parquet(path)` (roadmap 3.14.6.4) writes a frame as such a file, compressed
//! with Snappy. It gives nothing, takes the path alone too, and can fail with `OSError`.

use super::Body;
use super::calls::Arguments;
use syntax::NodeId;
use types::{Call, Callee, Key, Type, TypeKind};

/// How a call is written, for the hints.
const EXAMPLE: &str =
    "for example `trades: DataFrame[Trade] = DataFrame.read_parquet(\"trades.parquet\")`";

impl<'db> Body<'db> {
    /// The call `node` of `DataFrame.read_parquet(...)`: the frame type `expected` names,
    /// in a `Raises` of `OSError` and `ValueError`, after checking the path and that every
    /// column of that type is one the reader has, a number that is not complex, a `bool` or
    /// a `str`, or one of them or `None`.
    pub(super) fn read_parquet(
        &mut self,
        node: NodeId,
        arguments: &Arguments,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        if let Some(&(keyword, _)) = arguments.keywords.first() {
            let name = self.tree.text(keyword, self.text);
            let message = format!("`DataFrame.read_parquet` does not take `{name}` yet");
            self.untyped(arguments);
            return self.lowerer.error(
                keyword,
                &message,
                "not built yet",
                Some("this version takes the path alone, and the schema chooses the columns"),
            );
        }
        let &[path] = arguments.positional.as_slice() else {
            self.untyped(arguments);
            return self.lowerer.error(
                node,
                "`DataFrame.read_parquet` takes the path of the file",
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
                "`DataFrame.read_parquet` needs the frame's type from the context",
                "which columns, of which types?",
                Some(
                    "a compiled program must know the columns before it reads the file: \
                     write the schema as a struct and annotate the variable, \
                     `trades: DataFrame[Trade] = DataFrame.read_parquet(path)`",
                ),
            );
        };
        let TypeKind::DataFrame { columns, .. } = frame.kind(self.db) else {
            return self.lowerer.unknown();
        };
        for column in columns {
            if !self.is_csv_element(column.element) {
                let message = format!(
                    "`DataFrame.read_parquet` cannot read the column `{}` of type `{}` yet",
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
            callee: Callee::Builtin("DataFrame.read_parquet".to_owned()),
            type_arguments: Vec::new(),
            arguments: vec![Some(Key(path))],
        };
        self.typed.calls.insert(Key(node), call);
        self.raises_library_errors(node, frame, &["OSError", "ValueError"])
    }

    /// The call `node` of `to_parquet` on a frame of type `base`: nothing, after checking
    /// the path and that every column is of a type the writer has, a number that is not
    /// complex, a `bool` or a `str`, or one of them or `None`. `None`, with nothing checked,
    /// for any other method.
    pub(super) fn parquet_method(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        callee: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        if callee != "to_parquet" {
            return None;
        }
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return None;
        };
        if let Some(&(keyword, _)) = arguments.keywords.first() {
            let name = self.tree.text(keyword, self.text);
            let message = format!("`to_parquet` does not take `{name}` yet");
            self.untyped(arguments);
            return Some(self.lowerer.error(
                keyword,
                &message,
                "not built yet",
                Some("this version takes the path alone, and compresses with Snappy"),
            ));
        }
        let &[path] = arguments.positional.as_slice() else {
            self.untyped(arguments);
            return Some(self.lowerer.error(
                node,
                "`to_parquet` takes the path of the file to write",
                "wrong arguments",
                Some("for example `trades.to_parquet(\"trades.parquet\")`"),
            ));
        };
        let str = Type::new(self.db, TypeKind::Str);
        let found = self.expression(path, Some(str));
        self.expect(path, str, found, "a path is");
        for column in columns {
            if !self.is_csv_element(column.element) {
                let message = format!(
                    "`to_parquet` cannot write the column `{}` of type `{}` yet",
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
