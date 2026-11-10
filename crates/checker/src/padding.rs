//! The padding warning: a struct whose fields, reordered, would take fewer bytes.
//!
//! The compiler never reorders a struct's fields. Each field sits at the next offset its
//! alignment allows, in the order the author wrote them, so an array of structs can be shared
//! with C, a `numpy` structured array or a file format byte for byte (language reference,
//! "Types"). The price is padding: in
//!
//! ```python
//! class Particle(Struct):
//!     alive: bool
//!     mass: f64
//!     charge: f32
//! ```
//!
//! `mass` must start at a multiple of 8, so 7 bytes of padding follow `alive`, and 4 more round
//! the struct up to 24 bytes. In the order `mass, charge, alive` it takes 16. A million particles
//! is then 8 MB of memory and memory bandwidth spent on nothing.
//!
//! This module finds such structs and warns at the class name, naming an order that takes the
//! fewest bytes: the fields sorted by alignment, largest first, ties kept in the order written.
//! Every alignment is a power of two and every size a multiple of its alignment, so in that order
//! each field starts exactly where the previous one ends and only the trailing padding is left,
//! which no order can avoid.
//!
//! It checks every struct and error type that has no type parameters. A generic struct has no
//! layout until it is instantiated; its fields are checked wherever a non-generic struct holds an
//! instance, through that struct's own size. A struct whose layout cannot be computed, because a
//! field's type is unknown from an earlier error or the struct contains itself, gets no warning:
//! that error is reported elsewhere.
//!
//! A struct decorated `@layout("C")` gets no warning: its author has said the order is fixed on
//! purpose, to match a file format or a C library (decision 0017).

use crate::outline::{Kind, outline};
use database::{File, Package};
use diagnostics::Diagnostic;
use std::cmp::Reverse;
use syntax::{NodeId, SyntaxTree};
use types::{Declaration, Declared, Item, Layout, Nominal, Type, TypeDb, TypeKind};

/// A warning for each non-generic struct in `file`, a file of `package`, whose fields would take
/// fewer bytes in another order, in source order. A struct decorated `@layout("C")` gets none.
///
/// Cost: one layout per field of each struct, which visits every type nested in the field. A
/// struct's declaration and those of the classes its fields name are read through the cached
/// [`TypeDb::declaration`] query.
pub(crate) fn padding(db: &dyn TypeDb, package: Package, file: File) -> Vec<Diagnostic> {
    let tree = db.parse(file).tree();
    let text = file.source(db).text();
    outline(db, file)
        .entries()
        .iter()
        .filter(|entry| {
            matches!(&entry.kind, Kind::Class(class)
                if class.type_parameters.is_empty() && !class.c_layout)
        })
        .filter_map(|entry| {
            let item = Item::new(db, package, file, entry.name);
            let Declaration::Struct { fields, .. } = &db.declaration(item).declaration else {
                return None;
            };
            let types: Vec<Type<'_>> = fields.iter().map(|field| field.ty).collect();
            let names: Vec<&str> = fields.iter().map(|field| field.name.as_str()).collect();
            let saving = Saving::find(db, package, &types)?;
            Some(saving.warning(tree, text, entry.name, &names))
        })
        .collect()
}

/// How much smaller a struct would be with its fields in another order.
struct Saving {
    /// The struct's size in bytes, fields in the order written.
    declared: u64,
    /// Its size in bytes, fields in [`order`](Self::order).
    reordered: u64,
    /// The index of each field, in the order that takes the fewest bytes.
    order: Vec<usize>,
}

impl Saving {
    /// The saving for a struct whose fields have the types `fields`, in declaration order, or
    /// `None` if no order saves a byte or a field has no layout.
    fn find<'db>(db: &'db dyn TypeDb, package: Package, fields: &[Type<'db>]) -> Option<Self> {
        let declared = |nominal: &Nominal<'db>| declared(db, package, nominal);
        let layouts: Vec<Layout> = fields
            .iter()
            .map(|&ty| types::layout(db, ty, &declared))
            .collect::<Result<_, _>>()
            .ok()?;
        let mut order: Vec<usize> = (0..layouts.len()).collect();
        order.sort_by_key(|&index| Reverse(layouts[index].align()));
        let declared = Layout::record(layouts.iter().copied())?.layout().size();
        let reordered = Layout::record(order.iter().map(|&index| layouts[index]))?
            .layout()
            .size();
        (reordered < declared).then_some(Self {
            declared,
            reordered,
            order,
        })
    }

