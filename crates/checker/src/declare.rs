//! Declarations: what each kind of definition declares, from its annotations.

use crate::constant::to_float;
use crate::lower::Lowerer;
use crate::outline::{Base, Entry, Kind};
use crate::traits::{Trait, user_trait};
use diagnostics::Diagnostic;
use syntax::{NodeId, Role, SyntaxKind};
use types::{
    Checked, Declaration, Field, Float, Integer, Item, Mode, Parameter, Primitive, Signature, Type,
    TypeKind, Value,
};

impl<'db> Lowerer<'db> {
    /// What the definition `entry` declares, with the errors found.
    pub(crate) fn declare(mut self, entry: &Entry) -> Checked<'db> {
        if matches!(entry.kind, Kind::Function | Kind::Class(_)) {
            self.bounds(entry.statement);
        }
        let declaration = match &entry.kind {
            Kind::Function => Declaration::Function(self.signature(entry)),
            Kind::Class(class) if class.base == Base::Enum => self.enumeration(entry.statement),
            Kind::Class(class) if class.base == Base::Protocol => {
                self.trait_declaration(entry.statement)
            }
            Kind::Class(class) => self.structure(entry.statement, class.copy, class.base),
            Kind::Alias => {
                let ty = match self.tree.child(entry.statement, Role::Value) {
                    Some(value) => self.lower(value),
                    None => self.unknown(),
                };
                Declaration::Alias(ty)
            }
            Kind::Constant => self.constant_declaration(entry.statement),
            Kind::Unsupported => Declaration::Alias(self.unknown()),
        };
        Checked {
            declaration,
            diagnostics: self.diagnostics,
        }
    }

    /// Reports each bound in a definition's type parameters that names something other than
    /// a standard trait, such as a class.
    fn bounds(&mut self, definition: NodeId) {
        let Some(parameters) = self.tree.child(definition, Role::TypeParameters) else {
            return;
        };
        let variables: Vec<NodeId> = self.tree.children_with(parameters, Role::Item).collect();
        for variable in variables {
            let Some(bound) = self.tree.child(variable, Role::Bound) else {
                continue;
            };
            let written = self.tree.text(bound, self.text);
            if self.tree.kind(bound) == SyntaxKind::Tuple {
                self.error(
                    bound,
                    "a type parameter has one bound",
                    "more than one trait",
                    Some(
                        "declare a trait that requires them all, `class ChargedBody(Body, \
                         Charged, Protocol): pass`, and bound by it; in Python `[T: (A, B)]` \
                         means `T` is exactly `A` or exactly `B` (decision 0031)",
                    ),
                );
                continue;
            }
            let is_trait = (self.tree.kind(bound) == SyntaxKind::Name
                && Trait::from_name(written).is_some())
                || user_trait(self.db, self.package, self.file, bound).is_some();
            if is_trait || self.definition(bound).is_none() {
                // An undefined name is already reported by name resolution.
                continue;
            }
            let message = format!("`{written}` is not a trait");
            self.error(
                bound,
                &message,
                "expected a trait",
                Some(
                    "a type parameter is bounded by `Number`, `Float`, `VectorSpace`, `Copy`, \
                     or a trait declared as `class Body(Protocol)`",
                ),
            );
        }
    }

    /// A function's signature. A method's first parameter may leave out its type: `self` is a
    /// read-only borrow of the class.
    fn signature(&mut self, entry: &Entry) -> Signature<'db> {
        let statement = entry.statement;
        let type_parameters = self.parameter_types(statement);
        let nodes: Vec<NodeId> = self
            .tree
            .child(statement, Role::Parameters)
            .into_iter()
            .flat_map(|parameters| self.tree.children(parameters))
            .filter(|&parameter| self.tree.kind(parameter) == SyntaxKind::Parameter)
            .collect();
        let mut parameters = Vec::new();
        for (position, node) in nodes.into_iter().enumerate() {
            let Some(name) = self.tree.child(node, Role::Name) else {
                continue;
            };
            let name = self.tree.text(name, self.text).to_owned();
            let (ty, mode) = match self.tree.child(node, Role::Annotation) {
                Some(annotation) => self.parameter(annotation),
                None if position == 0 && entry.class.is_some() => (self.self_type(), Mode::Borrow),
                None => {
                    let message = format!("parameter `{name}` needs a type");
                    let note = format!("every parameter's type is written: `{name}: f64`");
                    let ty = self.error(node, &message, "no type annotation", Some(&note));
                    (ty, Mode::Borrow)
                }
            };
            let default = self.tree.child(node, Role::Default).is_some();
            parameters.push(Parameter {
                name,
                ty,
                mode,
                default,
            });
        }
        let returns = match self.tree.child(statement, Role::Returns) {
            Some(returns) => self.lower(returns),
            None => Type::new(self.db, TypeKind::None),
        };
        Signature {
            type_parameters,
            parameters,
            returns,
        }
    }

    /// A struct or error type: its fields, in order, its methods, and the traits it names.
    fn structure(&mut self, class: NodeId, copy: bool, base: Base) -> Declaration<'db> {
        let traits = self
            .named_traits(class)
            .into_iter()
            .map(|(_, found)| found.item(self.db, self.package))
            .collect();
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        let statements: Vec<NodeId> = self.tree.children_with(class, Role::Body).collect();
        for statement in statements {
            match self.tree.kind(statement) {
                SyntaxKind::AnnotatedAssign => {
                    let Some(target) = self.tree.child(statement, Role::Target) else {
                        continue;
                    };
                    let name = self.tree.text(target, self.text).to_owned();
                    let (ty, embedded) = match self.tree.child(statement, Role::Annotation) {
                        Some(annotation) => self.field_type(annotation),
                        None => (self.unknown(), false),
                    };
                    if let Some(value) = self.tree.child(statement, Role::Value) {
                        self.error(
                            value,
                            "a struct field has no default value",
                            "remove this default",
                            Some("every field is given when a struct is built: `Particle(position, velocity, 1.0)`"),
                        );
                    }
                    fields.push(Field { name, ty, embedded });
                }
                SyntaxKind::Assign => {
                    self.error(
                        statement,
                        "a struct field needs a type, and no value",
                        "not a field",
                        Some("declare a field with its type: `mass: f64`"),
                    );
                }
                SyntaxKind::FunctionDefinition => {
                    if let Some(name) = self.tree.child(statement, Role::Name) {
                        methods.push(Item::new(self.db, self.package, self.file, name));
                    }
                }
                _ => {}
            }
        }
        Declaration::Struct {
            fields,
            copy,
            error: base == Base::Error,
            methods,
            traits,
        }
    }

    /// An enum: the names of its variants, in order. An enum has no methods in version 0, so
    /// it cannot implement a trait yet.
    fn enumeration(&mut self, class: NodeId) -> Declaration<'db> {
        for (base, _) in self.named_traits(class) {
            self.error(
                base,
                "an enum cannot implement a trait in version 0",
                "a trait",
                Some("enums have no methods yet, so they cannot define a trait's methods"),
            );
        }
        let mut variants = Vec::new();
        let statements: Vec<NodeId> = self.tree.children_with(class, Role::Body).collect();
        for statement in statements {
            match self.tree.kind(statement) {
                SyntaxKind::Assign => variants.extend(
                    self.tree
                        .children_with(statement, Role::Target)
                        .map(|target| self.tree.text(target, self.text).to_owned()),
                ),
                SyntaxKind::AnnotatedAssign => {
                    self.error(
                        statement,
                        "an enum member has a value, not a type",
                        "not an enum member",
                        Some("write a member as `PERIODIC = 1`"),
                    );
                }
                _ => {}
            }
        }
        Declaration::Enum { variants }
    }

    /// A constant, `N: Final = 64` or `DT: Final[f32] = 0.005`: its type and value.
    fn constant_declaration(&mut self, statement: NodeId) -> Declaration<'db> {
        let declared = self
            .tree
            .child(statement, Role::Annotation)
            .and_then(|annotation| self.final_type(annotation));
        let Some(value) = self.tree.child(statement, Role::Value) else {
            self.error(
                statement,
                "a constant needs a value",
                "no value",
                Some("for example `N: Final = 64`"),
            );
            return Declaration::Constant {
                ty: declared.unwrap_or_else(|| self.unknown()),
                value: None,
            };
        };
        let literal = match self.tree.kind(value) {
            SyntaxKind::StringLiteral => Some(TypeKind::Str),
            SyntaxKind::True | SyntaxKind::False => Some(TypeKind::Primitive(Primitive::Bool)),
            _ => None,
        };
        if let Some(kind) = literal {
            let ty = Type::new(self.db, kind);
            if let Some(declared) = declared
                && declared != ty
                && declared.kind(self.db) != &TypeKind::Unknown
            {
                return self.mismatch(value, declared);
            }
            return Declaration::Constant { ty, value: None };
        }
        let computed = self.evaluate(value);
        let (ty, value) = match (declared, computed) {
            (_, None) => (declared.unwrap_or_else(|| self.unknown()), None),
            (None, Some(computed)) => {
                let primitive = match computed {
                    Value::Integer(_) => Primitive::Integer(Integer::I64),
                    Value::Float(_) => Primitive::Float(Float::F64),
                };
                let ty = Type::new(self.db, TypeKind::Primitive(primitive));
                (ty, Some(computed))
            }
            (Some(declared), Some(computed)) => match self.conform(declared, computed, value) {
                Some(converted) => (declared, Some(converted)),
                None => (declared, None),
            },
        };
        Declaration::Constant { ty, value }
    }

    /// The `T` in `Final[T]`, or `None` for a bare `Final`.
    fn final_type(&mut self, annotation: NodeId) -> Option<Type<'db>> {
        if self.tree.kind(annotation) != SyntaxKind::Subscript {
            return None;
        }
        let index = self.tree.child(annotation, Role::Index)?;
        Some(self.lower(index))
    }

    /// `value`, computed for a constant declared as `declared`, converted to that type: an
    /// integer becomes a float for a float type, and a float rounds to `f32` for an `f32`. Reports
    /// a value that does not fit and returns `None`.
    fn conform(&mut self, declared: Type<'db>, value: Value, node: NodeId) -> Option<Value> {
        match (declared.kind(self.db), value) {
            (TypeKind::Unknown, _) => None,
            (TypeKind::Primitive(Primitive::Integer(integer)), Value::Integer(number)) => {
                if fits(*integer, number) {
                    return Some(value);
                }
                let message = format!("`{number}` does not fit in `{}`", integer.name());
                self.error(node, &message, "out of range", None);
                None
            }
            (TypeKind::Primitive(Primitive::Float(float)), _) => {
                let number = match value {
                    Value::Integer(number) => to_float(number),
                    Value::Float(bits) => f64::from_bits(bits),
                };
                let number = match float {
                    Float::F32 => round_to_f32(number),
                    Float::F64 => number,
                };
                Some(Value::Float(number.to_bits()))
            }
            _ => {
                self.mismatch(node, declared);
                None
            }
        }
    }

    /// Reports a constant's value that is not of its declared type.
    fn mismatch(&mut self, node: NodeId, declared: Type<'db>) -> Declaration<'db> {
        let message = format!("this value is not of type `{}`", declared.display(self.db));
        self.error(node, &message, "wrong type for the constant", None);
        Declaration::Constant {
            ty: declared,
            value: None,
        }
    }
}

