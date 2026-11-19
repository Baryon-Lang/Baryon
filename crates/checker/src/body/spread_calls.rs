//! `*args` in a call (the decision 0026 amendment): a tuple's elements passed one by one to a
//! `Callable`, so a callback takes `SciPy`'s `args=` as parameters of its own.
//!
//! `fun(t, y, *args)` is allowed when `fun` is a value of a `Callable` type, `*args` is the
//! last argument and no keyword follows, and `args` is a tuple. In a generic body `args` is a
//! `tuple[*Ts]` and `fun` a `Callable[[f64, S, *Ts], S]`: the same pack ends both, so the call
//! fits whatever `Ts` an instance binds. Outside one, `args` has a known tuple type and its
//! elements are checked against the parameters left. Every other `*` in a call is an error:
//! a star on a list, `f(*xs)`, and on a named function's call, are not in the language.

use super::Body;
use super::calls::Arguments;
use database::Definition;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Callee, Key, Mode, Parameter, Signature, Type, TypeKind};

impl<'db> Body<'db> {
    /// The position of the first `*value` among `arguments`, if any.
    pub(super) fn starred(&self, arguments: &Arguments) -> Option<usize> {
        arguments
            .positional
            .iter()
            .position(|&argument| self.tree.kind(argument) == SyntaxKind::Starred)
    }

    /// The error for a `*value` among `arguments` of a call whose callee, `function` resolved
    /// to `resolved`, is not a value: a named function, a class or a method. Only a call of a
    /// local or of an expression's value takes `*args`.
    pub(super) fn star_outside_value(
        &mut self,
        function: NodeId,
        resolved: Option<&Definition>,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let value = matches!(resolved, Some(Definition::Local(_)) | None)
            && self.tree.kind(function) != SyntaxKind::Attribute;
        let star = self.starred(arguments).filter(|_| !value)?;
        Some(self.misplaced_star(arguments.positional[star], arguments))
    }

    /// The error for a `*value` argument where a call cannot take one: any call but a
    /// `Callable` value's, or one whose star is not last.
    pub(super) fn misplaced_star(&mut self, argument: NodeId, arguments: &Arguments) -> Type<'db> {
        self.untyped(arguments);
        self.lowerer.error(
            argument,
            "`*` unpacking in a call is not supported here",
            "",
            Some(
                "only a tuple, as the last argument of a call to a `Callable` value, is \
                 unpacked: `fun(t, y, *args)`; pass each other argument by position or by name",
            ),
        )
    }

    /// `fun(arguments, *args)`, where `fun` is a `Callable` with `parameters` and `returns`,
    /// and the star is at `star` among the positional arguments.
    pub(super) fn spread_call(
        &mut self,
        node: NodeId,
        callee: &str,
        (parameters, returns): (&[Type<'db>], Type<'db>),
        arguments: &Arguments,
        star: usize,
    ) -> Type<'db> {
        let starred = arguments.positional[star];
        if star + 1 != arguments.positional.len() || !arguments.keywords.is_empty() {
            return self.misplaced_star(starred, arguments);
        }
        let Some(value) = self.tree.child(starred, Role::Value) else {
            return self.misplaced_star(starred, arguments);
        };
        let tuple = self.expression(value, None);
        let spread: Vec<Type<'db>> = match tuple.kind(self.db) {
            TypeKind::Tuple(items) => items.clone(),
            TypeKind::Unknown => return self.untyped_before(arguments, star),
            _ => {
                self.untyped_before(arguments, star);
                let message = format!(
                    "`*` unpacks a tuple, and `{}` is a `{}`",
                    self.tree.text(value, self.text),
                    self.show(tuple)
                );
                return self.lowerer.error(
                    value,
                    &message,
                    "not a tuple",
                    Some("pass a tuple, `args=(10.0, 28.0)`, or each argument by position"),
                );
            }
        };
        // What the elements fill: the parameters after the arguments written before the star.
        let fixed = parameters.get(..star).unwrap_or(parameters);
        let rest = parameters.get(star..).unwrap_or_default();
        if rest != spread.as_slice() {
            self.untyped_before(arguments, star);
            let message = format!(
                "`{callee}` takes {} after the arguments before `*`, and `*{}` gives {}",
                self.listed(rest),
                self.tree.text(value, self.text),
                self.listed(&spread)
            );
            return self.lowerer.error(starred, &message, "does not fit", None);
        }
        let signature = Signature {
            type_parameters: Vec::new(),
            parameters: fixed
                .iter()
                .enumerate()
                .map(|(index, &ty)| Parameter {
                    name: format!("argument {}", index + 1),
                    ty,
                    mode: Mode::Borrow,
                    default: false,
                })
                .collect(),
            returns,
        };
        let before = Arguments {
            positional: arguments.positional[..star].to_vec(),
            keywords: Vec::new(),
        };
        let ty = self.check_call(node, callee, &signature, None, Callee::Value, &before);
        // Lowering passes the arguments the call records, so the star goes last among them.
        if let Some(call) = self.typed.calls.get_mut(&Key(node)) {
            call.arguments.push(Some(Key(starred)));
        }
        ty
    }

    /// Types the positional arguments before the star with no expectation, and gives the call
    /// an unknown type.
    fn untyped_before(&mut self, arguments: &Arguments, star: usize) -> Type<'db> {
        let before = Arguments {
            positional: arguments.positional[..star].to_vec(),
            keywords: arguments.keywords.clone(),
        };
        self.untyped(&before)
    }

    /// `types` as a diagnostic names them: `(f64, f64)`, or `nothing`.
    fn listed(&self, types: &[Type<'db>]) -> String {
        if types.is_empty() {
            return "nothing".to_owned();
        }
        let shown: Vec<String> = types.iter().map(|&ty| self.show(ty)).collect();
        format!("({})", shown.join(", "))
    }

    /// Checks `default`, the default of a parameter of type `ty`, when `ty` is `tuple[*Ts]`,
    /// and says whether it was: the one generic parameter that may have a default, and only
    /// `()`, which leaves `Ts` empty.
    pub(super) fn variadic_default(&mut self, default: NodeId, ty: Type<'db>) -> bool {
        let TypeKind::Tuple(items) = ty.kind(self.db) else {
            return false;
        };
        let pack = |item: &Type<'db>| matches!(item.kind(self.db), TypeKind::Unpacked(_));
        if !matches!(items.as_slice(), [only] if pack(only)) {
            return false;
        }
        let empty = self.tree.kind(default) == SyntaxKind::Tuple
            && self.tree.children(default).next().is_none();
        if !empty {
            self.lowerer.error(
                default,
                "a `tuple[*Ts]` parameter defaults only to `()`",
                "not `()`",
                Some("leaving the argument out passes no extra arguments: `args: tuple[*Ts] = ()`"),
            );
        }
        true
    }
}
