//! The standard streams (decision 0036, part 6): `sys.stdin`, `sys.stdout` and `sys.stderr`,
//! `print(..., file=, flush=)` and `input(prompt)`.
//!
//! A standard stream is a `File`, and each read of `sys.stdout` gives a new handle on the one
//! stream, as each read of `sys.argv` gives a new list, so a module needs no variable to hold
//! it. `print` to a file writes to it, so it can fail with `OSError` as `f.write` can; `print`
//! to `sys.stdout` or `sys.stderr`, named where `file=` is written, never fails, as `print`
//! without `file=` never does (Collins, 2026-11-11). `input` raises `EOFError` at the end of the
//! input.

use super::Body;
use super::calls::Arguments;
use database::Definition;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Primitive, StandardType, Type, TypeKind};

impl<'db> Body<'db> {
    /// The type of `sys.stdin`, `sys.stdout` and `sys.stderr`: a `File`, a new handle on the
    /// stream at each read.
    pub(super) fn standard_stream(&self) -> Type<'db> {
        Type::new(self.db, TypeKind::Standard(StandardType::File))
    }

    /// The standard stream `node` names where it stands, `stdin`, `stdout` or `stderr`, as
    /// `sys.stderr` or as `stderr` after `from sys import stderr`. `None` for anything else, a
    /// variable that holds a stream included.
    fn named_stream(&self, node: NodeId) -> Option<&'db str> {
        let name = match self.tree.kind(node) {
            SyntaxKind::Name => Some(node),
            SyntaxKind::Attribute => self.tree.child(node, Role::Attribute),
            _ => None,
        }?;
        match self.lowerer.definition(name)? {
            Definition::Standard {
                module: "sys",
                member: Some(member),
            } if matches!(member.as_str(), "stdin" | "stdout" | "stderr") => {
                Some(self.tree.text(name, self.text))
            }
            _ => None,
        }
    }

    /// Reports the assignment target `target` when it is a member of a standard module, such as
    /// `sys.stdout = log`: a module has no variables, so there is nothing to assign, and each
    /// read of `sys.stdout` would still give the process's own stream.
    pub(super) fn assigned_module_member(&mut self, target: NodeId) {
        let member = self.tree.child(target, Role::Attribute);
        let standard = member.and_then(|member| self.lowerer.definition(member));
        if !matches!(standard, Some(Definition::Standard { .. })) {
            return;
        }
        let note = self
            .named_stream(target)
            .map(|_| "to write somewhere else, give `print` the file: `print(..., file=log)`");
        let message = format!("`{}` cannot be assigned", self.tree.text(target, self.text));
        self.lowerer
            .error(target, &message, "a module has no variables", note);
    }

    /// The type of a `print` call whose keyword arguments have the types `types`, after
    /// checking `file=` and `flush=`: `None`, or `Raises[None, OSError]` when `file=` gives a
    /// file to write to, which the call changes. `flush=` takes a `bool`.
    pub(super) fn print_target(
        &mut self,
        node: NodeId,
        arguments: &Arguments,
        types: &[Type<'db>],
    ) -> Type<'db> {
        let nothing = Type::new(self.db, TypeKind::None);
        let mut result = nothing;
        for (&(name, value), &ty) in arguments.keywords.iter().zip(types) {
            match self.tree.text(name, self.text) {
                "flush" => {
                    let bool = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
                    self.expect(value, bool, ty, "`flush=` takes a");
                }
                "file" => match (ty.kind(self.db), self.named_stream(value)) {
                    (_, Some("stdin")) => {
                        self.lowerer.error(
                            value,
                            "`print` cannot write to `sys.stdin`",
                            "read only",
                            Some("write to `sys.stdout`, `sys.stderr` or a file opened to write"),
                        );
                    }
                    (_, Some(_)) | (TypeKind::Unknown, None) => {}
                    (TypeKind::Standard(StandardType::File), None) => {
                        self.change(value);
                        result = self.raises_library_error(node, nothing, "OSError");
                    }
                    _ => {
                        let message = format!("`file=` takes a `File`, found `{}`", self.show(ty));
                        self.lowerer.error(
                            value,
                            &message,
                            "not a file",
                            Some(
                                "for example `file=sys.stderr`, or a file from `open(path, \"w\")`",
                            ),
                        );
                    }
                },
                _ => {}
            }
        }
        result
    }

    /// `input()` or `input(prompt)`, the prompt a `str`: `Raises[str, EOFError]`, the next line
    /// of standard input, or the error at its end.
    pub(super) fn input(
        &mut self,
        node: NodeId,
        arguments: &Arguments,
        types: &[Type<'db>],
    ) -> Type<'db> {
        let str = Type::new(self.db, TypeKind::Str);
        if !arguments.keywords.is_empty() || arguments.positional.len() > 1 {
            return self.lowerer.error(
                node,
                "`input` takes one prompt",
                "wrong arguments",
                Some("for example `input()` or `input(\"grid size: \")`"),
            );
        }
        if let (Some(&prompt), Some(&ty)) = (arguments.positional.first(), types.first()) {
            self.expect(prompt, str, ty, "a prompt is");
        }
        self.raises_library_error(node, str, "EOFError")
    }
}
