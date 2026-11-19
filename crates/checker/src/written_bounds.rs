//! Written type arguments checked against their parameters' bounds (decision 0066, roadmap
//! 1.9.63).
//!
//! A generic call infers its type arguments and checks each against its parameter's bound (the
//! `body::generics` module). A type argument can also be written, on a class: in an annotation,
//! `p: Pair[str]`, on a class function, `Pair[str].both("a")`, or on a constructor,
//! `Pair[str]("a", "b")`. Each is checked the same way, so `class Pair[T: Number]` rejects all
//! three:
//!
//! ```text
//! error: `Pair` needs `T` to be a `Number`, found `str`
//! ```
//!
//! The check runs as the lowerer lowers the class with its arguments, but only where every
//! declaration is already known: in a function body, and in [`check`], which reads each
//! definition's annotations again after its declaration is computed. While a declaration is
//! computed it cannot run, because checking a user trait reads the declaration of the struct
//! that implements it, which may be the one being computed, `class Node(Struct, Shape)` with
//! a field `child: Holder[Node]`.

use crate::lower::Lowerer;
use crate::outline::{Kind, outline};
use crate::traits::{bound, satisfies};
use database::{File, Package};
use diagnostics::Diagnostic;
use syntax::NodeId;
use types::{Type, TypeDb};

/// The errors in the written type arguments of every definition's annotations in `file`, a
/// file of `package`: its signature, fields, aliases and constants. A function body's own
/// annotations are checked with the body.
///
/// Cost: one more pass over each definition's annotations, after every declaration is cached.
pub(crate) fn check(db: &dyn TypeDb, package: Package, file: File) -> Vec<Diagnostic> {
    outline(db, file)
        .entries()
        .iter()
        .flat_map(|entry| {
            let class = match entry.kind {
                Kind::Class(_) => Some(entry.name),
                _ => entry.class,
            };
            let mut lowerer = Lowerer::new(db, package, file, class, entry.statement);
            lowerer.bounds = Bounds::Collected(Vec::new());
            lowerer.declaration(entry);
            match lowerer.bounds {
                Bounds::Collected(found) => found,
                Bounds::Unchecked | Bounds::Reported => Vec::new(),
            }
        })
        .collect()
}

/// Whether a lowerer checks the written type arguments it lowers, and where their errors go.
#[derive(Debug)]
pub(crate) enum Bounds {
    /// Not checked: while a declaration is computed.
    Unchecked,
    /// Checked, each error added to the lowerer's in the order found: in a function body.
    Reported,
    /// Checked, the errors kept apart from the lowerer's: in [`check`], which reads annotations
    /// whose other errors the declaration has already reported.
    Collected(Vec<Diagnostic>),
}

impl<'db> Lowerer<'db> {
    /// Reports each of `arguments`, written on the class `written` at `node`, that does not
    /// satisfy the bound of its parameter in `parameters`, declared in `file`, unless this
    /// lowerer leaves bounds unchecked.
    pub(crate) fn written_bounds(
        &mut self,
        node: NodeId,
        written: &str,
        file: File,
        parameters: &[NodeId],
        arguments: &[Type<'db>],
    ) {
        if matches!(self.bounds, Bounds::Unchecked) {
            return;
        }
        for (&parameter, &ty) in parameters.iter().zip(arguments) {
            let Some(bound) = bound(self.db, self.package, file, parameter) else {
                continue;
            };
            if satisfies(self.db, self.package, bound, ty) {
                continue;
            }
            let name = self
                .db
                .parse(file)
                .tree()
                .text(parameter, file.source(self.db).text());
            let message = format!(
                "`{written}` needs `{name}` to be a `{}`, found `{}`",
                bound.name(self.db),
                ty.display(self.db)
            );
            self.error(node, &message, "bound not satisfied", None);
            if let Bounds::Collected(found) = &mut self.bounds
                && let Some(error) = self.diagnostics.pop()
            {
                found.push(error);
            }
        }
    }
}
