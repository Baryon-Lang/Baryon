//! Paths and directories (decision 0036, parts 1 and 7): `pathlib.Path` and the directory
//! functions of `os`, each a call of a [`Builtin::Path`].
//!
//! A `Path` is a struct of the standard library's own source that holds one `str`, the path's
//! text (decision 0038). So every operation here passes that field, or a `str` as it is, to the
//! runtime ([`Builder::path_argument`]), and a result that is a `Path` is the new text wrapped
//! in the struct:
//!
//! - `Path(text)`, `path / other` and `path.parent` make a new `Path`; `path.name`, `path.stem`
//!   and `path.suffix` a new `str`. None can fail.
//! - `path.exists()`, `path.is_file()` and `path.is_dir()` give a `bool` and cannot fail.
//! - `path.iterdir()` and `os.listdir`, `os.remove`, `os.rename`, `os.mkdir`, `os.makedirs` and
//!   `os.getcwd` can fail with `OSError`, so each call has an error edge (the `errors` module).
//! - `path.read_text()` and `path.write_text(text)` are `open`, a read or a write, and a close:
//!   the calls `with open(path) as f:` makes, so they need no built-in of their own.
//! - Where text is wanted, in `print`, in an f-string and in `str(path)`, a `Path` gives its
//!   text ([`Builder::text_place`]), as Python's `str(path)` does.

use crate::builder::Builder;
use mir::{Argument, Builtin, Constant, ConstantValue, FileMethod, Operand, PathMethod};
use mir::{Piece, Place, Projection, Rvalue};
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::{FormatSpec, Primitive, StandardType, Type, TypeKind};

/// The path of the standard library's file that defines `Path`.
const PATHLIB: &str = "<std>/pathlib.byn";

impl<'db> Builder<'db> {
    /// Whether `ty` is `pathlib.Path`: the struct `Path` of the library's `pathlib` file.
    pub(crate) fn is_path(&self, ty: Type<'db>) -> bool {
        let TypeKind::Struct(nominal) = ty.kind(self.db) else {
            return false;
        };
        let source = nominal.file.source(self.db);
        source.name() == PATHLIB
            && self
                .db
                .parse(nominal.file)
                .tree()
                .text(nominal.name, source.text())
                == "Path"
    }

    /// The place to read as text where `place`, holding a value of type `ty`, is printed or
    /// formatted: a `Path`'s text, and any other value as it is.
    pub(crate) fn text_place(&self, place: Place, ty: Type<'db>) -> Place {
        if self.is_path(ty) {
            place.project(Projection::Field(0))
        } else {
            place
        }
    }

