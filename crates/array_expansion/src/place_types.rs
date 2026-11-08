//! The type of a place, worked out from its local's type and its projections.

use mir::{Place, Projection};
use types::{Declared, Nominal, Type, TypeKind};

/// What working out a place's type needs: the database, the body's locals and what each struct
/// declares.
pub(crate) struct Types<'a, 'db> {
    /// The database types are read from.
    pub(crate) db: &'db dyn salsa::Database,
    /// What each struct declares, with its type arguments substituted.
    pub(crate) declared: &'a dyn Fn(&Nominal<'db>) -> Declared<'db>,
}

impl<'db> Types<'_, 'db> {
    /// The type of the value at `place`, starting from `local`, the type of its local; `None` when
    /// a projection does not fit the type, which a type-checked body never contains.
    pub(crate) fn of(&self, local: Type<'db>, place: &Place) -> Option<Type<'db>> {
        let mut ty = local;
        for projection in &place.projections {
            ty = match (projection, ty.kind(self.db)) {
                (Projection::Field(position), TypeKind::Tuple(items)) => {
                    *items.get(*position as usize)?
                }
                (Projection::Field(position), TypeKind::Struct(nominal)) => {
                    match (self.declared)(nominal) {
                        Declared::Struct(fields) => *fields.get(*position as usize)?,
                        Declared::Enum { .. } => return None,
                    }
                }
                (
                    Projection::Index(_),
                    TypeKind::Array { element, .. }
                    | TypeKind::Fixed { element, .. }
                    | TypeKind::List(element),
                ) => *element,
                (Projection::Value, TypeKind::Optional(value)) => *value,
                (Projection::Error(position), TypeKind::Fallible { errors, .. }) => {
                    *errors.get(*position as usize)?
                }
                _ => return None,
            };
        }
        Some(ty)
    }

    /// What an array type is, for expansion: its element type and either its rank, for an
    /// `Array`, or its lengths, for a `Fixed`. `None` for any other type.
    pub(crate) fn array(&self, ty: Type<'db>) -> Option<(Type<'db>, Shape)> {
        match ty.kind(self.db) {
            TypeKind::Array { element, rank } => Some((*element, Shape::Runtime(*rank))),
            TypeKind::Fixed { element, shape } => {
                Some((*element, Shape::Fixed(shape.lengths().to_vec())))
            }
            _ => None,
        }
    }
}

/// The shape of an array operand as far as expansion needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    /// An `Array`: its rank; the lengths are read at run time.
    Runtime(u32),
    /// A `Fixed`: its lengths, known at compile time.
    Fixed(Vec<u64>),
}
