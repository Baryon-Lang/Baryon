//! Writing a type the way a program spells it, for diagnostics and hovers.

use crate::{Type, TypeKind};
use database::{Db, File};
use std::fmt;
use syntax::NodeId;

impl<'db> Type<'db> {
    /// A value that writes the type as a program spells it: `Array[f64, 3]`,
    /// `Fixed[f64, (3, 3)]`, `f64 | None`, `Callable[[f64], f64]`, or a class's name.
    ///
    /// The database is needed to read class and type parameter names from their files.
    #[must_use]
    pub fn display(self, db: &'db dyn Db) -> Displayed<'db> {
        Displayed { db, ty: self }
    }
}

/// A type ready to write. Created by [`Type::display`].
#[derive(Clone, Copy)]
pub struct Displayed<'db> {
    /// The database the type and its names live in.
    db: &'db dyn Db,
    /// The type to write.
    ty: Type<'db>,
}

impl fmt::Debug for Displayed<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for Displayed<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let db = self.db;
        let show = |ty: Type<'_>| ty.display(db).to_string();
        let list = |types: &[Type<'_>]| types.iter().map(|&ty| show(ty)).collect::<Vec<_>>();
        match self.ty.kind(db) {
            TypeKind::Primitive(primitive) => write!(formatter, "{primitive}"),
            TypeKind::Str => formatter.write_str("str"),
            TypeKind::None => formatter.write_str("None"),
            TypeKind::Tuple(items) => write!(formatter, "tuple[{}]", list(items).join(", ")),
            TypeKind::List(item) => write!(formatter, "list[{}]", show(*item)),
            TypeKind::Optional(value) => write!(formatter, "{} | None", show(*value)),
            TypeKind::Array { element, rank } => {
                write!(formatter, "Array[{}, {rank}]", show(*element))
            }
            TypeKind::Fixed { element, shape } => {
                write!(formatter, "Fixed[{}, {shape}]", show(*element))
            }
            TypeKind::Struct(nominal) | TypeKind::Enum(nominal) => {
                write!(formatter, "{}", name(db, nominal.file, nominal.name))?;
                arguments(formatter, &list(&nominal.arguments))
            }
            TypeKind::Callable {
                parameters,
                returns,
            } => write!(
                formatter,
                "Callable[[{}], {}]",
                list(parameters).join(", "),
                show(*returns)
            ),
            TypeKind::Fallible { value, errors } => {
                write!(
                    formatter,
                    "Raises[{}, {}]",
                    show(*value),
                    list(errors).join(" | ")
                )
            }
            TypeKind::Parameter { file, name: node } => formatter.write_str(name(db, *file, *node)),
            TypeKind::Unknown => formatter.write_str("unknown"),
        }
    }
}

/// Writes `[A, B]` after a generic class's name, or nothing when there are no arguments.
fn arguments(formatter: &mut fmt::Formatter<'_>, arguments: &[String]) -> fmt::Result {
    if arguments.is_empty() {
        Ok(())
    } else {
        write!(formatter, "[{}]", arguments.join(", "))
    }
}

/// The text of the `Name` node `node` in `file`.
fn name(db: &dyn Db, file: File, node: NodeId) -> &str {
    db.parse(file).tree().text(node, file.source(db).text())
}
