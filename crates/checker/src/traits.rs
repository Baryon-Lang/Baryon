//! Traits, which bound a type parameter (language reference, "Types", "Traits"): the standard
//! traits `Number`, `Float`, `VectorSpace` and `Copy`, and the traits user code declares as
//! `class Body(Protocol)` (decision 0031).
//!
//! A bound is read from the syntax tree, `T: Number` in `def f[T: Number](x: T)`, rather than
//! stored in the type, so a `Type` stays a small interned value. A user trait is named by its
//! class's `Name` node; what it requires and which methods it lists come from its
//! [`Declaration::Trait`].
//!
//! A type has a user trait only by naming it, or a trait that requires it, among its bases
//! (nominal, decision 0031). A type parameter bounded by a trait has that trait and every
//! trait it requires.

use crate::outline::{Base, Kind, outline};
use database::{Definition, File, Package};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Declaration, Item, TypeDb, TypeKind};

/// A trait a type parameter may be bounded by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Trait {
    /// `Number`: every number type.
    Number,
    /// `Float`: `f32` and `f64`.
    Float,
    /// `VectorSpace`: `f64` and arrays of `f64`.
    VectorSpace,
    /// `Copy`: the copy types.
    Copy,
    /// A trait user code declares, `class Body(Protocol)`.
    User(UserTrait),
}

/// A trait user code declares: the class `class Body(Protocol)`, by its `Name` node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UserTrait {
    /// The file that declares it.
    pub(crate) file: File,
    /// The class's `Name` node there.
    pub(crate) name: NodeId,
}

impl UserTrait {
    /// The trait's name as written.
    pub(crate) fn name(self, db: &dyn TypeDb) -> String {
        db.parse(self.file)
            .tree()
            .text(self.name, self.file.source(db).text())
            .to_owned()
    }

    /// The trait's class as an item of `package`, to read its declaration.
    pub(crate) fn item(self, db: &dyn TypeDb, package: Package) -> Item<'_> {
        Item::new(db, package, self.file, self.name)
    }

    /// The trait of the class `item`, if it is a trait.
    pub(crate) fn of_item(db: &dyn TypeDb, item: Item<'_>) -> Self {
        Self {
            file: item.file(db),
            name: item.name(db),
        }
    }

    /// The methods the trait lists itself, not those of the traits it requires.
    pub(crate) fn own_methods(self, db: &dyn TypeDb, package: Package) -> Vec<Item<'_>> {
        match &db.declaration(self.item(db, package)).declaration {
            Declaration::Trait { methods, .. } => methods.clone(),
            _ => Vec::new(),
        }
    }
}

impl Trait {
    /// The standard trait a builtin name means, if it is one.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "Number" => Self::Number,
            "Float" => Self::Float,
            "VectorSpace" => Self::VectorSpace,
            "Copy" => Self::Copy,
            _ => return None,
        })
    }

    /// The trait's name as a program writes it.
    pub(crate) fn name(self, db: &dyn TypeDb) -> String {
        match self {
            Self::Number => "Number".to_owned(),
            Self::Float => "Float".to_owned(),
            Self::VectorSpace => "VectorSpace".to_owned(),
            Self::Copy => "Copy".to_owned(),
            Self::User(user) => user.name(db),
        }
    }

    /// Whether the trait allows arithmetic and ordering with number literals.
    pub(crate) fn is_numeric(self) -> bool {
        matches!(self, Self::Number | Self::Float)
    }
}

/// The bound of the type parameter whose `Name` node is `name` in `file`, a file of `package`:
/// the trait after `:` in `def f[T: Number]` or `class Pair[T: Number]`.
pub(crate) fn bound(db: &dyn TypeDb, package: Package, file: File, name: NodeId) -> Option<Trait> {
    match written_bound(db, package, file, name)? {
        Bound::Trait(found) => Some(found),
        Bound::Other => None,
    }
}

/// What a type parameter's bound, as written, names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Bound {
    /// A standard or user trait.
    Trait(Trait),
    /// Anything else, which declaring the parameter reports. Checks treat it as no constraint
    /// at all, so the one error does not cascade.
    Other,
}

