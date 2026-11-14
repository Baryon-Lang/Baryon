//! `for` over a value of the program's own class (decision 0053, part 3).
//!
//! - An *iterator* defines `__next__(self: InOut[Self]) -> Raises[T, StopIteration]`. The loop
//!   calls it on the value itself once a pass, binds the `T` to the loop variable, which owns
//!   it, and ends at `StopIteration`. The value is borrowed for the loop like a collection, so
//!   the body may read it but not change or move it.
//! - An *iterable* defines `__iter__` but not `__next__`. The loop calls `__iter__` once, in the
//!   mode it declares, and loops over the iterator it returns.
//!
//! Errors `__next__` lists besides `StopIteration`, and those of `__iter__`, are errors of the
//! `for` statement. A comprehension over such a value is not built yet.

use super::calls::Arguments;
use super::exclusive::Passed;
use super::{Body, relations};
use crate::embed::{self, Own};
use syntax::NodeId;
use types::{Callee, Declaration, Item, Key, Mode, Signature, Type, TypeKind};

impl<'db> Body<'db> {
    /// The method `name` the class of `ty` defines itself, with the class's type arguments in
    /// its signature. `None` for any other type, or when the class does not define it.
    pub(super) fn dunder_method(
        &self,
        ty: Type<'db>,
        name: &str,
    ) -> Option<(Item<'db>, Signature<'db>)> {
        let TypeKind::Struct(nominal) = ty.kind(self.db) else {
            return None;
        };
        let Some(Own::Method {
            item,
            parameters,
            arguments,
        }) = embed::own(self.db, self.lowerer.package, nominal, name)
        else {
            return None;
        };
        let Declaration::Function(signature) = &self.db.declaration(item).declaration else {
            return None;
        };
        let signature = relations::substitute_signature(
            self.db,
            signature,
            nominal.file,
            &parameters,
            &arguments,
        );
        Some((item, signature))
    }

    /// The element type of a loop over `iterator`, of type `ty`, a value of the program's own
    /// class with `__next__` or `__iter__`, inside the `for` statement `statement`, or `None`
    /// in a comprehension (`statement` is `None` there), which is reported. Returns `None`,
    /// reporting nothing, when the class defines neither method.
    pub(super) fn user_iteration(
        &mut self,
        iterator: NodeId,
        ty: Type<'db>,
        statement: Option<NodeId>,
    ) -> Option<Type<'db>> {
        let next = self.dunder_method(ty, "__next__");
        let iter = self.dunder_method(ty, "__iter__");
        if next.is_none() && iter.is_none() {
            return None;
        }
        let Some(statement) = statement else {
            return Some(self.lowerer.error(
                iterator,
                "a comprehension over a class with `__next__` or `__iter__` is not built yet",
                "not built yet",
                Some("write a `for` statement that appends to a list (decision 0053)"),
            ));
        };
        let element = match (next, iter) {
            (Some((_, signature)), _) => {
                let element = self.next_element(iterator, ty, &signature, statement);
                self.pass(&[Passed {
                    node: iterator,
                    mode: Mode::InOut,
                    ty,
                }]);
                element
            }
            (None, Some((item, signature))) => {
                let mode = signature
                    .parameters
                    .first()
                    .map_or(Mode::Borrow, |parameter| parameter.mode);
                let receiver = Passed {
                    node: iterator,
                    mode,
                    ty,
                };
                let arguments = Arguments {
                    positional: Vec::new(),
                    keywords: Vec::new(),
                };
                let returned = self.check_call(
                    statement,
                    "__iter__",
                    &signature,
                    Some(receiver),
                    Callee::Method(item),
                    &arguments,
                );
                let iterated = match returned.kind(self.db) {
                    TypeKind::Fallible { value, errors } => {
                        let (value, errors) = (*value, errors.clone());
                        self.typed.errors.insert(Key(statement), errors.clone());
                        self.propagate(statement, "`__iter__` can fail with", &errors);
                        value
                    }
                    _ => returned,
                };
                if let Some((_, next)) = self.dunder_method(iterated, "__next__") {
                    self.next_element(iterator, iterated, &next, statement)
                } else {
                    let message = format!(
                        "`__iter__` returns `{}`, which has no `__next__`",
                        self.show(iterated)
                    );
                    self.lowerer.error(
                        iterator,
                        &message,
                        "not an iterator",
                        Some(
                            "`__iter__` returns an iterator: a class with \
                            `def __next__(self: InOut[Self]) -> Raises[T, StopIteration]` \
                            (decision 0053)",
                        ),
                    )
                }
            }
            (None, None) => return None,
        };
        self.user_loops.insert(statement);
        Some(element)
    }

    /// The `T` a loop over `iterator`, an iterator of type `ty`, binds on each pass: the value
    /// `__next__`, whose signature is `signature`, returns. Reports a `__next__` with another
    /// shape, and checks the errors it lists besides `StopIteration`.
    fn next_element(
        &mut self,
        iterator: NodeId,
        ty: Type<'db>,
        signature: &Signature<'db>,
        statement: NodeId,
    ) -> Type<'db> {
        let stop = self.library_class("StopIteration");
        let returned = match signature.returns.kind(self.db) {
            TypeKind::Fallible { value, errors }
                if stop.is_some_and(|stop| errors.contains(&stop)) =>
            {
                Some((*value, errors.clone()))
            }
            _ => None,
        };
        let in_out = matches!(signature.parameters.as_slice(), [only] if only.mode == Mode::InOut);
        let Some((element, errors)) = returned.filter(|_| in_out) else {
            let message = format!(
                "`{}.__next__` must take `self: InOut[Self]` and return \
                 `Raises[T, StopIteration]` for `for`",
                self.show(ty)
            );
            return self.lowerer.error(
                iterator,
                &message,
                "looped over here",
                Some(
                    "`__next__` changes the iterator and raises `StopIteration` when it has no \
                    value left, which ends the loop (decision 0053)",
                ),
            );
        };
        let others: Vec<Type<'db>> = errors
            .into_iter()
            .filter(|&error| Some(error) != stop)
            .collect();
        self.propagate(statement, "`__next__` can fail with", &others);
        element
    }
}
