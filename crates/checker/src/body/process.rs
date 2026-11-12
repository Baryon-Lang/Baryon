//! The standard `sys` and `os` modules' view of the running process (decision 0036):
//! `sys.argv`, `sys.exit(status)`, `os.getenv(name)`, and the four ways to read `os.environ`:
//! `os.environ[name]`, `os.environ.get(name)`, `name in os.environ` and `os.environ.copy()`.
//! The environment is read-only, so anything that would change it is reported here.
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

    /// Whether `node` names `os.environ`, as `os.environ` or as `environ` after
    /// `from os import environ`.
    pub(super) fn is_environ(&self, node: NodeId) -> bool {
        let name = match self.tree.kind(node) {
            SyntaxKind::Name => Some(node),
            SyntaxKind::Attribute => self.tree.child(node, Role::Attribute),
            _ => None,
        };
        name.and_then(|name| self.lowerer.definition(name))
            .is_some_and(|definition| {
                matches!(definition, Definition::Standard { module: "os", member: Some(member) } if member == "environ")
            })
    }

    /// Whether `function`, the callee of a call, is a method of `os.environ`, such as
    /// `os.environ.get`.
    pub(super) fn environ_value(&self, function: NodeId) -> bool {
        self.tree.kind(function) == SyntaxKind::Attribute
            && self
                .tree
                .child(function, Role::Value)
                .is_some_and(|value| self.is_environ(value))
    }

    /// `os.environ[name]`: the variable's value, a `str`. A variable that is not set panics, as
    /// a missing `dict` key does (decision 0036, part 5).
    pub(super) fn environ_item(&mut self, node: NodeId) -> Type<'db> {
        let str = Type::new(self.db, TypeKind::Str);
        if let Some(index) = self.tree.child(node, Role::Index) {
            let found = self.expression(index, Some(str));
            self.expect(index, str, found, "a variable's name is");
        }
        str
    }

    /// Reports `os.environ` used on its own, as a value: the environment has no type of its
    /// own, only the four ways to read it.
    pub(super) fn environ_alone(&mut self, node: NodeId) -> Type<'db> {
        self.lowerer.error(
            node,
            "`os.environ` is not a value on its own",
            "read one variable, or copy them all",
            Some(
                "the environment is read with `os.environ[name]`, `os.environ.get(name)`, \
                 `name in os.environ` and `os.environ.copy()`, which gives a `dict[str, str]` \
                 (decision 0036)",
            ),
        )
    }

    /// Reports the place `node` when changing it would change the environment, as
    /// `os.environ[name] = value` and `del os.environ[name]` would, and returns whether it did:
    /// the environment is read-only (decision 0036, part 5).
    pub(super) fn environ_changed(&mut self, node: NodeId) -> bool {
        let environ = self.tree.kind(node) == SyntaxKind::Subscript
            && self
                .tree
                .child(node, Role::Value)
                .is_some_and(|value| self.is_environ(value));
        if environ {
            self.read_only_environment(node);
        }
        environ
    }

    /// The error for anything that would change the environment.
    fn read_only_environment(&mut self, node: NodeId) -> Type<'db> {
        self.lowerer.error(
            node,
            "the environment is read-only",
            "cannot be changed",
            Some(
                "changing it would race with every thread that reads it; to run a program with \
                 another environment, pass it one: `subprocess.run(..., env=...)` (decision 0036)",
            ),
        )
    }

    /// Records a typed call of the method `method` of `os.environ`: `get` as a call of the
    /// built-in `os.getenv`, which it means, with its arguments as written, and `copy` as the
    /// built-in `os.environ.copy`.
    pub(super) fn record_environ(
        &mut self,
        node: NodeId,
        method: &str,
        arguments: &Arguments,
        ty: Type<'db>,
    ) {
        if matches!(ty.kind(self.db), TypeKind::Unknown) {
            return;
        }
        let name = if method == "copy" {
            "os.environ.copy"
        } else {
            "os.getenv"
        };
        let call = Call {
            callee: Callee::Builtin(name.to_owned()),
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
            ("os", "putenv" | "unsetenv") => {
                self.untyped(arguments);
                self.read_only_environment(node)
            }
            _ => {
                if module == "os"
                    && let Some(ty) = self.directory_call(node, member, arguments)
                {
                    return ty;
                }
                self.untyped(arguments);
                self.not_built(node, module, member)
            }
        }
    }

    /// `os.environ.get(name)` or `os.environ.get(name, default)`, which mean what
    /// `os.getenv` does, or `os.environ.copy()`, a `dict[str, str]` of every variable; any
    /// other method of `os.environ` is reported.
    pub(super) fn environ_method(
        &mut self,
        node: NodeId,
        method: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        if method == "get" {
            return self.getenv(node, "`os.environ.get`", arguments);
        }
        let empty = arguments.positional.is_empty() && arguments.keywords.is_empty();
        if method == "copy" && empty {
            let str = Type::new(self.db, TypeKind::Str);
            let copy = TypeKind::Dict {
                key: str,
                value: str,
            };
            return Type::new(self.db, copy);
        }
        self.untyped(arguments);
        if method == "copy" {
            return self
                .lowerer
                .error(node, "`copy` takes no arguments", "wrong arguments", None);
        }
        self.not_built(node, "os", &format!("environ.{method}"))
    }

    /// `module.member` of `sys`, `os` or `subprocess` read as a value: `sys.argv` is a
    /// `list[str]`, `os.environ` on its own is an error, and any other member is not built.
    pub(super) fn process_value(&mut self, node: NodeId, module: &str, member: &str) -> Type<'db> {
        match (module, member) {
            ("sys", "argv") => self.argv(),
            ("sys", "stdin" | "stdout" | "stderr") => self.standard_stream(),
            ("os", "environ") => self.environ_alone(node),
            _ => self.not_built(node, module, member),
        }
    }

    /// The error for `module.member` of `sys`, `os` or `subprocess` that is not built.
    pub(super) fn not_built(&mut self, node: NodeId, module: &str, member: &str) -> Type<'db> {
        let message = format!("`{module}.{member}` is not in Baryon yet");
        let built = if module == "subprocess" {
            "`subprocess` has `run` and `CompletedProcess` (decision 0036)"
        } else {
            "so far `sys` has `argv`, `exit`, `stdin`, `stdout` and `stderr`, and `os` has \
             `environ`, `getenv`, `listdir`, `remove`, `rename`, `mkdir`, `makedirs`, `getcwd` and \
             `ErrorKind` (decision 0036)"
        };
        self.lowerer.error(node, &message, "not built", Some(built))
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
