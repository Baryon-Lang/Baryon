//! Names reached through embedded fields (decision 0031): `ion.m` reads `ion.particle.m` when
//! `Ion` declares `particle: Embed[Particle]`.
//!
//! A name is looked for on the struct itself first, then on the structs it embeds, then on the
//! structs those embed, one depth at a time. The first depth that has the name decides: a name
//! the struct defines itself hides an embedded one, and two embedded structs that both provide
//! it at that depth are ambiguous, which the class check reports at the class. Embedding costs
//! nothing at run time: the path found becomes field projections, the same offsets as writing
//! `ion.particle.m`.

use crate::body::relations;
use crate::outline::{Kind, outline};
use database::{File, Package};
use syntax::NodeId;
use types::{Declaration, Item, Nominal, Type, TypeDb, TypeKind};

/// A field or method of one struct, found by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Own<'db> {
    /// A field, with the struct's type arguments substituted into its type.
    Field(Type<'db>),
    /// A method, with the struct's class's type parameters and its type arguments, to
    /// substitute into the method's signature.
    Method {
        /// The method.
        item: Item<'db>,
        /// The `Name` nodes of the class's type parameters.
        parameters: Vec<NodeId>,
        /// The struct's type arguments.
        arguments: Vec<Type<'db>>,
    },
}

/// The result of looking a name up on a struct and the structs it embeds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Lookup<'db> {
    /// Found once at the shallowest depth that has it.
    Found {
        /// The position of each embedded field passed through, outermost first; empty for the
        /// struct's own name.
        path: Vec<u32>,
        /// The struct that has the name, with its type arguments: the outer struct itself, or
        /// the one at the end of `path`.
        owner: Nominal<'db>,
        /// What the name is there.
        member: Own<'db>,
    },
    /// Provided by two or more embedded structs at the same depth, each reached by its path.
    Ambiguous(Vec<Vec<u32>>),
    /// Neither the struct nor any struct it embeds has the name.
    Missing,
}

/// The name `wanted` on the struct `nominal` of `package`, or on a struct it embeds.
///
/// Cost: one cached declaration per struct visited; each depth is searched once, and a struct
/// reached twice, through an embedding cycle that the class check reports, is not searched
/// again.
pub(crate) fn lookup<'db>(
    db: &'db dyn TypeDb,
    package: Package,
    nominal: &Nominal<'db>,
    wanted: &str,
) -> Lookup<'db> {
    let mut level: Vec<(Vec<u32>, Nominal<'db>)> = vec![(Vec::new(), nominal.clone())];
    let mut visited: Vec<(File, NodeId)> = Vec::new();
    while !level.is_empty() {
        let mut hits: Vec<(Vec<u32>, Nominal<'db>, Own<'db>)> = Vec::new();
        let mut next = Vec::new();
        for (path, current) in level {
            if visited.contains(&(current.file, current.name)) {
                continue;
            }
            visited.push((current.file, current.name));
            if let Some(member) = own(db, package, &current, wanted) {
                hits.push((path.clone(), current.clone(), member));
            }
            for (index, inner) in embedded(db, package, &current) {
                let mut deeper = path.clone();
                deeper.push(index);
                next.push((deeper, inner));
            }
        }
        match hits.len() {
            0 => level = next,
            1 => {
                let Some((path, owner, member)) = hits.pop() else {
                    return Lookup::Missing;
                };
                return Lookup::Found {
                    path,
                    owner,
                    member,
                };
            }
            _ => return Lookup::Ambiguous(hits.into_iter().map(|(path, ..)| path).collect()),
        }
    }
    Lookup::Missing
}

/// The field or method named `wanted` that the struct `nominal` declares itself.
pub(crate) fn own<'db>(
    db: &'db dyn TypeDb,
    package: Package,
    nominal: &Nominal<'db>,
    wanted: &str,
) -> Option<Own<'db>> {
    let item = Item::new(db, package, nominal.file, nominal.name);
    let parameters = class_parameters(db, nominal);
    let Declaration::Struct {
        fields, methods, ..
    } = &db.declaration(item).declaration
    else {
        return None;
    };
    if let Some(field) = fields.iter().find(|field| field.name == wanted) {
        let ty = relations::substitute(db, field.ty, nominal.file, &parameters, &nominal.arguments);
        return Some(Own::Field(ty));
    }
    let tree = db.parse(nominal.file).tree();
    let text = nominal.file.source(db).text();
    methods
        .iter()
        .find(|method| tree.text(method.name(db), text) == wanted)
        .map(|&method| Own::Method {
            item: method,
            parameters,
            arguments: nominal.arguments.clone(),
        })
}

/// Every name the struct `nominal` declares itself: its fields, then its methods.
pub(crate) fn own_names(db: &dyn TypeDb, package: Package, nominal: &Nominal<'_>) -> Vec<String> {
    let item = Item::new(db, package, nominal.file, nominal.name);
    let Declaration::Struct {
        fields, methods, ..
    } = &db.declaration(item).declaration
    else {
        return Vec::new();
    };
    let tree = db.parse(nominal.file).tree();
    let text = nominal.file.source(db).text();
    fields
        .iter()
        .map(|field| field.name.clone())
        .chain(
            methods
                .iter()
                .map(|method| tree.text(method.name(db), text).to_owned()),
        )
        .collect()
}

/// The embedded fields of the struct `nominal`, each with its position among the fields and
/// the embedded struct's type, the outer struct's type arguments substituted.
pub(crate) fn embedded<'db>(
    db: &'db dyn TypeDb,
    package: Package,
    nominal: &Nominal<'db>,
) -> Vec<(u32, Nominal<'db>)> {
    let item = Item::new(db, package, nominal.file, nominal.name);
    let Declaration::Struct { fields, .. } = &db.declaration(item).declaration else {
        return Vec::new();
    };
    let parameters = class_parameters(db, nominal);
    fields
        .iter()
        .zip(0_u32..)
        .filter(|(field, _)| field.embedded)
        .filter_map(|(field, index)| {
            let ty =
                relations::substitute(db, field.ty, nominal.file, &parameters, &nominal.arguments);
            match ty.kind(db) {
                TypeKind::Struct(inner) => Some((index, inner.clone())),
                _ => None,
            }
        })
        .collect()
}

/// The `Name` nodes of the type parameters of the class `nominal` names.
fn class_parameters(db: &dyn TypeDb, nominal: &Nominal<'_>) -> Vec<NodeId> {
    match outline(db, nominal.file)
        .get(nominal.name)
        .map(|entry| &entry.kind)
    {
        Some(Kind::Class(class)) => class.type_parameters.clone(),
        _ => Vec::new(),
    }
}
