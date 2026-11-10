//! Text files (decision 0036, parts 3 and 4): `open`, the `File` methods and
//! `with open(...) as f:`, each a call of a [`Builtin::File`] that can fail with `OSError`.
//!
//! - `open(path, mode)` passes the mode as its number: 0 `"r"`, 1 `"w"`, 2 `"a"`, 3 `"x"`.
//! - `f.read()`, `f.write(text)` and the other methods borrow the file; `f.close()` moves it in,
//!   so it is not dropped again.
//! - `with value as f:` stores the file in `f`, lowers the body, and where the body falls
//!   through, closes it as `f.close()` does, so a failure to write it out is an `OSError` from
//!   the `with` statement. A body that leaves early by `return`, `break` or an error leaves `f`
//!   to be dropped, which also closes it.
//!
//! Each call's error edge goes where any failing call's does (the `errors` module).

use crate::builder::Builder;
use mir::{Argument, Builtin, Callee, Constant, FileMethod, Operand, Place, TerminatorKind};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Integer, Primitive, Type, TypeKind};

/// The modes `open` takes and the number each passes, as the checker accepts them.
const MODES: [(&str, i128); 4] = [("r", 0), ("w", 1), ("a", 2), ("x", 3)];

impl<'db> Builder<'db> {
    /// `open(path)` or `open(path, mode)` into `place`, a new `File`; `arguments` are the path
    /// and the mode, if given, as the checker recorded them.
    pub(crate) fn open(&mut self, place: Place, arguments: &[NodeId], node: NodeId) {
        let (path, mode) = match *arguments {
            [path] => (path, None),
            [path, mode] => (path, Some(mode)),
            _ => return self.unsupported(node, "these arguments of `open`"),
        };
        let written = mode.map_or(Some("r"), |mode| {
            let text = self.text_of(mode);
            text.get(1..text.len().checked_sub(1)?)
        });
        let Some(&(_, number)) = MODES.iter().find(|&&(known, _)| Some(known) == written) else {
            return self.unsupported(node, "this mode of `open`");
        };
        let int = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        let path = self.text_argument(path);
        let mode = Argument::Owned(Operand::Constant(Constant::integer(int, number)));
        self.file_call(FileMethod::Open, vec![path, mode], place, node);
    }

    /// `file.method(arguments)` into `place`, where `arguments` starts with the file.
    pub(crate) fn file_method(
        &mut self,
        place: Place,
        method: &str,
        arguments: &[NodeId],
        node: NodeId,
    ) {
        let method = match (method, arguments) {
            ("read", [_]) => FileMethod::Read,
            ("readline", [_]) => FileMethod::ReadLine,
            ("readlines", [_]) => FileMethod::ReadLines,
            ("write", [_, _]) => FileMethod::Write,
            ("flush", [_]) => FileMethod::Flush,
            ("close", [_]) => FileMethod::Close,
            _ => return self.unsupported(node, &format!("the `File` method `{method}`")),
        };
        // A file that is not a variable, as in `open(path).read()`, is computed into a
        // temporary that the statement's end drops, which closes it.
        let file = self.borrow(arguments[0]);
        let file = if method == FileMethod::Close {
            Argument::Owned(Operand::Move(file))
        } else {
            Argument::Borrow(file)
        };
        let mut passed = vec![file];
        if let [_, text] = arguments {
            passed.push(self.text_argument(*text));
        }
        self.file_call(method, passed, place, node);
    }

    /// `with value as name:` and its body, the file closed where the body falls through.
    pub(crate) fn with_file(&mut self, statement: NodeId, item: NodeId) {
        let (Some(value), Some(target)) = (
            self.tree.child(item, Role::Value),
            self.tree.child(item, Role::Target),
        ) else {
            return;
        };
        let Some(file) = self.target_variable(target) else {
            return self.unsupported(target, "this `with` target");
        };
        self.enter();
        self.store(target, value);
        self.leave(statement);
        self.block(statement, Role::Body);
        if self.is_dead() {
            return;
        }
        let none = Type::new(self.db, TypeKind::None);
        let closed = self.temporary(none, statement);
        let arguments = vec![Argument::Owned(Operand::Move(Place::from(file)))];
        self.file_call(FileMethod::Close, arguments, Place::from(closed), statement);
    }

    /// A `str` argument: a literal passed `Owned`, anything else borrowed.
    fn text_argument(&mut self, text: NodeId) -> Argument<'db> {
        if self.tree.kind(text) == SyntaxKind::StringLiteral {
            Argument::Owned(self.operand(text))
        } else {
            Argument::Borrow(self.borrow(text))
        }
    }

    /// A call of the file operation `method`, its result written into `place`, with the error
    /// edge for the errors the checker recorded at `node`.
    fn file_call(
        &mut self,
        method: FileMethod,
        arguments: Vec<Argument<'db>>,
        place: Place,
        node: NodeId,
    ) {
        let errors = self.typed.errors(node).to_vec();
        let edge = self.error_edge(&errors, node);
        let next = self.new_block();
        self.terminate(
            TerminatorKind::Call {
                callee: Callee::Builtin(Builtin::File(method)),
                arguments,
                destination: place,
                target: next,
                error: edge.clone(),
            },
            node,
        );
        if let Some(edge) = edge {
            self.dispatch(&edge, &errors, node);
        }
        self.start(next);
    }
}