/// The bound written on the type parameter whose `Name` node is `name` in `file`, a file of
/// `package`, or `None` when it has none.
pub(crate) fn written_bound(
    db: &dyn TypeDb,
    package: Package,
    file: File,
    name: NodeId,
) -> Option<Bound> {
    let tree = db.parse(file).tree();
    let text = file.source(db).text();
    outline(db, file).entries().iter().find_map(|entry| {
        let parameters = tree.child(entry.statement, Role::TypeParameters)?;
        let variable = tree
            .children_with(parameters, Role::Item)
            .find(|&variable| tree.child(variable, Role::Name) == Some(name))?;
        let bound = tree.child(variable, Role::Bound)?;
        let standard = (tree.kind(bound) == SyntaxKind::Name)
            .then(|| Trait::from_name(tree.text(bound, text)))
            .flatten();
        let found = standard.or_else(|| user_trait(db, package, file, bound).map(Trait::User));
        Some(found.map_or(Bound::Other, Bound::Trait))
    })
}

/// The user trait the name or attribute `node`, in `file`, refers to, if it names a class with
/// the base `Protocol`.
pub(crate) fn user_trait(
    db: &dyn TypeDb,
    package: Package,
    file: File,
    node: NodeId,
) -> Option<UserTrait> {
    let tree = db.parse(file).tree();
    let name = match tree.kind(node) {
        SyntaxKind::Name => node,
        SyntaxKind::Attribute => tree.child(node, Role::Attribute)?,
        _ => return None,
    };
    let &Definition::Item(declared, class) = db.resolve(package, file).definition(name)? else {
        return None;
    };
    is_trait(db, declared, class).then_some(UserTrait {
        file: declared,
        name: class,
    })
}

/// Whether the definition whose `Name` node is `name` in `file` is a trait.
pub(crate) fn is_trait(db: &dyn TypeDb, file: File, name: NodeId) -> bool {
    matches!(
        outline(db, file).get(name).map(|entry| &entry.kind),
        Some(Kind::Class(class)) if class.base == Base::Protocol
    )
}

/// `traits` and every trait they require, directly or through others, each once, in the order
/// found: the traits themselves first.
///
/// Cost: one cached declaration per trait reached. A trait that requires itself, which the
/// class check reports, ends the walk rather than looping.
pub(crate) fn closure(
    db: &dyn TypeDb,
    package: Package,
    traits: impl IntoIterator<Item = UserTrait>,
) -> Vec<UserTrait> {
    let mut found: Vec<UserTrait> = Vec::new();
    let mut pending: Vec<UserTrait> = traits.into_iter().collect();
    pending.reverse();
    while let Some(next) = pending.pop() {
        if found.contains(&next) {
            continue;
        }
        found.push(next);
        if let Declaration::Trait { requires, .. } =
            &db.declaration(next.item(db, package)).declaration
        {
            pending.extend(
                requires
                    .iter()
                    .rev()
                    .map(|&item| UserTrait::of_item(db, item)),
            );
        }
    }
    found
}

/// The user traits a value of `ty` has: for a struct, those its class names and all they
/// require; for a type parameter, its bound and all it requires. Empty for any other type.
pub(crate) fn implemented<'db>(
    db: &'db dyn TypeDb,
    package: Package,
    ty: types::Type<'db>,
) -> Vec<UserTrait> {
    match ty.kind(db) {
        TypeKind::Struct(nominal) => {
            let item = Item::new(db, package, nominal.file, nominal.name);
            match &db.declaration(item).declaration {
                Declaration::Struct { traits, .. } => closure(
                    db,
                    package,
                    traits.iter().map(|&item| UserTrait::of_item(db, item)),
                ),
                _ => Vec::new(),
            }
        }
        TypeKind::Parameter { file, name } => match bound(db, package, *file, *name) {
            Some(Trait::User(user)) => closure(db, package, [user]),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// The method named `wanted` that one of `traits` lists, with the trait that lists it, so its
/// `Self` can be replaced. The first trait in order wins; two traits listing one name is
/// reported where they are combined.
pub(crate) fn find_method<'db>(
    db: &'db dyn TypeDb,
    package: Package,
    traits: &[UserTrait],
    wanted: &str,
) -> Option<(UserTrait, Item<'db>)> {
    traits.iter().find_map(|&user| {
        let tree = db.parse(user.file).tree();
        let text = user.file.source(db).text();
        user.own_methods(db, package)
            .into_iter()
            .find(|method| tree.text(method.name(db), text) == wanted)
            .map(|method| (user, method))
    })
}
