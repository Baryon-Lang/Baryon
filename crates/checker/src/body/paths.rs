//! Paths and directories (decision 0036, parts 1 and 7): `pathlib.Path` and the directory
//! functions of `os`.
//!
//! `Path` is a class of the standard library's own source (decision 0038): a struct that holds
//! the path's text. What it does is built into the compiler, so it is checked here rather than
//! against declared signatures: `Path(text)`, `path / other`, the parts `name`, `stem`, `suffix`
//! and `parent`, and the methods `exists`, `is_file`, `is_dir`, `iterdir`, `read_text` and
//! `write_text`. Everything that takes a path takes a `str` or a `Path`, as in Python
//! ([`Body::path_like`]), and everything that can fail has the type `Raises[T, OSError]`.

use super::Body;
use super::calls::Arguments;
use database::{Definition, File};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Call, Callee, Integer, Key, Primitive, Type, TypeKind};

/// What a `Path` has, for the hint of an error about something it does not have.
const HAS: &str = "a `Path` has `name`, `stem`, `suffix` and `parent`, and the methods `exists`, \
                   `is_file`, `is_dir`, `iterdir`, `read_text`, `write_text` and `copy` (decision 0036)";

/// The methods of a `Path`, which are called: `path.exists()`.
const METHODS: [&str; 7] = [
    "copy",
    "exists",
    "is_file",
    "is_dir",
    "iterdir",
    "read_text",
    "write_text",
];

