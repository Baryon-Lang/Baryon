//! Text files (decision 0036, parts 3 and 4): `open`, the `File` methods,
//! `with open(...) as f:` and `for line in f:`, each a call of a [`Builtin::File`] that can fail
//! with `OSError`.
//!
//! - `open(path, mode)` passes the mode as its number: 0 `"r"`, 1 `"w"`, 2 `"a"`, 3 `"x"`.
//! - `f.read()`, `f.write(text)` and the other methods borrow the file; `f.close()` moves it in,
//!   so it is not dropped again.
//! - `with value as f:` stores the file in `f`, lowers the body, and where the body falls
//!   through, closes it as `f.close()` does, so a failure to write it out is an `OSError` from
//!   the `with` statement. A body that leaves early by `return`, `break` or an error leaves `f`
//!   to be dropped, which also closes it.
//! - `for line in f:` calls `f.readline()` at the top of each pass and leaves at the empty
//!   string, the end of the file. The loop variable owns each line.
//!
//! Each call's error edge goes where any failing call's does (the `errors` module).

use crate::builder::Builder;
use crate::loops::Pass;
use mir::{
    Argument, BinaryOperator, Builtin, Callee, Constant, ConstantValue, FileMethod, Operand, Place,
    Rvalue, StatementKind, TerminatorKind,
};
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
        let path = self.path_argument(path);
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
            passed.push(self.path_argument(*text));
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

    /// `for target in file: body`, or a comprehension's clause over a file. Each pass reads
    /// the next line into a temporary, as `file.readline()` does; an empty line is the end of
    /// the file and leaves the loop, and any other replaces the loop variable's value, which
    /// the variable owns, so the last line stays in it after the loop, as in Python. A read
    /// that fails takes the error edge the checker recorded at `statement`.
    ///
    /// Cost: one runtime call and one new `str` for each line, and no system call until the
    /// file's 64 KiB buffer runs out.
    pub(crate) fn file_loop(
        &mut self,
        statement: NodeId,
        target: NodeId,
        iterator: NodeId,
        pass: Pass<'_, 'db>,
    ) {
        if self.tree.kind(target) != SyntaxKind::Name {
            return self.unsupported(target, "this loop target over a file");
        }
        let Some((variable, _)) = self.target(target) else {
            return;
        };
        let file = self.borrow(iterator);
        let str = Type::new(self.db, TypeKind::Str);
        let (int, boolean) = (self.int_type(), self.boolean_type());
        let header = self.new_block();
        self.terminate(TerminatorKind::Goto(header), statement);
        self.start(header);
        let line = self.temporary(str, iterator);
        let read = vec![Argument::Borrow(file)];
        self.file_call(FileMethod::ReadLine, read, Place::from(line), statement);
        // Only the end of the file reads as the empty string: an empty line is `"\n"`.
        let order = self.temporary(int, iterator);
        let empty = Operand::Constant(Constant {
            ty: str,
            value: ConstantValue::Str(String::new()),
        });
        let compared = vec![Argument::Borrow(Place::from(line)), Argument::Owned(empty)];
        self.text_call(Place::from(order), Builtin::CompareStr, compared, iterator);
        let more = self.temporary(boolean, iterator);
        let zero = Operand::Constant(Constant::integer(int, 0));
        let order = Operand::Copy(Place::from(order));
        let differs = Rvalue::Binary(BinaryOperator::NotEqual, order, zero);
        self.assign(Place::from(more), differs, iterator);
        let (body, finished, end) = (self.new_block(), self.new_block(), self.new_block());
        self.branch(Operand::Copy(Place::from(more)), (body, finished), iterator);
        self.start(finished);
        self.push(StatementKind::Drop(Place::from(line)), iterator);
        self.terminate(TerminatorKind::Goto(end), statement);
        self.start(body);
        self.replace(variable, Operand::Move(Place::from(line)), target);
        pass(self, header, end);
        self.start(end);
    }

    /// A `str` argument: a literal passed `Owned`, anything else borrowed.
    pub(crate) fn text_argument(&mut self, text: NodeId) -> Argument<'db> {
        if self.tree.kind(text) == SyntaxKind::StringLiteral {
            Argument::Owned(self.operand(text))
        } else {
            Argument::Borrow(self.borrow(text))
        }
    }

    /// A call of the file operation `method`, its result written into `place`, with the error
    /// edge for the errors the checker recorded at `node`.
    pub(crate) fn file_call(
        &mut self,
        method: FileMethod,
        arguments: Vec<Argument<'db>>,
        place: Place,
        node: NodeId,
    ) {
        self.raising_call(Builtin::File(method), arguments, place, node);
    }

    /// A call of `builtin`, its result written into `place`, with the error edge for the errors
    /// the checker recorded at `node`: none for a built-in that cannot fail.
    pub(crate) fn raising_call(
        &mut self,
        builtin: Builtin,
        arguments: Vec<Argument<'db>>,
        place: Place,
        node: NodeId,
    ) {
        let errors = self.typed.errors(node).to_vec();
        let edge = self.error_edge(&errors, node);
        let next = self.new_block();
        self.terminate(
            TerminatorKind::Call {
                callee: Callee::Builtin(builtin),
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
