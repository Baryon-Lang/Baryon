//! The standard `subprocess` module (decision 0036, part 8): `subprocess.run(args, ...)`, which
//! runs another program and waits for it.
//!
//! The compiler builds `run` in, as it builds in `open`, so it is checked here rather than
//! against a declared signature. Its type is `Raises[CompletedProcess, OSError]`, both classes
//! of the standard library's own source (decision 0038).
//!
//! There is no shell: the command is a `list[str]`, and a `str` command or `shell=True` is an
//! error that shows the list to write. `input`, `cwd`, `env` and `timeout` are each written as
//! a value or left out, since the compiler decides here which of them the call passes; `None`
//! written out counts as left out. A value that may be `None`, such as a `str | None`, is not in
//! Baryon yet (idea I.39): it would need the choice made while the program runs. A keyword
//! Python has and decision 0036 leaves for later, such as `check=True`, is reported by name,
//! never given another meaning.

use super::Body;
use super::calls::Arguments;
use super::files::literal;
use syntax::{NodeId, SyntaxKind};
use types::{Call, Callee, Float, Key, Primitive, Type, TypeKind};

/// The keywords of `run` that carry a value to the program, in the order the recorded call
/// lists them after the command.
const OPTIONS: [&str; 5] = ["capture_output", "input", "cwd", "env", "timeout"];

/// What every wrong call is told `run` takes.
const USAGE: &str = "`subprocess.run` takes a `list[str]`, then `capture_output=`, `input=`, \
                     `cwd=`, `env=` and `timeout=`";

impl<'db> Body<'db> {
    /// `subprocess.member(arguments)`: `run`, or a member that is not in Baryon yet.
    pub(super) fn subprocess_call(
        &mut self,
        node: NodeId,
        member: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        if member == "run" {
            return self.run(node, arguments);
        }
        self.untyped(arguments);
        self.not_built(node, "subprocess", member)
    }