    /// A path argument, a `str` or a `Path`, as the text a [`Builtin::Path`] or `open` takes: a
    /// literal passed `Owned`, a `str` borrowed, and a `Path`'s text borrowed from it.
    pub(crate) fn path_argument(&mut self, path: NodeId) -> Argument<'db> {
        if self.tree.kind(path) == SyntaxKind::StringLiteral {
            return Argument::Owned(self.operand(path));
        }
        let ty = self.ty(path);
        let place = self.borrow(path);
        Argument::Borrow(self.text_place(place, ty))
    }

    /// Whether `node` is `path.name`, `path.stem`, `path.suffix` or `path.parent`: an attribute
    /// of a `Path`, which names no place, since each is computed.
    pub(crate) fn is_path_part(&self, node: NodeId) -> bool {
        self.tree.kind(node) == SyntaxKind::Attribute
            && self
                .tree
                .child(node, Role::Value)
                .is_some_and(|value| self.is_path(self.ty(value)))
    }

    /// Whether `node` is `left / right` joining paths, which the checker gave the type `Path`.
    pub(crate) fn is_path_join(&self, node: NodeId) -> bool {
        self.tree.kind(node) == SyntaxKind::BinaryOperation
            && self.tree.node(node).operator() == Operator::Divide
            && self.is_path(self.ty(node))
    }

    /// `path.name`, `path.stem`, `path.suffix` or `path.parent` into `place`.
    pub(crate) fn path_part(&mut self, place: Place, node: NodeId) {
        let (Some(value), Some(attribute)) = (
            self.tree.child(node, Role::Value),
            self.tree.child(node, Role::Attribute),
        ) else {
            return;
        };
        let method = match self.text_of(attribute) {
            "name" => PathMethod::Name,
            "stem" => PathMethod::Stem,
            "suffix" => PathMethod::Suffix,
            "parent" => PathMethod::Parent,
            other => return self.unsupported(node, &format!("the `Path` attribute `{other}`")),
        };
        let arguments = vec![self.path_argument(value)];
        self.path_text(place, method, arguments, node);
    }

    /// `left / right`, where either is a `Path`, into `place`: the two texts joined.
    pub(crate) fn path_join(&mut self, place: Place, node: NodeId) {
        let (Some(left), Some(right)) = (
            self.tree.child(node, Role::Left),
            self.tree.child(node, Role::Right),
        ) else {
            return;
        };
        let arguments = vec![self.path_argument(left), self.path_argument(right)];
        self.path_text(place, PathMethod::Join, arguments, node);
    }

    /// `Path(text)` into `place`: the text in the form `pathlib` keeps.
    pub(crate) fn path_new(&mut self, place: Place, arguments: &[NodeId], node: NodeId) {
        let [text] = arguments else {
            return self.unsupported(node, "these arguments of `Path`");
        };
        let arguments = vec![self.path_argument(*text)];
        self.path_text(place, PathMethod::New, arguments, node);
    }

    /// `path.method(arguments)` into `place`, where `arguments` starts with the path.
    pub(crate) fn path_method(
        &mut self,
        place: Place,
        method: &str,
        arguments: &[NodeId],
        node: NodeId,
    ) {
        let question = match (method, arguments) {
            ("exists", [_]) => PathMethod::Exists,
            ("is_file", [_]) => PathMethod::IsFile,
            ("is_dir", [_]) => PathMethod::IsDirectory,
            ("iterdir", [_]) => PathMethod::IterateDirectory,
            ("copy", [path]) => return self.path_copy(place, *path, node),
            ("read_text", [path]) => return self.read_text(place, *path, node),
            ("write_text", [path, text]) => return self.write_text(place, *path, *text, node),
            _ => return self.unsupported(node, &format!("the `Path` method `{method}`")),
        };
        // The runtime writes `iterdir`'s paths as a list of their texts, which is how a
        // `list[Path]` is laid out: a `Path` is its text and nothing more.
        let passed = vec![self.path_argument(arguments[0])];
        self.raising_call(Builtin::Path(question), passed, place, node);
    }

    /// `os.member(arguments)` for a directory function of `os`, into `place`. Returns whether
    /// `member` with these arguments is one.
    pub(crate) fn directory_call(
        &mut self,
        place: &Place,
        member: &str,
        arguments: &[NodeId],
        node: NodeId,
    ) -> bool {
        let method = match (member, arguments.len()) {
            ("listdir", 0 | 1) => PathMethod::ListDirectory,
            ("remove", 1) => PathMethod::Remove,
            ("rename", 2) => PathMethod::Rename,
            ("mkdir", 1) => PathMethod::MakeDirectory,
            ("makedirs", 1 | 2) => PathMethod::MakeDirectories,
            ("getcwd", 0) => PathMethod::CurrentDirectory,
            _ => return false,
        };
        let boolean = |value| {
            Argument::Owned(Operand::Constant(Constant {
                ty: Type::new(self.db, TypeKind::Primitive(Primitive::Bool)),
                value: ConstantValue::Bool(value),
            }))
        };
        let passed = match (method, arguments) {
            // `os.listdir()` lists the current directory.
            (PathMethod::ListDirectory, []) => vec![self.text_constant(".")],
            (PathMethod::MakeDirectories, [path]) => {
                vec![self.path_argument(*path), boolean(false)]
            }
            (PathMethod::MakeDirectories, [path, exist_ok]) => {
                let path = self.path_argument(*path);
                vec![path, Argument::Owned(self.operand(*exist_ok))]
            }
            _ => arguments
                .iter()
                .map(|&path| self.path_argument(path))
                .collect(),
        };
        self.raising_call(Builtin::Path(method), passed, place.clone(), node);
        true
    }

    /// `str(value)` into `place`, for a `str` or a `Path`, a new `str` with the same text,
    /// or a text array, a new `str` of its elements as `print` shows them. Returns whether
    /// `value` is one of the three.
    pub(crate) fn text_of_value(&mut self, place: &Place, value: NodeId, node: NodeId) -> bool {
        let ty = self.ty(value);
        let shown = self.is_text_array(value);
        if !shown && !self.is_path(ty) && !matches!(ty.kind(self.db), TypeKind::Str) {
            return false;
        }
        let borrowed = self.borrow(value);
        let text = self.shown_place(value, borrowed, ty);
        let pieces = vec![Piece::Value(text, FormatSpec::default())];
        self.assign(place.clone(), Rvalue::Format(pieces), node);
        true
    }

    /// The call of the path method `method`, which gives text, with `arguments`, its result
    /// written into `place`: as it is when `place` holds a `str`, and wrapped in the struct
    /// when it holds a `Path`.
    fn path_text(
        &mut self,
        place: Place,
        method: PathMethod,
        arguments: Vec<Argument<'db>>,
        node: NodeId,
    ) {
        let ty = self.ty(node);
        if !self.is_path(ty) {
            return self.raising_call(Builtin::Path(method), arguments, place, node);
        }
        let str = Type::new(self.db, TypeKind::Str);
        let text = self.temporary(str, node);
        self.raising_call(Builtin::Path(method), arguments, Place::from(text), node);
        let wrapped = Rvalue::Aggregate(ty, vec![Operand::Move(Place::from(text))]);
        self.assign(place, wrapped, node);
    }

    /// `path.copy()` into `place`: a new `Path` around a copy of the text.
    fn path_copy(&mut self, place: Place, path: NodeId, node: NodeId) {
        let ty = self.ty(path);
        let borrowed = self.borrow(path);
        let piece = Piece::Value(self.text_place(borrowed, ty), FormatSpec::default());
        let text = self.temporary(Type::new(self.db, TypeKind::Str), node);
        self.assign(Place::from(text), Rvalue::Format(vec![piece]), node);
        let wrapped = Rvalue::Aggregate(ty, vec![Operand::Move(Place::from(text))]);
        self.assign(place, wrapped, node);
    }

    /// `path.read_text()` into `place`: the file opened to read, read to its end, and closed
    /// when the temporary that holds it is dropped, which cannot fail for a file only read.
    fn read_text(&mut self, place: Place, path: NodeId, node: NodeId) {
        let file = self.open_path(path, 0, node);
        let arguments = vec![Argument::Borrow(Place::from(file))];
        self.raising_call(Builtin::File(FileMethod::Read), arguments, place, node);
    }

    /// `path.write_text(text)` into `place`, the number of characters written: the file opened
    /// to write, written, and closed by a call, so a failure to write it out is an `OSError`
    /// here, as it is from `with open(path, "w") as f:`.
    fn write_text(&mut self, place: Place, path: NodeId, text: NodeId, node: NodeId) {
        let file = self.open_path(path, 1, node);
        let text = self.path_argument(text);
        let arguments = vec![Argument::Borrow(Place::from(file)), text];
        self.raising_call(Builtin::File(FileMethod::Write), arguments, place, node);
        let nothing = Type::new(self.db, TypeKind::None);
        let closed = self.temporary(nothing, node);
        let arguments = vec![Argument::Owned(Operand::Move(Place::from(file)))];
        let close = Builtin::File(FileMethod::Close);
        self.raising_call(close, arguments, Place::from(closed), node);
    }

    /// Opens the file at `path` in the mode numbered `mode`, as `open` numbers them, into a
    /// temporary `File` that the end of the statement drops unless a close moves it first.
    fn open_path(&mut self, path: NodeId, mode: i128, node: NodeId) -> mir::Local {
        let file_type = Type::new(self.db, TypeKind::Standard(StandardType::File));
        let file = self.temporary(file_type, node);
        self.drop_later(file);
        let mode = Operand::Constant(Constant::integer(self.int_type(), mode));
        let arguments = vec![self.path_argument(path), Argument::Owned(mode)];
        let open = Builtin::File(FileMethod::Open);
        self.raising_call(open, arguments, Place::from(file), node);
        file
    }

    /// The `str` constant `text`, passed `Owned`.
    pub(crate) fn text_constant(&self, text: &str) -> Argument<'db> {
        Argument::Owned(Operand::Constant(Constant {
            ty: Type::new(self.db, TypeKind::Str),
            value: ConstantValue::Str(text.to_owned()),
        }))
    }
}
