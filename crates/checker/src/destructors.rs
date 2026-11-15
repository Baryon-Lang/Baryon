//! A class's `__del__`, which runs when a value of the class is dropped (decision 0063).
//!
//! The checker finds it, checks its shape where the class is declared, and tells the passes
//! after lowering which classes have one: drop elaboration keeps the drop of such a value even
//! when its fields own nothing, and the backend calls the method before it drops the fields.
//! Calling `__del__` by name, and moving a field out of a value whose class has it, are reported
//! where they are written, in the `body` module.

use crate::embed::{self, Own};
use crate::outline::{Base, Kind, outline};
use database::{File, Package};
use diagnostics::Diagnostic;
use syntax::{Role, SyntaxKind};
use types::{Declaration, Item, Mode, Nominal, TypeDb, TypeKind};

/// The `__del__` method the struct `nominal`, a class of `package`, defines itself, whatever
/// its shape; `None` when it defines none or is not a struct.
pub(crate) fn own_destructor<'db>(
    db: &'db dyn TypeDb,
    package: Package,
    nominal: &Nominal<'db>,
) -> Option<Item<'db>> {
    match embed::own(db, package, nominal, "__del__")? {
        Own::Method { item, .. } => Some(item),
        Own::Field(_) => None,
    }
}

/// The `__del__` the backend calls when a value of the struct `nominal` is dropped, and
/// whether it can fail: the method of decision 0063's shape that the class defines itself.
/// `None` when it has none, or one the checker reports, so a program with that error is never
/// built.
#[must_use]
pub fn destructor<'db>(
    db: &'db dyn TypeDb,
    package: Package,
    nominal: &Nominal<'db>,
) -> Option<(Item<'db>, bool)> {
    let item = own_destructor(db, package, nominal)?;
    let class = outline(db, nominal.file).get(nominal.name)?;
    let Kind::Class(class) = &class.kind else {
        return None;
    };
    if class.copy || !class.type_parameters.is_empty() {
        return None;
    }
    shape(db, item).ok()
}

/// Whether the method `item` has `__del__`'s shape, `self: InOut[Self]` and nothing else,
/// returning `None` or `Raises[None, E]`: the method and whether it can fail, or `Err` when
/// it has another shape.
fn shape<'db>(db: &'db dyn TypeDb, item: Item<'db>) -> Result<(Item<'db>, bool), ()> {
    let Declaration::Function(signature) = &db.declaration(item).declaration else {
        return Err(());
    };
    let [only] = signature.parameters.as_slice() else {
        return Err(());
    };
    if only.mode != Mode::InOut || !signature.type_parameters.is_empty() {
        return Err(());
    }
    match signature.returns.kind(db) {
        TypeKind::None => Ok((item, false)),
        TypeKind::Fallible { value, .. } if matches!(value.kind(db), TypeKind::None) => {
            Ok((item, true))
        }
        _ => Err(()),
    }
}

/// The errors of each `__del__` in `file`, a file of `package`: one with another shape, one on
/// a class declared `Copy`, and one on a generic class, which is not built yet.
pub(crate) fn check(db: &dyn TypeDb, package: Package, file: File) -> Vec<Diagnostic> {
    let tree = db.parse(file).tree();
    let text = file.source(db).text();
    let mut diagnostics = Vec::new();
    for entry in outline(db, file).entries() {
        let Kind::Class(class) = &entry.kind else {
            continue;
        };
        if !matches!(class.base, Base::Struct | Base::Error) {
            continue;
        }
        let definition = tree
            .children_with(entry.statement, Role::Body)
            .filter(|&statement| tree.kind(statement) == SyntaxKind::FunctionDefinition)
            .find(|&method| {
                tree.child(method, Role::Name)
                    .is_some_and(|name| tree.text(name, text) == "__del__")
            });
        let Some(definition) = definition else {
            continue;
        };
        let name = tree.child(definition, Role::Name).unwrap_or(definition);
        let span = tree.span(name);
        let written = tree.text(entry.name, text);
        let nominal = Nominal {
            file,
            name: entry.name,
            arguments: Vec::new(),
        };
        let item = own_destructor(db, package, &nominal);
        if item.is_none_or(|item| shape(db, item).is_err()) {
            diagnostics.push(
                Diagnostic::error(format!(
                    "`{written}.__del__` must take only `self: InOut[Self]` and return `None`"
                ))
                .with_primary(span, "runs when the value is dropped")
                .with_note(
                    "write `def __del__(self: InOut[Self]):`, or `-> Raises[None, E]` when it \
                     can fail; an error it raises panics, since a drop cannot raise \
                     (decision 0063)",
                ),
            );
        } else if class.copy {
            diagnostics.push(
                Diagnostic::error(format!(
                    "`{written}` is declared `Copy`, so it cannot have `__del__`"
                ))
                .with_primary(span, "never runs")
                .with_note(
                    "a copy type is copied, never dropped; remove `Copy` to give the class a \
                     `__del__` (decision 0063)",
                ),
            );
        } else if !class.type_parameters.is_empty() {
            diagnostics.push(
                Diagnostic::error(format!(
                    "`__del__` on a generic class, `{written}`, is not built yet"
                ))
                .with_primary(span, "not built yet")
                .with_note(
                    "give the class a method such as `close()` and call it, or use `with` \
                     (decision 0063)",
                ),
            );
        }
    }
    diagnostics
}
