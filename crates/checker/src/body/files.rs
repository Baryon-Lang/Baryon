//! Text files (decision 0036, parts 3 and 4): `open(path, mode)`, the methods of the `File` it
//! returns, `with open(...) as f:` and `for line in f:`.
//!
//! Every one of them can fail with `OSError`, the class the standard library's own source
//! defines (decision 0038), so each has the type `Raises[T, OSError]` and its error is handled
//! as any call's is. A method that reads or writes changes the file, as `append` changes a list,
//! so the file must be a variable of the function or an `InOut` parameter; `close()` moves it,
//! so it cannot be used after.

use super::Body;
use super::calls::Arguments;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Call, Callee, Integer, Key, Primitive, StandardType, Type, TypeKind};

/// The modes `open` takes, each with the number the `mir` call passes: read, write, append and
/// create (decision 0036, part 4).
pub(crate) const MODES: [(&str, i128); 4] = [("r", 0), ("w", 1), ("a", 2), ("x", 3)];

/// The text of a plain string literal, without its quotes: `r` for `"r"`. `None` for anything
/// else, such as an f-string or a name.
pub(crate) fn literal(tree: &syntax::SyntaxTree, text: &str, node: NodeId) -> Option<String> {
    if tree.kind(node) != SyntaxKind::StringLiteral {
        return None;
    }
    let written = tree.text(node, text);
    ['"', '\'']
        .into_iter()
        .find_map(|quote| written.strip_prefix(quote)?.strip_suffix(quote))
        .filter(|inside| !inside.contains(['"', '\'', '\\']))
        .map(str::to_owned)
}

impl<'db> Body<'db> {
    /// `Raises[value, OSError]`.
    pub(super) fn raises_os_error(&mut self, node: NodeId, value: Type<'db>) -> Type<'db> {
        self.raises_library_error(node, value, "OSError")
    }

    /// `open(path)` or `open(path, mode)`, the path a `str` or a `Path`, the mode positional or by keyword, with an optional
    /// `encoding="utf-8"`. The mode is a literal: `"r"`, `"w"`, `"a"` or `"x"`.
    pub(super) fn open(&mut self, node: NodeId, arguments: &Arguments) -> Type<'db> {
        let mut mode = arguments.positional.get(1).copied();
        let mut wrong = arguments.positional.len() > 2 || arguments.positional.is_empty();
        for &(name, value) in &arguments.keywords {
            match self.tree.text(name, self.text) {
                "mode" if mode.is_none() => mode = Some(value),
                "encoding" => self.encoding(value),
                _ => wrong = true,
            }
        }
        if wrong {
            self.untyped(arguments);
            return self.lowerer.error(
                node,
                "`open` takes a path and an optional mode",
                "wrong arguments",
                Some("for example `open(\"data.txt\")` or `open(\"out.txt\", \"w\")`"),
            );
        }
        self.path_like(arguments.positional[0]);
        if let Some(mode) = mode {
            self.mode(mode);
        }
        let file = Type::new(self.db, TypeKind::Standard(StandardType::File));
        self.raises_os_error(node, file)
    }

    /// Records a typed `open` call with its path and, when it is given, its mode; `encoding`
    /// adds nothing to build.
    pub(super) fn record_open(&mut self, node: NodeId, arguments: &Arguments, ty: Type<'db>) {
        if matches!(ty.kind(self.db), TypeKind::Unknown) {
            return;
        }
        let mode = arguments.positional.get(1).copied().or_else(|| {
            arguments
                .keywords
                .iter()
                .find(|&&(name, _)| self.tree.text(name, self.text) == "mode")
                .map(|&(_, value)| value)
        });
        let call = Call {
            callee: Callee::Builtin("open".to_owned()),
            type_arguments: Vec::new(),
            arguments: arguments
                .positional
                .first()
                .copied()
                .into_iter()
                .chain(mode)
                .map(|argument| Some(Key(argument)))
                .collect(),
        };
        self.typed.calls.insert(Key(node), call);
    }

    /// Checks `open`'s mode: a literal, one of [`MODES`].
    fn mode(&mut self, mode: NodeId) {
        let str = Type::new(self.db, TypeKind::Str);
        let found = self.expression(mode, Some(str));
        self.expect(mode, str, found, "a mode is");
        let Some(written) = literal(self.tree, self.text, mode) else {
            if !matches!(found.kind(self.db), TypeKind::Unknown) {
                self.lowerer.error(
                    mode,
                    "`open`'s mode must be written out",
                    "not a literal",
                    Some("write `\"r\"`, `\"w\"`, `\"a\"` or `\"x\"`"),
                );
            }
            return;
        };
        if MODES.iter().any(|&(known, _)| known == written) {
            return;
        }
        let note = if written.contains('b') {
            "binary data is whole arrays: `Array.fromfile(path, f64)` and `a.tofile(path)` \
             (decision 0036)"
        } else if written.contains('+') {
            "a file is opened to read or to write, not both"
        } else {
            "the modes are `\"r\"`, `\"w\"`, `\"a\"` and `\"x\"`"
        };
        let message = format!("`open` has no mode `\"{written}\"`");
        self.lowerer
            .error(mode, &message, "unknown mode", Some(note));
    }

