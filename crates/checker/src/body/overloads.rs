//! Overloaded library functions (decision 0067): a standard-library module may define one
//! function name more than once, and each call runs the overload its arguments fit.
//!
//! The resolver binds the name to its first definition; the overloads are the functions of the
//! same name in the same file. A call checks its arguments once without an expected type to
//! learn their types, discards what that check reported, and keeps the overloads whose
//! parameters accept them: an argument fits a parameter of a compatible type, and a number
//! literal also fits a wider number type, at a rank (point 4 of the decision): an int literal
//! fits an integer type at rank 0, a float at 1 and a complex number at 2, and a float literal a
//! float at 0 and a complex number at 1. Of the overloads that fit, the one whose every
//! argument's rank is no higher than another's wins; when none is, the call is ambiguous. A
//! generic overload, whose bound excludes the concrete overloads' types (point 6), is chosen
//! when no concrete one fits. The chosen overload is then checked as an ordinary call, so
//! every error the call has is reported against it.

use super::Body;
use super::calls::Arguments;
use super::relations::compatible;
use database::{File, library_module};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Declaration, Item, Primitive, Signature, Type, TypeKind};

/// What a call's argument is, for choosing an overload: its type, and whether it is a number
/// literal, which fits wider number types.
#[derive(Clone, Copy)]
struct Given<'db> {
    /// The argument's type, checked without an expected type.
    ty: Type<'db>,
    /// Whether it is an int literal, a float literal or neither, signs included.
    literal: Literal,
}

/// The kind of number literal an argument is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Literal {
    /// `2` or `-2`.
    Integer,
    /// `2.0` or `-2.0`.
    Float,
    /// Any other expression.
    No,
}

impl<'db> Body<'db> {
    /// The overloads of the function `name` defines in `file`: every top-level function of the
    /// same name, in source order, when `file` is a library file and there is more than one;
    /// otherwise empty.
    pub(super) fn overloads(&self, file: File, name: NodeId) -> Vec<NodeId> {
        if library_module(file.source(self.db).name()).is_none() {
            return Vec::new();
        }
        let tree = self.db.parse(file).tree();
        let text = file.source(self.db).text();
        let wanted = tree.text(name, text);
        let found: Vec<NodeId> = tree
            .children_with(tree.root(), Role::Body)
            .filter(|&statement| tree.kind(statement) == SyntaxKind::FunctionDefinition)
            .filter_map(|statement| tree.child(statement, Role::Name))
            .filter(|&node| tree.text(node, text) == wanted)
            .collect();
        if found.len() > 1 { found } else { Vec::new() }
    }

    /// The overload of `overloads`, defined in `file`, that the call `node` runs, or an error
    /// naming the overloads when none fits or two fit equally well. `callee` is the name as
    /// written.
    pub(super) fn choose_overload(
        &mut self,
        node: NodeId,
        callee: &str,
        file: File,
        overloads: &[NodeId],
        arguments: &Arguments,
    ) -> Result<NodeId, Type<'db>> {
        let given = self.given(arguments);
        let signatures: Vec<(NodeId, Signature<'db>)> = overloads
            .iter()
            .filter_map(|&overload| {
                let item = Item::new(self.db, self.lowerer.package, file, overload);
                match self.db.declaration(item).declaration.clone() {
                    Declaration::Function(signature) => Some((overload, signature)),
                    _ => None,
                }
            })
            .collect();
        let fitting: Vec<(NodeId, Vec<u8>)> = signatures
            .iter()
            .filter(|(_, signature)| signature.type_parameters.is_empty())
            .filter_map(|(overload, signature)| {
                self.ranks(signature, &given, arguments)
                    .map(|ranks| (*overload, ranks))
            })
            .collect();
        let best: Vec<NodeId> = fitting
            .iter()
            .filter(|(_, ranks)| {
                fitting
                    .iter()
                    .all(|(_, other)| ranks.iter().zip(other).all(|(mine, theirs)| mine <= theirs))
            })
            .map(|&(overload, _)| overload)
            .collect();
        if let [chosen] = best[..] {
            return Ok(chosen);
        }
        if fitting.is_empty() {
            let generic: Vec<NodeId> = signatures
                .iter()
                .filter(|(_, signature)| !signature.type_parameters.is_empty())
                .map(|&(overload, _)| overload)
                .collect();
            if let [chosen] = generic[..] {
                return Ok(chosen);
            }
        }
        let shown: Vec<String> = if fitting.is_empty() {
            overloads.to_vec()
        } else {
            fitting.iter().map(|&(overload, _)| overload).collect()
        }
        .into_iter()
        .map(|overload| format!("`{}`", self.header(file, overload)))
        .collect();
        let (message, label) = if fitting.is_empty() {
            (
                format!("no overload of `{callee}` takes these arguments"),
                "no overload fits",
            )
        } else {
            (
                format!("this call of `{callee}` fits more than one overload"),
                "ambiguous call",
            )
        };
        let hint = format!("the overloads are {}", shown.join(", "));
        Err(self.lowerer.error(node, &message, label, Some(&hint)))
    }

