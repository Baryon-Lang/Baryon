//! The standard `sys` and `os` modules' view of the running process (decision 0036), each a
//! call of a [`Builtin`] that the backend hands to the runtime:
//!
//! - `sys.argv` is a new `list[str]`, built by appending [`Builtin::Argument`] for each of the
//!   [`Builtin::ArgumentCount`] arguments, so every read gives a list of its own.
//! - `sys.exit(status)` is a call of [`Builtin::Exit`], which never returns.
//! - `os.getenv(name)` and `os.environ.get(name)`, with or without a default, are a call of
//!   [`Builtin::EnvironmentGet`] and a branch on whether the variable is set.
//!
//! [`Builder::standard_call`] also sends the calls of `random` and the `File` methods to their
//! modules, so the one match over built-ins in `calls` stays short.

use crate::builder::Builder;
use database::Definition;
use mir::{Argument, Builtin, Callee, Check, Constant, ConstantValue, Local, Operand, Piece};
use mir::{Place, Rvalue, TerminatorKind, UnaryOperator};
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::{FormatSpec, Primitive, Type, TypeKind};

impl<'db> Builder<'db> {
    /// Whether `node` reads `sys.argv`, as `sys.argv` or as `argv` after `from sys import argv`.
    pub(crate) fn is_argv(&self, node: NodeId) -> bool {
        self.is_standard(node, "sys", "argv")
    }

    /// Whether `node` names `os.environ`, as `os.environ` or as `environ` after
    /// `from os import environ`.
    pub(crate) fn is_environ(&self, node: NodeId) -> bool {
        self.is_standard(node, "os", "environ")
    }

    /// Whether `node` is `os.environ[name]`, which names no place: each read asks the runtime.
    pub(crate) fn is_environ_item(&self, node: NodeId) -> bool {
        self.tree.kind(node) == SyntaxKind::Subscript
            && self
                .tree
                .child(node, Role::Value)
                .is_some_and(|value| self.is_environ(value))
    }

    /// Whether `node` is a value that names no place because each read computes it: `sys.argv`,
    /// `sys.stdout` and the other standard streams, `os.environ[name]`, or a part of a `Path`
    /// such as `path.name`, or `shape` or `columns` of a `DataFrame`.
    pub(crate) fn is_computed(&self, node: NodeId) -> bool {
        self.is_argv(node)
            || self.is_environ_item(node)
            || self.is_path_part(node)
            || self.standard_stream(node).is_some()
            || self.is_library_access(node)
            || self.is_frame_attribute(node)
    }

    /// Whether `node` names the member `member` of the standard module `module`, as
    /// `module.member` or as `member` after `from module import member`.
    fn is_standard(&self, node: NodeId, module: &str, member: &str) -> bool {
        let name = match self.tree.kind(node) {
            SyntaxKind::Name => Some(node),
            SyntaxKind::Attribute => self.tree.child(node, Role::Attribute),
            _ => None,
        };
        name.and_then(|name| self.definition(name))
            .is_some_and(|definition| {
                matches!(definition, Definition::Standard { module: found, member: Some(name) } if *found == module && name == member)
            })
    }

    /// `sys.argv` into `place`: an empty `list[str]`, then each argument appended in order as a
    /// new owned `str`. Cost: one copy of each argument, a few strings for most programs.
    pub(crate) fn argv(&mut self, place: &Place, node: NodeId) {
        let ty = self.ty(node);
        let int = self.int_type();
        let count = self.temporary(int, node);
        self.call_builtin(Builtin::ArgumentCount, Vec::new(), Place::from(count), node);
        self.assign(place.clone(), Rvalue::Aggregate(ty, Vec::new()), node);
        let str = Type::new(self.db, TypeKind::Str);
        let none_type = Type::new(self.db, TypeKind::None);
        let lengths = [Operand::Copy(Place::from(count))];
        self.loop_nest(&lengths, &mut Vec::new(), node, &mut |builder, counters| {
            let [counter] = counters else { return };
            let argument = builder.temporary(str, node);
            let position = Argument::Owned(Operand::Copy(Place::from(*counter)));
            builder.call_builtin(
                Builtin::Argument,
                vec![position],
                Place::from(argument),
                node,
            );
            let appended = vec![
                Argument::InOut(place.clone()),
                Argument::Owned(Operand::Move(Place::from(argument))),
            ];
            let nothing = builder.temporary(none_type, node);
            builder.call_builtin(Builtin::Append, appended, Place::from(nothing), node);
        });
    }

    /// `sys.exit()` or `sys.exit(status)` into `place`: a call of [`Builtin::Exit`], the status
    /// 0 when it is left out. The call never returns; the block after it is unreachable.
    pub(crate) fn exit(&mut self, place: Place, status: Option<NodeId>, node: NodeId) {
        let int = self.int_type();
        let status = match status {
            Some(status) => self.operand_as(status, int),
            None => Operand::Constant(Constant::integer(int, 0)),
        };
        self.call_builtin(Builtin::Exit, vec![Argument::Owned(status)], place, node);
    }

