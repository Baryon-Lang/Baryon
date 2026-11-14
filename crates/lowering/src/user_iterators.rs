//! `for` over a value of the program's own class (decision 0053, part 3).
//!
//! `for x in counter: body`, where the class of `counter` defines `__next__`, lowers to:
//!
//! ```text
//! header:  value = __next__(InOut counter)   ; error edge
//!          ok: x = value; body; goto header
//!          StopIteration: goto end
//!          any other error: fail, as any call's error does
//! end:
//! ```
//!
//! The loop calls `__next__` on the value itself: a variable or field in place, or a temporary
//! the statement drops after the loop. When the class defines `__iter__` and not `__next__`,
//! the loop first calls `__iter__` once, as the checker recorded at the `for` statement, and
//! loops over the iterator it returns, which the statement drops after the loop.

use crate::builder::Builder;
use crate::frames::erase;
use crate::loops::Pass;
use database::modules;
use mir::{Argument, Place, TerminatorKind};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Item, Nominal, Signature, Type, TypeKind};

impl<'db> Builder<'db> {
    /// `for target in iterator:` when `iterator` is a value of a class with `__next__` or
    /// `__iter__`: lowers the loop, with `pass` as each pass's body, and returns `true`.
    /// Returns `false`, doing nothing, for any other loop.
    pub(crate) fn user_loop(
        &mut self,
        statement: NodeId,
        target: NodeId,
        iterator: NodeId,
        pass: Pass<'_, 'db>,
    ) -> bool {
        let ty = self.ty(iterator);
        let iterable = matches!(
            self.typed.call(statement).map(|call| &call.callee),
            Some(types::Callee::Method(_))
        );
        let (holder, iterated) = if iterable {
            let Some((iter, signature)) = self.dunder(ty, "__iter__") else {
                return false;
            };
            let returned = match signature.returns.kind(self.db) {
                TypeKind::Fallible { value, .. } => *value,
                _ => signature.returns,
            };
            let iterated = self.instantiated(returned, ty, (iter, &signature));
            let holder = self.temporary(iterated, statement);
            self.call(Place::from(holder), statement);
            self.drop_later(holder);
            (Place::from(holder), iterated)
        } else if self.dunder(ty, "__next__").is_some() {
            (self.borrow(iterator), ty)
        } else {
            return false;
        };
        let Some((next, signature)) = self.dunder(iterated, "__next__") else {
            self.unsupported(iterator, "this iterator");
            return true;
        };
        let TypeKind::Fallible { value, errors } = signature.returns.kind(self.db) else {
            self.unsupported(iterator, "this iterator");
            return true;
        };
        let (element, errors) = (
            self.instantiated(*value, iterated, (next, &signature)),
            errors.clone(),
        );
        let Some(stop) = self.stop_iteration() else {
            self.unsupported(iterator, "this iterator");
            return true;
        };
        let type_arguments = match iterated.kind(self.db) {
            TypeKind::Struct(nominal) => nominal
                .arguments
                .iter()
                .map(|&argument| erase(self.db, argument))
                .collect(),
            _ => Vec::new(),
        };
        let header = self.new_block();
        self.terminate(TerminatorKind::Goto(header), statement);
        self.start(header);
        let value = self.temporary(element, iterator);
        let (body, end) = (self.new_block(), self.new_block());
        let edge = self.error_edge(&errors, statement);
        self.terminate(
            TerminatorKind::Call {
                callee: mir::Callee::Function {
                    item: next,
                    type_arguments,
                },
                arguments: vec![Argument::InOut(holder)],
                destination: Place::from(value),
                target: body,
                error: edge.clone(),
            },
            statement,
        );
        if let Some(edge) = edge {
            self.dispatch_except(&edge, &errors, statement, Some((stop, end)));
        }
        self.start(body);
        self.assign_part(target, Place::from(value), element);
        pass(self, header, end);
        self.start(end);
        true
    }

    /// `ty`, a type in the signature `signature` of `method`, a method of the class of
    /// `receiver`, with the class's type arguments in place of its type parameters.
    fn instantiated(
        &self,
        ty: Type<'db>,
        receiver: Type<'db>,
        (method, signature): (Item<'db>, &Signature<'db>),
    ) -> Type<'db> {
        let TypeKind::Struct(nominal) = receiver.kind(self.db) else {
            return ty;
        };
        let generic = self.type_parameters(method, signature);
        self.instantiate(ty, &generic, &nominal.arguments)
    }

    /// The library's `StopIteration`, the error that ends a loop over a class's `__next__`.
    fn stop_iteration(&self) -> Option<Type<'db>> {
        let file = modules(self.db, self.package).library("builtins")?;
        let tree = self.db.parse(file).tree();
        let text = file.source(self.db).text();
        let name = tree
            .children_with(tree.root(), Role::Body)
            .filter(|&statement| tree.kind(statement) == SyntaxKind::ClassDefinition)
            .filter_map(|class| tree.child(class, Role::Name))
            .find(|&class| tree.text(class, text) == "StopIteration")?;
        Some(Type::new(
            self.db,
            TypeKind::Struct(Nominal {
                file,
                name,
                arguments: Vec::new(),
            }),
        ))
    }
}