    /// The warning for the struct named by the `Name` node `class` in `tree`, whose source text
    /// is `text`, and whose fields are `names`, in declaration order. It points at the name.
    fn warning(&self, tree: &SyntaxTree, text: &str, class: NodeId, names: &[&str]) -> Diagnostic {
        let span = tree.span(class);
        let class = tree.text(class, text);
        let order: Vec<&str> = self.order.iter().map(|&index| names[index]).collect();
        Diagnostic::warning(format!(
            "reordering the fields of `{class}` would save {} bytes",
            self.declared - self.reordered
        ))
        .with_primary(
            span,
            format!(
                "{} bytes as declared, {} reordered",
                self.declared, self.reordered
            ),
        )
        .with_note(format!(
            "declare the fields in the order `{}`; the compiler keeps the order written, so the \
             layout matches C and `numpy`",
            order.join(", ")
        ))
        .with_note(
            "if the order is fixed by a file format or a C library, decorate the struct \
             `@layout(\"C\")` to keep it without this warning",
        )
    }
}

/// What the class `nominal` declares, as [`types::layout()`] needs it: a struct's field types
/// with its type arguments substituted, or an enum's number of variants.
pub(crate) fn declared<'db>(
    db: &'db dyn TypeDb,
    package: Package,
    nominal: &Nominal<'db>,
) -> Declared<'db> {
    let item = Item::new(db, package, nominal.file, nominal.name);
    match &db.declaration(item).declaration {
        Declaration::Struct { fields, .. } => {
            let parameters = match outline(db, nominal.file).get(nominal.name) {
                Some(entry) => match &entry.kind {
                    Kind::Class(class) => class.type_parameters.clone(),
                    _ => Vec::new(),
                },
                None => Vec::new(),
            };
            let substitution = Substitution {
                file: nominal.file,
                parameters: &parameters,
                arguments: &nominal.arguments,
            };
            Declared::Struct(
                fields
                    .iter()
                    .map(|field| substitution.apply(db, field.ty))
                    .collect(),
            )
        }
        Declaration::Enum { variants } => Declared::Enum {
            variants: u64::try_from(variants.len()).unwrap_or(u64::MAX),
        },
        // A class type always names a struct or an enum; anything else has no layout, which an
        // unknown field says without a further error.
        _ => Declared::Struct(vec![Type::new(db, TypeKind::Unknown)]),
    }
}

/// Replaces a generic class's type parameters with the type arguments of one instance:
/// `first: T` in `Pair[T]` becomes `first: f64` in `Pair[f64]`.
struct Substitution<'a, 'db> {
    /// The file that declares the class, and so its type parameters.
    file: File,
    /// The `Name` node of each type parameter, in order.
    parameters: &'a [NodeId],
    /// The type arguments, in the same order.
    arguments: &'a [Type<'db>],
}

impl<'db> Substitution<'_, 'db> {
    /// `ty` with every type parameter of the class replaced by its argument.
    fn apply(&self, db: &'db dyn TypeDb, ty: Type<'db>) -> Type<'db> {
        let all = |types: &[Type<'db>]| -> Vec<Type<'db>> {
            types.iter().map(|&ty| self.apply(db, ty)).collect()
        };
        let nominal = |nominal: &Nominal<'db>| Nominal {
            file: nominal.file,
            name: nominal.name,
            arguments: all(&nominal.arguments),
        };
        let kind = match ty.kind(db) {
            TypeKind::Parameter { file, name } if *file == self.file => {
                let index = self
                    .parameters
                    .iter()
                    .position(|parameter| parameter == name);
                return index
                    .and_then(|index| self.arguments.get(index).copied())
                    .unwrap_or(ty);
            }
            TypeKind::Tuple(items) => TypeKind::Tuple(all(items)),
            TypeKind::List(item) => TypeKind::List(self.apply(db, *item)),
            TypeKind::Dict { key, value } => TypeKind::Dict {
                key: self.apply(db, *key),
                value: self.apply(db, *value),
            },
            TypeKind::Optional(value) => TypeKind::Optional(self.apply(db, *value)),
            TypeKind::Array { element, rank } => TypeKind::Array {
                element: self.apply(db, *element),
                rank: *rank,
            },
            TypeKind::Fixed { element, shape } => TypeKind::Fixed {
                element: self.apply(db, *element),
                shape: shape.clone(),
            },
            TypeKind::Struct(class) => TypeKind::Struct(nominal(class)),
            TypeKind::Enum(class) => TypeKind::Enum(nominal(class)),
            TypeKind::Callable {
                parameters,
                returns,
            } => TypeKind::Callable {
                parameters: all(parameters),
                returns: self.apply(db, *returns),
            },
            TypeKind::Fallible { value, errors } => TypeKind::Fallible {
                value: self.apply(db, *value),
                errors: all(errors),
            },
            TypeKind::Primitive(_)
            | TypeKind::Str
            | TypeKind::None
            | TypeKind::Parameter { .. }
            | TypeKind::Function { .. }
            | TypeKind::Standard(_)
            | TypeKind::Unknown => return ty,
        };
        Type::new(db, kind)
    }
}