    /// `os.getenv(name)` or `os.getenv(name, default)` into `place`: the variable's value, as a
    /// `str | None` without a default and a `str` with one. The runtime gives a new owned `str`
    /// and whether the variable is set; the value moves into `place` when it is, and the default
    /// or `None` goes there when it is not, the unused empty `str` dropped.
    pub(crate) fn getenv(
        &mut self,
        place: Place,
        name: NodeId,
        default: Option<NodeId>,
        node: NodeId,
    ) {
        let name = if self.is_literal(name) {
            Argument::Owned(self.operand(name))
        } else {
            Argument::Borrow(self.borrow(name))
        };
        let (found, value) = self.environment_get(name, node);
        let (set, unset, join) = (self.new_block(), self.new_block(), self.new_block());
        self.branch(Operand::Copy(Place::from(found)), (set, unset), node);
        self.start(set);
        let moved = Operand::Move(Place::from(value));
        let result = self.ty(node);
        let rvalue = if default.is_some() {
            Rvalue::Use(moved)
        } else {
            Rvalue::Aggregate(result, vec![moved])
        };
        self.assign(place.clone(), rvalue, node);
        self.terminate(TerminatorKind::Goto(join), node);
        self.start(unset);
        let fallback = match default {
            Some(default) => self.operand(default),
            None => Operand::Constant(Constant {
                ty: result,
                value: ConstantValue::None,
            }),
        };
        self.assign(place, Rvalue::Use(fallback), node);
        self.terminate(TerminatorKind::Goto(join), node);
        self.start(join);
    }

    /// The call of [`Builtin::EnvironmentGet`] for the variable `name`: a `bool` local set to
    /// whether it is set, and a `str` local holding its value, or the empty text, which the end
    /// of the statement drops unless it is moved first.
    fn environment_get(&mut self, name: Argument<'db>, node: NodeId) -> (Local, Local) {
        let str = Type::new(self.db, TypeKind::Str);
        let found = self.temporary(
            Type::new(self.db, TypeKind::Primitive(Primitive::Bool)),
            node,
        );
        let value = self.temporary(str, node);
        self.drop_later(value);
        let arguments = vec![name, Argument::InOut(Place::from(found))];
        self.call_builtin(Builtin::EnvironmentGet, arguments, Place::from(value), node);
        (found, value)
    }

    /// `os.environ[name]` into `place`: the variable's value. A variable that is not set
    /// panics with the message of a missing `dict` key, naming it (decision 0036, part 5); the
    /// text of that message is built only on the failing path.
    pub(crate) fn environ_item(&mut self, place: Place, node: NodeId) {
        let Some(index) = self.tree.child(node, Role::Index) else {
            return;
        };
        let name = self.borrow(index);
        let (found, value) = self.environment_get(Argument::Borrow(name.clone()), node);
        let (set, unset) = (self.new_block(), self.new_block());
        self.branch(Operand::Copy(Place::from(found)), (set, unset), node);
        self.start(unset);
        let quote = || Piece::Text("'".to_owned());
        let quoted = vec![quote(), Piece::Value(name, FormatSpec::default()), quote()];
        let key = self.temporary(Type::new(self.db, TypeKind::Str), node);
        self.assign(Place::from(key), Rvalue::Format(quoted), node);
        self.terminate(
            TerminatorKind::Assert {
                condition: Operand::Copy(Place::from(found)),
                check: Check::KeyPresent {
                    key: Operand::Move(Place::from(key)),
                },
                target: set,
            },
            node,
        );
        self.start(set);
        let moved = Rvalue::Use(Operand::Move(Place::from(value)));
        self.assign(place, moved, node);
    }

    /// `name in os.environ`, or `not in`, into the `bool` at `place`: whether the variable is
    /// set. The value the runtime also reads is dropped.
    pub(crate) fn environ_contains(
        &mut self,
        place: Place,
        operator: Operator,
        name: NodeId,
        node: NodeId,
    ) {
        let name = if self.is_literal(name) {
            Argument::Owned(self.operand(name))
        } else {
            Argument::Borrow(self.borrow(name))
        };
        let (found, _) = self.environment_get(name, node);
        let found = Operand::Copy(Place::from(found));
        let rvalue = if operator == Operator::NotIn {
            Rvalue::Unary(UnaryOperator::Not, found)
        } else {
            Rvalue::Use(found)
        };
        self.assign(place, rvalue, node);
    }

    /// A call of a built-in of `random`, `sys` or `os`, or of a `Philox` method, named
    /// `owner.member`, into `place`: `random.Philox`, `Philox.uniform`, `sys.exit` and
    /// `os.getenv`, which `os.environ.get` is recorded as, and a `File` method. Any other, or the wrong number of
    /// arguments, is reported as not lowered yet.
    pub(crate) fn standard_call(
        &mut self,
        place: Place,
        (owner, member): (&str, &str),
        arguments: &[NodeId],
        node: NodeId,
    ) {
        if owner == "os" && self.directory_call(&place, member, arguments, node) {
            return;
        }
        match ((owner, member), arguments) {
            (("os", "environ.copy"), []) => self.environ_copy(&place, node),
            (("Path", method), _) => self.path_method(place, method, arguments, node),
            (("random", "Philox"), [seed, rest @ ..]) if rest.len() <= 1 => {
                self.philox(place, *seed, rest.first().copied(), node);
            }
            (("Philox", "uniform"), [generator]) => self.uniform(place, *generator, node),
            (("sys", "exit"), [] | [_]) => self.exit(place, arguments.first().copied(), node),
            (("os", "getenv"), [name, rest @ ..]) if rest.len() <= 1 => {
                self.getenv(place, *name, rest.first().copied(), node);
            }
            (("File", method), _) => self.file_method(place, method, arguments, node),
            _ => self.unsupported(node, &format!("the built-in `{owner}.{member}`")),
        }
    }

    /// A call of `builtin` with `arguments`, its result written into `place`; it cannot fail.
    pub(crate) fn call_builtin(
        &mut self,
        builtin: Builtin,
        arguments: Vec<Argument<'db>>,
        place: Place,
        node: NodeId,
    ) {
        self.then(node, |next| TerminatorKind::Call {
            callee: Callee::Builtin(builtin),
            arguments,
            destination: place,
            target: next,
            error: None,
        });
    }
}
