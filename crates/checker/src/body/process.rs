//! The standard `sys` and `os` modules' view of the running process (decision 0036):
//! `sys.argv`, `sys.exit(status)`, `os.getenv(name)` and `os.environ.get(name)`.
//!
//! The compiler builds these in until the standard library can define them, as it builds in
//! `math` and `random`, so they are checked here rather than against declared signatures. A
//! member of `sys` or `os` that decision 0036 has and this slice does not build yet, and one
//! Python has that the decision leaves out, are both reported by name, never given another
//! meaning.

use super::Body;
use super::calls::Arguments;
use database::Definition;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Call, Callee, Integer, Key, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// The type of `sys.argv`: a `list[str]`, a new list of the arguments at each read.
    pub(super) fn argv(&self) -> Type<'db> {
        let str = Type::new(self.db, TypeKind::Str);
        Type::new(self.db, TypeKind::List(str))
    }

    /// Whether `function`, the callee of a call, is a method of `os.environ`, such as
    /// `os.environ.get`.
    pub(super) fn environ_value(&self, function: NodeId) -> bool {
        self.tree.kind(function) == SyntaxKind::Attribute
            && self
                .tree
                .child(function, Role::Value)
                .and_then(|value| self.tree.child(value, Role::Attribute))
                .and_then(|attribute| self.lowerer.definition(attribute))
                .is_some_and(|definition| {
                    matches!(definition, Definition::Standard { module: "os", member: Some(member) } if member == "environ")
                })
    }

    /// Records a typed call of a method of `os.environ` as a call of the built-in
    /// `os.getenv`, which it means, with its arguments as written.
    pub(super) fn record_environ(&mut self, node: NodeId, arguments: &Arguments, ty: Type<'db>) {
        if matches!(ty.kind(self.db), TypeKind::Unknown) {
            return;
        }
        let call = Call {
            callee: Callee::Builtin("os.getenv".to_owned()),
            type_arguments: Vec::new(),
            arguments: arguments
                .positional
                .iter()
                .map(|&argument| Some(Key(argument)))
                .collect(),
        };
        self.typed.calls.insert(Key(node), call);
    }

    /// `module.member(arguments)` for the `sys` or `os` module: `sys.exit(status)` with an
    /// optional `int` status, or `os.getenv(name)` and `os.getenv(name, default)` with `str`s.
    pub(super) fn process_call(
        &mut self,
        node: NodeId,
        module: &str,
        member: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        match (module, member) {
            ("sys", "exit") => self.exit(node, arguments),
            ("os", "getenv") => self.getenv(node, "`os.getenv`", arguments),
            _ => {
                self.untyped(arguments);
                self.not_built(node, module, member)
            }
        }
    }

    /// `os.environ.get(name)` or `os.environ.get(name, default)`, which mean what
    /// `os.getenv` does; any other method of `os.environ` is reported.
    pub(super) fn environ_method(
        &mut self,
        node: NodeId,
        method: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        if method == "get" {
            return self.getenv(node, "`os.environ.get`", arguments);
        }
        self.untyped(arguments);
        self.not_built(node, "os", &format!("environ.{method}"))
    }

    /// The error for `module.member` that this slice does not build.
    pub(super) fn not_built(&mut self, node: NodeId, module: &str, member: &str) -> Type<'db> {
        let message = format!("`{module}.{member}` is not in Baryon yet");
        self.lowerer.error(
            node,
            &message,
            "not built",
            Some(
                "so far `sys` has `argv` and `exit`, and `os` has `getenv` and `environ.get` \
                 (decision 0036)",
            ),
        )
    }

    /// `sys.exit()` or `sys.exit(status)`: the status, an `int`, defaults to 0. The call ends
    /// the program, so its type is `None`.
    fn exit(&mut self, node: NodeId, arguments: &Arguments) -> Type<'db> {
        let int = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        if !arguments.keywords.is_empty() || arguments.positional.len() > 1 {
            self.untyped(arguments);
            return self.lowerer.error(
                node,
                "`sys.exit` takes one status",
                "wrong arguments",
                Some("for example `sys.exit(2)`, an `int` from 0 to 255"),
            );
        }
        for &status in &arguments.positional {
            let found = self.expression(status, Some(int));
            self.expect(status, int, found, "an exit status is");
        }
        Type::new(self.db, TypeKind::None)
    }

    /// `name` alone gives a `str | None`, `None` when the variable is not set; with a `str`
    /// default it gives a `str`. `what` names the call in messages.
    fn getenv(&mut self, node: NodeId, what: &str, arguments: &Arguments) -> Type<'db> {
        let str = Type::new(self.db, TypeKind::Str);
        if !arguments.keywords.is_empty() || !(1..=2).contains(&arguments.positional.len()) {
            self.untyped(arguments);
            let message = format!("{what} takes a name and an optional default");
            return self.lowerer.error(
                node,
                &message,
                "wrong arguments",
                Some(
                    "for example `os.getenv(\"HOME\")` or `os.getenv(\"OMP_NUM_THREADS\", \"1\")`",
                ),
            );
        }
        for &argument in &arguments.positional {
            let found = self.expression(argument, Some(str));
            self.expect(argument, str, found, "a variable's name and default are");
        }
        if arguments.positional.len() == 2 {
            str
        } else {
            Type::new(self.db, TypeKind::Optional(str))
        }
    }
}