    /// Checks `encoding=`: text files are UTF-8, so only `"utf-8"` is accepted.
    pub(super) fn encoding(&mut self, encoding: NodeId) {
        let str = Type::new(self.db, TypeKind::Str);
        let found = self.expression(encoding, Some(str));
        self.expect(encoding, str, found, "an encoding is");
        let utf8 = literal(self.tree, self.text, encoding)
            .is_some_and(|written| matches!(written.to_lowercase().as_str(), "utf-8" | "utf8"));
        if !utf8 && !matches!(found.kind(self.db), TypeKind::Unknown) {
            self.lowerer.error(
                encoding,
                "text files are UTF-8",
                "not UTF-8",
                Some("leave `encoding` out, or write `encoding=\"utf-8\"`"),
            );
        }
    }

    /// `file.method(arguments)` on a `File`: `read()`, `readline()` and `readlines()` give text,
    /// `write(text)` gives the number of characters, `flush()` and `close()` give `None`.
    pub(super) fn file_method(
        &mut self,
        node: NodeId,
        method: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        let str = Type::new(self.db, TypeKind::Str);
        let nothing = Type::new(self.db, TypeKind::None);
        let returns = match method {
            "read" | "readline" => Some(str),
            "readlines" => Some(Type::new(self.db, TypeKind::List(str))),
            "flush" | "close" => Some(nothing),
            "write" => None,
            _ => {
                self.untyped(arguments);
                let message = format!("`File` has no method `{method}`");
                return self.lowerer.error(
                    node,
                    &message,
                    "not defined",
                    Some(
                        "a `File` has `read`, `readline`, `readlines`, `write`, `flush` and \
                         `close`",
                    ),
                );
            }
        };
        let given = arguments.positional.len() + arguments.keywords.len();
        let value = match returns {
            Some(value) if given == 0 => value,
            Some(_) => {
                self.untyped(arguments);
                let message = format!("`{method}` takes no arguments");
                return self.lowerer.error(node, &message, "wrong arguments", None);
            }
            None if given == 1 && arguments.keywords.is_empty() => {
                let text = arguments.positional[0];
                let found = self.expression(text, Some(str));
                self.expect(text, str, found, "`write` takes a");
                Type::new(
                    self.db,
                    TypeKind::Primitive(Primitive::Integer(Integer::I64)),
                )
            }
            None => {
                self.untyped(arguments);
                return self.lowerer.error(
                    node,
                    "`write` takes one `str`",
                    "wrong arguments",
                    Some("build the text first: `out.write(f\"{t} {energy}\\n\")`"),
                );
            }
        };
        self.raises_os_error(node, value)
    }

    /// `for line in file:` or a comprehension's `for line in file`, where `statement` is the
    /// loop or the clause and `iterator` what it goes over. When `iterator` is a `File`, each
    /// pass reads the next line as `readline()` does, so the loop changes the file and can fail
    /// with `OSError`, recorded at `statement`. Returns whether `iterator` is a `File`.
    pub(super) fn file_loop(&mut self, statement: NodeId, iterator: NodeId) -> bool {
        let is_file = matches!(
            self.typed.expression(iterator).map(|ty| ty.kind(self.db)),
            Some(TypeKind::Standard(StandardType::File))
        );
        if is_file {
            self.change(iterator);
            let none = Type::new(self.db, TypeKind::None);
            let read = self.raises_os_error(statement, none);
            self.fallible_at(statement, read, "reading a line of the file can fail with");
        }
        is_file
    }

    /// Records the receiver `value`, of the built-in type `base`, as used by the method `method`:
    /// moved by a file's `close()`, which ends the file, and changed by every other method, as
    /// reading or writing changes a file and drawing a number changes a generator.
    pub(super) fn used_receiver(&mut self, value: NodeId, base: Type<'db>, method: &str) {
        if base.kind(self.db) == &TypeKind::Standard(StandardType::File) && method == "close" {
            self.consume(value, base);
        } else {
            self.change(value);
        }
    }

    /// `with value as name:`, where `value` is a `File`, such as `open(path, "w")`: binds `name`
    /// to the file for the body and closes it at the end, which can fail with `OSError` as
    /// `close()` can. A `NetCDF` `Dataset` binds too, and closing it does nothing, as
    /// `ds.close()` does nothing (decision 0047). Returns the type of `value`, `None` when it is
    /// neither, which is reported.
    pub(super) fn with_file(&mut self, statement: NodeId, item: NodeId) -> Option<Type<'db>> {
        let value = self.tree.child(item, Role::Value)?;
        let ty = self.expression(value, None);
        if self.is_netcdf(ty, "Dataset") {
            return Some(ty);
        }
        let is_file = matches!(ty.kind(self.db), TypeKind::Standard(StandardType::File));
        if !is_file && !matches!(ty.kind(self.db), TypeKind::Unknown) {
            let message = format!("`with` needs a `File`, found `{}`", self.show(ty));
            self.lowerer.error(
                value,
                &message,
                "not a file",
                Some("`with` is for `with open(...) as f:` and `with unsafe:` (decision 0036)"),
            );
            return None;
        }
        self.consume(value, ty);
        let none = Type::new(self.db, TypeKind::None);
        let closed = self.raises_os_error(statement, none);
        self.fallible_at(
            statement,
            closed,
            "closing the file at the end of `with` can fail with",
        );
        Some(ty)
    }
}