impl<'db> Body<'db> {
    /// The type `pathlib.Path`, when the package has the library's `pathlib` file, as every
    /// package the driver builds does.
    pub(super) fn path_type(&self) -> Option<Type<'db>> {
        self.library_class_in("pathlib", "Path")
    }

    /// Whether the class named by the node `name` of `file` is `pathlib.Path`, so a call of it
    /// is `Path(text)`.
    pub(super) fn is_path_class(&self, file: File, name: NodeId) -> bool {
        self.path_type().is_some_and(|path| {
            matches!(path.kind(self.db), TypeKind::Struct(nominal) if nominal.file == file && nominal.name == name)
        })
    }

    /// Whether `ty` is `pathlib.Path`.
    pub(super) fn is_path(&self, ty: Type<'db>) -> bool {
        self.path_type() == Some(ty)
    }

    /// Reports `node`, a value of type `ty` that `print` or an f-string shows, when it holds
    /// a `Path` inside a list, a dict, a tuple, an optional or a struct. Python shows such a
    /// path as `PosixPath('runs')`; a struct's text here would show the field the compiler
    /// keeps the path in, so it is refused until that text is built. A `Path` on its own
    /// prints as its text.
    pub(super) fn shows_no_path(&mut self, node: NodeId, ty: Type<'db>) {
        if self.is_path(ty) || !self.holds_path(ty, 0) {
            return;
        }
        let message = format!("a `Path` inside `{}` cannot be shown yet", self.show(ty));
        self.lowerer.error(
            node,
            &message,
            "holds a `Path`",
            Some("show each path on its own, `print(path)`, or keep its text: `str(path)`"),
        );
    }

    /// Whether a value of type `ty` is or holds a `Path`. `depth` counts the types entered, and
    /// stops a struct that holds a list of itself.
    fn holds_path(&self, ty: Type<'db>, depth: u8) -> bool {
        if self.is_path(ty) {
            return true;
        }
        if depth > 8 {
            return false;
        }
        let inside = |ty: &Type<'db>| self.holds_path(*ty, depth + 1);
        match ty.kind(self.db) {
            TypeKind::List(element) | TypeKind::Optional(element) => inside(element),
            TypeKind::Dict { key, value } => inside(key) || inside(value),
            TypeKind::Tuple(items) | TypeKind::Union(items) => items.iter().any(inside),
            TypeKind::Struct(nominal) => {
                crate::embed::fields(self.db, self.lowerer.package, nominal)
                    .iter()
                    .any(|(_, field)| inside(field))
            }
            _ => false,
        }
    }

    /// Checks that `node` is a path: a `str` or a `Path`, as every function that takes a path
    /// accepts (decision 0036, part 7).
    pub(super) fn path_like(&mut self, node: NodeId) {
        let str = Type::new(self.db, TypeKind::Str);
        let found = self.expression(node, Some(str));
        if !self.is_path(found) {
            self.expect(node, str, found, "a path is a `Path` or a");
        }
    }

    /// `Path(text)`: one path, a `str` or another `Path`.
    pub(super) fn path_new(&mut self, node: NodeId, arguments: &Arguments) -> Type<'db> {
        let (&[text], true) = (&arguments.positional[..], arguments.keywords.is_empty()) else {
            self.untyped(arguments);
            return self.lowerer.error(
                node,
                "`Path` takes one path",
                "wrong arguments",
                Some("join more with `/`: `Path(\"runs\") / name / \"out.csv\"`"),
            );
        };
        self.path_like(text);
        self.record_path(node, "pathlib.Path", &[text]);
        self.path_type().unwrap_or_else(|| self.lowerer.unknown())
    }

    /// `left / right` where either side is a `Path`: the other is a `Path` or a `str`, and the
    /// result is a `Path`. Returns `None` when neither side is a `Path`.
    pub(super) fn path_join(
        &mut self,
        node: NodeId,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Option<Type<'db>> {
        if !self.is_path(left) && !self.is_path(right) {
            return None;
        }
        for side in [left, right] {
            let joins = self.is_path(side)
                || matches!(side.kind(self.db), TypeKind::Str | TypeKind::Unknown);
            if !joins {
                let message = format!(
                    "`/` joins a `Path` with a `Path` or a `str`, found `{}`",
                    self.show(side)
                );
                let hint = "write the name as text, such as `path / f\"step_{i}.csv\"`";
                return Some(self.lowerer.error(node, &message, "not a path", Some(hint)));
            }
        }
        self.path_type()
    }

    /// Whether `node` is `path.name` or another part of a `Path`: an attribute of a value the
    /// checker gave the type `Path`. A part is computed from the path's text, so it is a new
    /// value, not a place inside the path.
    ///
    /// The value's type is the one the checker recorded, or, before it has typed the value, the
    /// type of the variable it names, so an assignment can be checked before its target is.
    pub(super) fn is_path_part(&self, node: NodeId) -> bool {
        if self.tree.kind(node) != SyntaxKind::Attribute {
            return false;
        }
        let Some(value) = self.tree.child(node, Role::Value) else {
            return false;
        };
        let recorded = self.typed.expressions.get(&Key(value)).copied();
        let local = || match self.lowerer.definition(value) {
            Some(&Definition::Local(binding)) => self.locals.get(&binding).copied(),
            _ => None,
        };
        recorded.or_else(local).is_some_and(|ty| self.is_path(ty))
    }

    /// Reports the place `node` when it is a part of a `Path`, as in `path.name = "out.csv"`,
    /// and returns whether it did: a part is computed from the path, so there is nothing to
    /// assign to, and Python's `Path` refuses the same.
    pub(super) fn path_part_changed(&mut self, node: NodeId) -> bool {
        let part = self.is_path_part(node);
        if part {
            self.lowerer.error(
                node,
                "a part of a `Path` cannot be assigned",
                "computed from the path",
                Some("make a new path instead, such as `path.parent / \"out.csv\"`"),
            );
        }
        part
    }

    /// `path.name`, `path.stem` and `path.suffix`, each a `str`, and `path.parent`, a `Path`.
    /// Anything else is reported, the struct's own field included: it is the compiler's.
    pub(super) fn path_attribute(&mut self, node: NodeId, name: &str) -> Type<'db> {
        match name {
            "name" | "stem" | "suffix" => Type::new(self.db, TypeKind::Str),
            "parent" => self.path_type().unwrap_or_else(|| self.lowerer.unknown()),
            _ if METHODS.contains(&name) => {
                let message = format!("`Path.{name}` is a method");
                let hint = format!("call it: `path.{name}()`");
                self.lowerer
                    .error(node, &message, "not called", Some(&hint))
            }
            _ => {
                let message = format!("`Path` has no attribute `{name}`");
                self.lowerer.error(node, &message, "not defined", Some(HAS))
            }
        }
    }

    /// `path.method(arguments)` on the `Path` `value`: `exists()`, `is_file()` and `is_dir()`
    /// give a `bool`; `iterdir()` a `list[Path]`, `read_text()` a `str` and `write_text(text)`
    /// the number of characters written, each of which can fail with `OSError`; `copy()` gives
    /// a new `Path` with the same text. Returns `None` only when the library has no `Path`.
    pub(super) fn path_method(
        &mut self,
        node: NodeId,
        value: NodeId,
        method: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let str = Type::new(self.db, TypeKind::Str);
        // `read_text` and `write_text` take `encoding="utf-8"`, as `open` does.
        let reads_text = matches!(method, "read_text" | "write_text");
        let mut wrong = false;
        for &(name, encoding) in &arguments.keywords {
            if reads_text && self.tree.text(name, self.text) == "encoding" {
                self.encoding(encoding);
            } else {
                wrong = true;
            }
        }
        let positional = arguments.positional.as_slice();
        let (takes, result) = match method {
            "exists" | "is_file" | "is_dir" => {
                (0, Type::new(self.db, TypeKind::Primitive(Primitive::Bool)))
            }
            "iterdir" => {
                let path = self.path_type()?;
                (0, Type::new(self.db, TypeKind::List(path)))
            }
            "copy" => (0, self.path_type()?),
            "read_text" => (0, str),
            "write_text" => {
                let int = TypeKind::Primitive(Primitive::Integer(Integer::I64));
                (1, Type::new(self.db, int))
            }
            _ => {
                self.untyped(arguments);
                let message = format!("`Path` has no method `{method}`");
                return Some(self.lowerer.error(node, &message, "not defined", Some(HAS)));
            }
        };
        if wrong || positional.len() != takes {
            self.untyped(arguments);
            let (message, hint) = if takes == 0 {
                (format!("`{method}` takes no arguments"), None)
            } else {
                let hint = "for example `path.write_text(f\"{energy}\\n\")`";
                ("`write_text` takes one `str`".to_owned(), Some(hint))
            };
            return Some(self.lowerer.error(node, &message, "wrong arguments", hint));
        }
        for &text in positional {
            let found = self.expression(text, Some(str));
            self.expect(text, str, found, "`write_text` takes a");
        }
        let mut passed = vec![value];
        passed.extend_from_slice(positional);
        self.record_path(node, &format!("Path.{method}"), &passed);
        Some(
            if matches!(method, "exists" | "is_file" | "is_dir" | "copy") {
                result
            } else {
                self.raises_os_error(node, result)
            },
        )
    }

    /// `os.member(arguments)` for a directory function of `os`: `listdir(path)`, with `.` when
    /// the path is left out, `remove(path)`, `rename(source, destination)`, `mkdir(path)`,
    /// `makedirs(path, exist_ok=False)` and `getcwd()`. Each can fail with `OSError`. Returns
    /// `None` for any other member.
    pub(super) fn directory_call(
        &mut self,
        node: NodeId,
        member: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let str = Type::new(self.db, TypeKind::Str);
        let nothing = Type::new(self.db, TypeKind::None);
        let (paths, result, example) = match member {
            "listdir" => (
                0..=1,
                Type::new(self.db, TypeKind::List(str)),
                "os.listdir(\"runs\")",
            ),
            "remove" => (1..=1, nothing, "os.remove(\"out.tmp\")"),
            "rename" => (2..=2, nothing, "os.rename(\"out.tmp\", \"out.csv\")"),
            "mkdir" => (1..=1, nothing, "os.mkdir(\"runs\")"),
            "makedirs" => (1..=1, nothing, "os.makedirs(\"runs/2026\", exist_ok=True)"),
            "getcwd" => (0..=0, str, "os.getcwd()"),
            _ => return None,
        };
        let mut wrong = !paths.contains(&arguments.positional.len());
        for &(name, value) in &arguments.keywords {
            if member == "makedirs" && self.tree.text(name, self.text) == "exist_ok" {
                let bool = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
                let found = self.expression(value, Some(bool));
                self.expect(value, bool, found, "`exist_ok` is");
            } else {
                wrong = true;
            }
        }
        if wrong {
            self.untyped(arguments);
            let message = format!("`os.{member}` does not take these arguments");
            let hint = format!("for example `{example}`");
            return Some(
                self.lowerer
                    .error(node, &message, "wrong arguments", Some(&hint)),
            );
        }
        for &path in &arguments.positional {
            self.path_like(path);
        }
        Some(self.raises_os_error(node, result))
    }

    /// Records the typed call `node` of a `Path` built-in named `name`, with `arguments` in
    /// order, the receiver first for a method.
    fn record_path(&mut self, node: NodeId, name: &str, arguments: &[NodeId]) {
        let call = Call {
            callee: Callee::Builtin(name.to_owned()),
            type_arguments: Vec::new(),
            arguments: arguments
                .iter()
                .map(|&argument| Some(Key(argument)))
                .collect(),
        };
        self.typed.calls.insert(Key(node), call);
    }
}