/// Whether `number` fits in the integer type `integer`.
pub(crate) fn fits(integer: Integer, number: i64) -> bool {
    let bits = integer.bits();
    if integer.is_signed() {
        bits == 64 || (-(1_i64 << (bits - 1))..1_i64 << (bits - 1)).contains(&number)
    } else {
        number >= 0 && (bits == 64 || number < 1_i64 << bits)
    }
}

/// `number` rounded to the nearest `f32`, as an `f32` constant stores it.
#[expect(
    clippy::cast_possible_truncation,
    reason = "rounding to the nearest f32 is what an f32 constant means"
)]
fn round_to_f32(number: f64) -> f64 {
    f64::from(number as f32)
}

/// The declaration given to an alias or constant defined in terms of itself, such as
/// `A: Final = B + 1` with `B: Final = A`, so the cycle ends with one error and unknown types.
pub(crate) fn cyclic<'db>(lowerer: &mut Lowerer<'db>, entry: &Entry) -> Declaration<'db> {
    let name = lowerer.tree.text(entry.name, lowerer.text).to_owned();
    let message = format!("`{name}` is defined in terms of itself");
    let unknown = lowerer.error(
        entry.name,
        &message,
        "refers back to itself",
        Some("an alias or constant cannot depend on its own value"),
    );
    match entry.kind {
        Kind::Constant => Declaration::Constant {
            ty: unknown,
            value: None,
        },
        _ => Declaration::Alias(unknown),
    }
}

/// The errors of a lowerer, for a declaration built outside [`Lowerer::declare`].
pub(crate) fn finish<'db>(lowerer: Lowerer<'db>, declaration: Declaration<'db>) -> Checked<'db> {
    let diagnostics: Vec<Diagnostic> = lowerer.diagnostics;
    Checked {
        declaration,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_ranges() {
        assert!(fits(Integer::U8, 255));
        assert!(!fits(Integer::U8, 256));
        assert!(!fits(Integer::U8, -1));
        assert!(fits(Integer::I8, -128));
        assert!(!fits(Integer::I8, 128));
        assert!(fits(Integer::I64, i64::MIN));
        assert!(fits(Integer::U64, i64::MAX));
    }

    #[test]
    fn f32_constants_round() {
        assert_eq!(round_to_f32(0.1).to_bits(), f64::from(0.1_f32).to_bits());
    }
}
