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
//! Each takes the path and `sep=`, the character between fields; `read_csv` takes
//! `na_values=`, more cells to read as missing, `nrows=` and `skiprows=`, and `to_csv` takes
//! `na_rep=`, the text to write for a missing value (roadmap 3.14.4.2). Every other keyword
//! pandas has is an error that names it, never silently ignored.

use super::calls::Arguments;
use super::{Body, relations};
use syntax::NodeId;
use types::{Call, Callee, Integer, Key, Primitive, Type, TypeKind};

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
        if let Err(refused) = self.csv_keywords(true, arguments) {
            return refused;
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

    /// Checks the keyword arguments of `read_csv`, when `reading`, or of `to_csv` (roadmap
    /// 3.14.4.2). Five are built. `sep=`, on both, is the character between fields.
    /// `na_values=` of `read_csv` is a `str` or a `list[str]`: the cells read as missing
    /// besides the empty one. Its `nrows=` and `skiprows=` are each an `int`: how many
    /// rows to read at most, and how many lines to pass over before the header. `na_rep=`
    /// of `to_csv` is the `str` written for a missing value. `Err`, with the unknown type
    /// and every argument marked untyped, after reporting a keyword that is not built yet
    /// or a separator that is not one character.
    fn csv_keywords(&mut self, reading: bool, arguments: &Arguments) -> Result<(), Type<'db>> {
        let function = if reading {
            "`DataFrame.read_csv`"
        } else {
            "`to_csv`"
        };
        let str = Type::new(self.db, TypeKind::Str);
        for &(keyword, value) in &arguments.keywords {
            match (self.tree.text(keyword, self.text), reading) {
                ("sep", _) => self.csv_separator(function, value, arguments)?,
                ("na_values", true) => {
                    let found = self.expression(value, None);
                    let list = Type::new(self.db, TypeKind::List(str));
                    if !relations::compatible(self.db, list, found) {
                        self.expect(value, str, found, "`na_values` is a `list[str]` or a");
                    }
                }
                (name @ ("nrows" | "skiprows"), true) => {
                    let int = TypeKind::Primitive(Primitive::Integer(Integer::I64));
                    let int = Type::new(self.db, int);
                    let found = self.expression(value, Some(int));
                    self.expect(value, int, found, &format!("`{name}` is"));
                }
                ("na_rep", false) => {
                    let found = self.expression(value, Some(str));
                    self.expect(value, str, found, "`na_rep` is");
                }
                (name, _) => {
                    let message = format!("{function} does not take `{name}` yet");
                    self.untyped(arguments);
                    let hint = if reading {
                        "this version takes the path, `sep=`, `na_values=`, `nrows=` and \
                         `skiprows=`, for a file with a header row"
                    } else {
                        "this version takes the path, `sep=` and `na_rep=`, and writes a \
                         header row and no index column"
                    };
                    return Err(self
                        .lowerer
                        .error(keyword, &message, "not built yet", Some(hint)));
                }
            }
        }
        Ok(())
    }

    /// Checks `value`, the `sep=` of the call of `function` whose arguments are `arguments`:
    /// a string literal of one character, since a file split by a text the program works
    /// out could not be checked here, and `to_csv` would gain a way to fail. `Err`, as
    /// `csv_keywords` gives it, after reporting a separator that is not one character.
    fn csv_separator(
        &mut self,
        function: &str,
        value: NodeId,
        arguments: &Arguments,
    ) -> Result<(), Type<'db>> {
        let written = self.literal_text(value);
        let one = written.is_some_and(|text| {
            text == "\\t"
                || (text.len() == 1 && !matches!(text, "\\" | "\"" | "'") && text.is_ascii())
        });
        if !one {
            self.untyped(arguments);
            let message = format!("`sep` of {function} is one character, written out");
            let hint = "for example `sep=\";\"` or `sep=\"\\t\"`; a quote, a backslash and a \
                        character outside ASCII cannot split fields";
            return Err(self
                .lowerer
                .error(value, &message, "not one character", Some(hint)));
        }
        let str = Type::new(self.db, TypeKind::Str);
        self.expression(value, Some(str));
        Ok(())
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
        if let Err(refused) = self.csv_keywords(false, arguments) {
            return Some(refused);
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