    /// `subprocess.run(args, ...)`: checks the command and each keyword, records the call with
    /// the command and then one slot for each of [`OPTIONS`], empty when it is left out, and
    /// gives `Raises[CompletedProcess, OSError]`.
    fn run(&mut self, node: NodeId, arguments: &Arguments) -> Type<'db> {
        let mut given: [Option<NodeId>; 5] = [None; 5];
        let mut sound = true;
        for &(name, value) in &arguments.keywords {
            let written = self.tree.text(name, self.text).to_owned();
            match OPTIONS.iter().position(|&option| option == written) {
                Some(slot) if given[slot].is_none() => {
                    sound &= self.option(&written, value);
                    // `None` written out means what leaving the keyword out means.
                    if self.tree.kind(value) != SyntaxKind::None {
                        given[slot] = Some(value);
                    }
                }
                Some(_) => {
                    self.expression(value, None);
                    let message = format!("`{written}=` is given twice");
                    self.lowerer.error(name, &message, "given already", None);
                    sound = false;
                }
                None => sound &= self.fixed_keyword(name, &written, value),
            }
        }
        let [command] = arguments.positional[..] else {
            for &argument in &arguments.positional {
                self.expression(argument, None);
            }
            return self.lowerer.error(
                node,
                "`subprocess.run` takes one command, a `list[str]`",
                "wrong arguments",
                Some(USAGE),
            );
        };
        sound &= self.command(command);
        let Some(completed) = self.library_class_in("subprocess", "CompletedProcess") else {
            return self.lowerer.error(
                node,
                "`CompletedProcess` is missing from the standard library",
                "cannot be typed",
                Some("the compiler adds the library's files to every package (decision 0038)"),
            );
        };
        if !sound {
            return self.lowerer.unknown();
        }
        let ty = self.raises_library_error(node, completed, "OSError");
        let call = Call {
            callee: Callee::Builtin("subprocess.run".to_owned()),
            type_arguments: Vec::new(),
            arguments: std::iter::once(Some(command))
                .chain(given)
                .map(|argument| argument.map(Key))
                .collect(),
        };
        self.typed.calls.insert(Key(node), call);
        ty
    }

    /// Checks the command: a `list[str]`. A `str` would need a shell to split it, so it is
    /// reported with the list to write instead. Returns whether it is sound.
    fn command(&mut self, command: NodeId) -> bool {
        let str = Type::new(self.db, TypeKind::Str);
        let list = Type::new(self.db, TypeKind::List(str));
        let found = self.expression(command, Some(list));
        if !matches!(found.kind(self.db), TypeKind::Str) {
            self.expect(command, list, found, "a command is");
            return true;
        }
        let note = match literal(self.tree, self.text, command) {
            Some(line) => {
                let words: Vec<String> = line
                    .split_whitespace()
                    .map(|word| format!("\"{word}\""))
                    .collect();
                format!(
                    "write `subprocess.run([{}])`: there is no shell to split the line \
                     (decision 0036)",
                    words.join(", ")
                )
            }
            None => "write the program and each argument as one item of a list: there is no \
                     shell to split the line (decision 0036)"
                .to_owned(),
        };
        self.lowerer.error(
            command,
            "`subprocess.run` takes its command as a `list[str]`, not a `str`",
            "one `str`",
            Some(&note),
        );
        false
    }

    /// Checks the value of the keyword `name`, one of [`OPTIONS`]: a `bool` for
    /// `capture_output`, a `str` for `input` and `cwd`, a `dict[str, str]` for `env`, a number
    /// of seconds for `timeout`; each of the last four may also be `None` written out. Returns
    /// whether it is sound.
    fn option(&mut self, name: &str, value: NodeId) -> bool {
        let str = Type::new(self.db, TypeKind::Str);
        let expected = match name {
            "capture_output" => Type::new(self.db, TypeKind::Primitive(Primitive::Bool)),
            "env" => Type::new(
                self.db,
                TypeKind::Dict {
                    key: str,
                    value: str,
                },
            ),
            "timeout" => Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64))),
            _ => str,
        };
        if name != "capture_output" && self.tree.kind(value) == SyntaxKind::None {
            self.expression(value, None);
            return true;
        }
        let found = self.expression(value, Some(expected));
        if let TypeKind::Optional(inner) = found.kind(self.db)
            && super::relations::compatible(self.db, expected, *inner)
        {
            let message =
                format!("passing a value that may be `None` to `{name}=` is not in Baryon yet");
            self.lowerer.error(
                value,
                &message,
                "may be `None`",
                Some(
                    "test for `None` first, and call `subprocess.run` with the keyword only \
                     where there is a value",
                ),
            );
            return false;
        }
        let whole_seconds = name == "timeout"
            && matches!(
                found.kind(self.db),
                TypeKind::Primitive(Primitive::Integer(_))
            );
        if !whole_seconds {
            self.expect(value, expected, found, &format!("`{name}=` is"));
        }
        true
    }

    /// Checks a keyword Python's `run` has whose value Baryon fixes: `shell=False`,
    /// `text=True` and `check=False` may be written and change nothing; any other value, and
    /// any other keyword, is an error that says why. Returns whether it is sound.
    fn fixed_keyword(&mut self, name: NodeId, written: &str, value: NodeId) -> bool {
        self.expression(value, None);
        let kind = self.tree.kind(value);
        let (message, label, note) = match (written, kind) {
            ("shell" | "check", SyntaxKind::False) | ("text", SyntaxKind::True) => return true,
            ("shell", _) => (
                "`subprocess.run` has no shell".to_owned(),
                "no shell",
                "write the program and each argument as one item of the list, such as \
                 `[\"grep\", \"-c\", pattern, path]`; a shell would split a file name with a \
                 space and let a value become a command (decision 0036)",
            ),
            ("text", _) => (
                "`subprocess.run` gives output as text".to_owned(),
                "always text",
                "there is no `bytes` type: `stdout` and `stderr` are `str`, read as UTF-8 \
                 (decision 0036)",
            ),
            ("check", _) => (
                "`check=True` is not in Baryon yet".to_owned(),
                "not built",
                "test `returncode` after the call: `if result.returncode != 0:`",
            ),
            _ => (
                format!("`subprocess.run` has no parameter `{written}` in Baryon"),
                "unknown parameter",
                USAGE,
            ),
        };
        self.lowerer.error(name, &message, label, Some(note));
        false
    }
}
