//! Arrays a `prange` loop adds into (language reference, "Parallel and vector loops", "Adding
//! into shared arrays"; decision 0013). `prange(n, accumulate=forces)` lets every iteration
//! add into any element of `forces`, because each chunk of iterations adds into its own
//! private blocks, combined in a fixed order after the loop. So inside the loop an array
//! listed there:
//!
//! - is only added to: it appears only as the target of `+=` or `-=`, at any index;
//! - appears nowhere else: it is never read, assigned or passed to a function;
//! - holds numbers, or `Fixed` arrays of them, which can be added and have a zero.
//!
//! Listing an array the loop never adds to is an error, so the list cannot go stale.

use super::Body;
use super::calls::Arguments;
use super::numbers;
use super::parallel::{Access, Parallel, Reduction, Usage, overlap};
use diagnostics::Diagnostic;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

impl<'db> Body<'db> {
    /// Checks the keyword arguments of the `prange` call: only `accumulate=`, listing arrays
    /// of numbers by name. Types each value, and keeps the valid entries for the loop.
    pub(super) fn prange_keywords(&mut self, arguments: &Arguments) {
        self.accumulate.clear();
        for &(name, value) in &arguments.keywords {
            let written = self.tree.text(name, self.text);
            if written != "accumulate" {
                self.expression(value, None);
                let message = format!("`prange` has no parameter `{written}`");
                self.lowerer.error(
                    name,
                    &message,
                    "unknown parameter",
                    Some("`prange` takes the arguments of `range`, and `accumulate=`"),
                );
                continue;
            }
            // A tuple here lists arrays; it builds nothing, so nothing moves into it.
            let items: Vec<NodeId> = if self.tree.kind(value) == SyntaxKind::Tuple {
                self.tree.children_with(value, Role::Item).collect()
            } else {
                vec![value]
            };
            for item in items {
                let ty = self.expression(item, None);
                let named = self
                    .path(item)
                    .is_some_and(|(_, path)| path.iter().all(Option::is_some));
                if !named {
                    self.lowerer.error(
                        item,
                        "`accumulate=` lists arrays by name",
                        "not a variable or field",
                        Some("name the arrays the loop adds into: `accumulate=forces` or `accumulate=(forces, virial)`"),
                    );
                } else if !self.addable(ty) && !self.is_open(ty) {
                    let message = format!(
                        "`accumulate=` needs an array of numbers, found `{}`",
                        self.show(ty)
                    );
                    self.lowerer.error(
                        item,
                        &message,
                        "cannot be added into",
                        Some("an accumulated array holds integers, floats, complex numbers, or `Fixed` arrays of them"),
                    );
                } else {
                    self.accumulate.push(item);
                }
            }
        }
    }

    /// Whether `ty` is an array whose elements can be added and have a zero.
    fn addable(&self, ty: Type<'db>) -> bool {
        let number = |ty: Type<'db>| matches!(ty.kind(self.db), TypeKind::Primitive(primitive) if numbers::is_number(*primitive));
        match ty.kind(self.db) {
            TypeKind::Array { element, .. } | TypeKind::Fixed { element, .. } => {
                number(*element)
                    || matches!(element.kind(self.db), TypeKind::Fixed { element, .. } if number(*element))
            }
            _ => false,
        }
    }

    /// Whether `ty` holds an unknown part, whose error was already reported.
    fn is_open(&self, ty: Type<'db>) -> bool {
        super::relations::is_open(self.db, ty)
    }

    /// Checks the uses of each array the loop `parallel` lists in `accumulate=`, and returns
    /// the other uses, which the rules for shared variables check.
    pub(super) fn accumulated(
        &mut self,
        parallel: &Parallel,
        accesses: Vec<Access>,
    ) -> Vec<Access> {
        let mut rest = accesses;
        for &listed in &parallel.accumulate {
            let Some((binding, path)) = self.path(listed) else {
                continue;
            };
            let (members, others): (Vec<Access>, Vec<Access>) = rest
                .into_iter()
                .partition(|access| access.binding == binding && overlap(&access.path, &path));
            rest = others;
            let name = self.tree.text(listed, self.text);
            let mut adds = false;
            for access in &members {
                if access.usage == Usage::Reduce(Reduction::Sum) {
                    adds = true;
                    continue;
                }
                let label = if access.usage == Usage::Read {
                    "read here"
                } else {
                    "not an addition"
                };
                let diagnostic = Diagnostic::error(format!(
                    "`{name}` is listed in `accumulate=`, so this loop may only add to it"
                ))
                .with_primary(self.tree.span(access.node), label)
                .with_secondary(self.tree.span(listed), "listed here")
                .with_note(
                    "an accumulated array appears only as the target of `+=` or `-=`; a helper function cannot add into it, so return the contribution and add it in the loop",
                );
                self.lowerer.diagnostics.push(diagnostic);
            }
            if !adds {
                let message =
                    format!("`{name}` is listed in `accumulate=`, but this loop never adds to it");
                self.lowerer.error(
                    listed,
                    &message,
                    "never added to",
                    Some("remove it from `accumulate=`"),
                );
            }
        }
        rest
    }
}