    /// The definition the function name `node`, read as a value, stands for: `name` itself when
    /// it has no overloads, and otherwise the one overload whose signature is the `Callable`
    /// the context expects (point 5 of decision 0067). Anywhere else, or when no overload or
    /// more than one has that signature, it is an error.
    pub(super) fn overload_value(
        &mut self,
        node: NodeId,
        file: File,
        name: NodeId,
        expected: Option<Type<'db>>,
    ) -> Result<NodeId, Type<'db>> {
        let overloads = self.overloads(file, name);
        if overloads.is_empty() {
            return Ok(name);
        }
        let wanted = expected.and_then(|expected| match expected.kind(self.db) {
            TypeKind::Callable {
                parameters,
                returns,
            } => Some((parameters.clone(), *returns)),
            _ => None,
        });
        let matching: Vec<NodeId> = match &wanted {
            Some((parameters, returns)) => overloads
                .iter()
                .copied()
                .filter(|&overload| {
                    let item = Item::new(self.db, self.lowerer.package, file, overload);
                    matches!(&self.db.declaration(item).declaration,
                        Declaration::Function(signature)
                            if signature.type_parameters.is_empty()
                                && signature.returns == *returns
                                && signature.parameters.len() == parameters.len()
                                && signature.parameters.iter().zip(parameters)
                                    .all(|(parameter, ty)| parameter.ty == *ty))
                })
                .collect(),
            None => Vec::new(),
        };
        if let [chosen] = matching[..] {
            return Ok(chosen);
        }
        let shown = self.tree.text(node, self.text).to_owned();
        let headers: Vec<String> = overloads
            .iter()
            .map(|&overload| format!("`{}`", self.header(file, overload)))
            .collect();
        let message = if wanted.is_some() {
            format!("no overload of `{shown}` has the signature this `Callable` needs")
        } else {
            format!("`{shown}` has overloads, so it is a value only where a `Callable` is expected")
        };
        let hint = format!("the overloads are {}", headers.join(", "));
        Err(self
            .lowerer
            .error(node, &message, "an overloaded function", Some(&hint)))
    }

    /// Reports each concrete overload of the generic function `name` whose arguments the generic
    /// one also takes: a generic overload's bound must exclude every concrete overload's types
    /// (point 6 of decision 0067), so that no call fits both.
    pub(super) fn overlapping_overloads(&mut self, name: NodeId) {
        if self.signature.type_parameters.is_empty() {
            return;
        }
        let file = self.lowerer.file;
        for overload in self.overloads(file, name) {
            let item = Item::new(self.db, self.lowerer.package, file, overload);
            let Declaration::Function(concrete) = self.db.declaration(item).declaration.clone()
            else {
                continue;
            };
            let generic = &self.signature.parameters;
            let overlaps = concrete.type_parameters.is_empty()
                && concrete.parameters.len() == generic.len()
                && generic
                    .iter()
                    .zip(&concrete.parameters)
                    .all(|(mine, theirs)| self.unifies(mine.ty, theirs.ty));
            if overlaps {
                let message = format!(
                    "this generic overload also takes the arguments of `{}`",
                    self.header(file, overload)
                );
                self.lowerer.error(
                    name,
                    &message,
                    "overlaps a concrete overload",
                    Some("a generic overload's bound must exclude every concrete overload's types"),
                );
            }
        }
    }

    /// Whether the generic parameter type `generic` takes a value of the concrete type
    /// `concrete`: each type parameter in it stands for a type that satisfies its bound.
    fn unifies(&self, generic: Type<'db>, concrete: Type<'db>) -> bool {
        match (generic.kind(self.db), concrete.kind(self.db)) {
            (TypeKind::Parameter { file, name }, _) => {
                crate::traits::bound(self.db, self.lowerer.package, *file, *name).is_none_or(
                    |bound| {
                        crate::traits::satisfies(self.db, self.lowerer.package, bound, concrete)
                    },
                )
            }
            (
                TypeKind::Array { element, rank },
                TypeKind::Array {
                    element: theirs,
                    rank: their_rank,
                },
            ) => rank == their_rank && self.unifies(*element, *theirs),
            (TypeKind::List(element), TypeKind::List(theirs)) => self.unifies(*element, *theirs),
            _ => generic == concrete,
        }
    }

    /// Each argument's type and literal kind, checked without an expected type. What the check
    /// reports is discarded: the chosen overload's check reports it again.
    fn given(&mut self, arguments: &Arguments) -> Vec<(Option<String>, Given<'db>)> {
        let before = self.lowerer.diagnostics.len();
        let mut given = Vec::new();
        for &argument in &arguments.positional {
            let ty = self.expression(argument, None);
            given.push((
                None,
                Given {
                    ty,
                    literal: self.literal_kind(argument),
                },
            ));
        }
        for &(name, value) in &arguments.keywords {
            let ty = self.expression(value, None);
            let name = self.tree.text(name, self.text).to_owned();
            given.push((
                Some(name),
                Given {
                    ty,
                    literal: self.literal_kind(value),
                },
            ));
        }
        self.lowerer.diagnostics.truncate(before);
        given
    }

    /// The kind of number literal `node` is, looking through a sign.
    fn literal_kind(&self, node: NodeId) -> Literal {
        if !self.is_number_literal(node) {
            return Literal::No;
        }
        let mut node = node;
        while self.tree.kind(node) == SyntaxKind::UnaryOperation {
            match self.tree.child(node, Role::Value) {
                Some(value) => node = value,
                None => return Literal::No,
            }
        }
        match self.tree.kind(node) {
            SyntaxKind::Integer => Literal::Integer,
            SyntaxKind::Float => Literal::Float,
            _ => Literal::No,
        }
    }

    /// The rank at which each argument fits its parameter of `signature`, in the order given,
    /// or `None` when one does not fit, a keyword names no parameter, or a parameter without a
    /// default is left out.
    fn ranks(
        &self,
        signature: &Signature<'db>,
        given: &[(Option<String>, Given<'db>)],
        arguments: &Arguments,
    ) -> Option<Vec<u8>> {
        let parameters = &signature.parameters;
        if arguments.positional.len() > parameters.len() {
            return None;
        }
        let mut filled = vec![false; parameters.len()];
        let mut ranks = Vec::new();
        for (position, (keyword, argument)) in given.iter().enumerate() {
            let index = match keyword {
                Some(name) => parameters
                    .iter()
                    .position(|parameter| &parameter.name == name)?,
                None => position,
            };
            if filled[index] {
                return None;
            }
            filled[index] = true;
            ranks.push(self.rank(parameters[index].ty, *argument)?);
        }
        let complete = parameters
            .iter()
            .zip(&filled)
            .all(|(parameter, &filled)| filled || parameter.default);
        complete.then_some(ranks)
    }

    /// How far `argument` is from `parameter`'s type: 0 when it fits as it is, more for a
    /// number literal taken at a wider type, `None` when it does not fit.
    fn rank(&self, parameter: Type<'db>, argument: Given<'db>) -> Option<u8> {
        let primitive = match parameter.kind(self.db) {
            TypeKind::Primitive(primitive) => Some(*primitive),
            _ => None,
        };
        // A literal's place on the ladder integer, float, complex, and the parameter's: the
        // literal climbs, never descends.
        let from = match argument.literal {
            Literal::Integer => Some(0),
            Literal::Float => Some(1),
            Literal::No => None,
        };
        let to = match primitive {
            Some(Primitive::Integer(_)) => Some(0),
            Some(Primitive::Float(_)) => Some(1),
            Some(Primitive::Complex(_)) => Some(2),
            _ => None,
        };
        match (from, to) {
            (Some(from), Some(to)) if to >= from => Some(to - from),
            _ => compatible(self.db, parameter, argument.ty).then_some(0),
        }
    }

    /// The first line of the definition whose name is `overload`, in `file`, without its colon:
    /// `def jv(v: f64, z: complex128) -> complex128`.
    fn header(&self, file: File, overload: NodeId) -> String {
        let tree = self.db.parse(file).tree();
        let text = file.source(self.db).text();
        let statement = tree
            .children_with(tree.root(), Role::Body)
            .find(|&statement| tree.child(statement, Role::Name) == Some(overload));
        let Some(statement) = statement else {
            return tree.text(overload, text).to_owned();
        };
        let span = tree.span(statement);
        let start = span.start() as usize;
        let line = text[start..].lines().next().unwrap_or_default();
        line.trim_end().trim_end_matches(':').to_owned()
    }
}
