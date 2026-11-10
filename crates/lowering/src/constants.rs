//! Constants: literals, `Final` constants and enum members, each the value it writes at the
//! type the checker gave it.

use crate::builder::Builder;
use database::Definition;
use mir::{Constant, ConstantValue};
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::{Declaration, Item, Primitive, Type, TypeKind, Value};

/// What a call writes for an argument it leaves out.
pub(crate) enum DefaultArgument<'db> {
    /// The default is a literal or a `Final` constant: this value.
    Constant(Constant<'db>),
    /// Any other default: the function that computes it, named by the parameter's `Name`
    /// node (decision 0027).
    Computed(Item<'db>),
}

impl<'db> Builder<'db> {
    /// The constant `node` names: a `Final` constant, by its name or through its module as in
    /// `units.G`, an enum member such as `Side.LEFT`, or a
    /// module-level function read as a value, such as `lorenz` passed to a `Callable`
    /// parameter. A function's value takes no bytes, since its type names the function
    /// (decision 0026), so it is written as `None` at that type.
    pub(crate) fn named_constant(&mut self, node: NodeId) -> Option<Constant<'db>> {
        let ty = self.ty(node);
        if matches!(ty.kind(self.db), TypeKind::Function { .. }) {
            return Some(Constant {
                ty,
                value: ConstantValue::None,
            });
        }
        if let Some(value) = self.math_constant(node) {
            return Some(Constant::float(ty, value));
        }
        match self.tree.kind(node) {
            SyntaxKind::Name => self.final_constant(node, node, ty),
            SyntaxKind::Attribute
                if self.tree.child(node, Role::Attribute).is_some_and(|name| {
                    matches!(self.definition(name), Some(Definition::Item(..)))
                }) =>
            {
                // `units.G` or `physics.units.G`: a constant of another module, named
                // through the module.
                let name = self.tree.child(node, Role::Attribute)?;
                self.final_constant(name, node, ty)
            }
            SyntaxKind::Attribute => {
                let TypeKind::Enum(nominal) = ty.kind(self.db) else {
                    return None;
                };
                let value = self.tree.child(node, Role::Value)?;
                // `Side.LEFT`, or `units.Side.LEFT` through the enum's module.
                let class = match self.tree.kind(value) {
                    SyntaxKind::Name => value,
                    SyntaxKind::Attribute => self.tree.child(value, Role::Attribute)?,
                    _ => return None,
                };
                if !matches!(self.definition(class), Some(Definition::Item(..))) {
                    return None;
                }
                let name = self.tree.child(node, Role::Attribute)?;
                let item = Item::new(self.db, self.package, nominal.file, nominal.name);
                let Declaration::Enum { variants } = &self.db.declaration(item).declaration else {
                    return None;
                };
                let wanted = self.text_of(name);
                let position = variants.iter().position(|variant| variant == wanted)?;
                Some(Constant {
                    ty,
                    value: ConstantValue::Integer(i128::try_from(position).ok()?),
                })
            }
            _ => None,
        }
    }

    /// The `Final` constant the name `name` resolves to, as a constant of type `ty`, or `None`
    /// when it names anything else. A constant whose value is not a number is reported at
    /// `node`, the whole expression.
    fn final_constant(
        &mut self,
        name: NodeId,
        node: NodeId,
        ty: Type<'db>,
    ) -> Option<Constant<'db>> {
        let &Definition::Item(file, name) = self.definition(name)? else {
            return None;
        };
        let item = Item::new(self.db, self.package, file, name);
        let Declaration::Constant { value, .. } = &self.db.declaration(item).declaration else {
            return None;
        };
        let value = match value {
            Some(Value::Integer(value)) if self.is_integer(ty) => {
                ConstantValue::Integer(i128::from(*value))
            }
            #[expect(clippy::cast_precision_loss, reason = "a constant as a float")]
            Some(Value::Integer(value)) => ConstantValue::Float((*value as f64).to_bits()),
            Some(Value::Float(bits)) => ConstantValue::Float(*bits),
            None => {
                self.unsupported(node, "constants whose value is not a number");
                ConstantValue::None
            }
        };
        Some(Constant { ty, value })
    }

    /// The default of parameter `position` of `item`, for the call `node` that leaves that
    /// argument out. A literal or `Final` constant has the same value whenever it is evaluated,
    /// so the call writes it itself, at the type the checker gave it in `item`'s body, which is
    /// the parameter's type. Any other default is computed by a function of its own, called at
    /// each such call (decision 0027, the `defaults` module). `None` after reporting a default
    /// of a generic function, whose types name that function's type parameters.
    pub(crate) fn default_value(
        &mut self,
        item: Item<'db>,
        position: usize,
        node: NodeId,
    ) -> Option<DefaultArgument<'db>> {
        let Declaration::Function(signature) = &self.db.declaration(item).declaration else {
            return None;
        };
        if !signature.type_parameters.is_empty() {
            self.unsupported(
                node,
                "leaving out a defaulted argument of a generic function",
            );
            return None;
        }
        let tree = self.db.parse(item.file(self.db)).tree();
        let (function, class) = crate::definition(tree, item.name(self.db))?;
        let parameter = crate::defaults::parameters(tree, function).nth(position)?;
        let default = tree.child(parameter, Role::Default)?;
        let mut callee = Builder::new(
            self.db,
            item,
            signature,
            self.db.body(item),
            function,
            class,
        );
        let constant = if callee.is_literal(default) {
            Some(callee.literal(default))
        } else {
            callee.named_constant(default)
        };
        if let (Some(constant), Ok(_)) = (constant, callee.finish()) {
            return Some(DefaultArgument::Constant(constant));
        }
        let file = item.file(self.db);
        let name = tree.child(parameter, Role::Name)?;
        Some(DefaultArgument::Computed(Item::new(
            self.db,
            item.package(self.db),
            file,
            name,
        )))
    }

    /// Whether `node` is a literal: a number, possibly signed, `True`, `False`, `None` or a
    /// plain string.
    pub(crate) fn is_literal(&self, node: NodeId) -> bool {
        match self.tree.kind(node) {
            SyntaxKind::Integer
            | SyntaxKind::Float
            | SyntaxKind::Imaginary
            | SyntaxKind::True
            | SyntaxKind::False
            | SyntaxKind::None
            | SyntaxKind::StringLiteral => true,
            SyntaxKind::UnaryOperation => {
                matches!(
                    self.tree.node(node).operator(),
                    Operator::Negate | Operator::Plus
                ) && self.tree.child(node, Role::Value).is_some_and(|value| {
                    matches!(
                        self.tree.kind(value),
                        SyntaxKind::Integer | SyntaxKind::Float | SyntaxKind::Imaginary
                    )
                })
            }
            _ => false,
        }
    }

    /// The constant a literal writes, at the type the checker gave it: `2` is a float constant
    /// where the context made it a float.
    pub(crate) fn literal(&mut self, node: NodeId) -> Constant<'db> {
        let ty = self.ty(node);
        let value = match self.tree.kind(node) {
            SyntaxKind::True => ConstantValue::Bool(true),
            SyntaxKind::False => ConstantValue::Bool(false),
            SyntaxKind::None => ConstantValue::None,
            SyntaxKind::StringLiteral => {
                if let Some(text) = crate::escapes::string(self.text_of(node)) {
                    ConstantValue::Str(text)
                } else {
                    self.unsupported(
                        node,
                        "strings with `\\N{…}` escapes, a `b` prefix or several parts",
                    );
                    ConstantValue::None
                }
            }
            _ => self.number(node, ty),
        };
        Constant { ty, value }
    }

    /// The value of the number literal `node`, possibly signed, as a constant of type `ty`.
    fn number(&mut self, node: NodeId, ty: Type<'db>) -> ConstantValue {
        let (negative, digits) = match self.tree.kind(node) {
            SyntaxKind::UnaryOperation => {
                let negative = self.tree.node(node).operator() == Operator::Negate;
                let value = self.tree.child(node, Role::Value);
                (negative, value.unwrap_or(node))
            }
            _ => (false, node),
        };
        let written = self.text_of(digits).replace('_', "").to_ascii_lowercase();
        let sign = if negative { -1.0 } else { 1.0 };
        let primitive = match *ty.kind(self.db) {
            TypeKind::Primitive(primitive) => primitive,
            // A literal of a type parameter's type, `2` in `x * 2` for `T: Number`, keeps the
            // value as written: an integer, or a float. Monomorphization writes it at the
            // instance's number type.
            TypeKind::Parameter { .. } if self.tree.kind(digits) == SyntaxKind::Integer => {
                return integer(&written).map_or(ConstantValue::None, |value| {
                    ConstantValue::Integer(if negative { -value } else { value })
                });
            }
            TypeKind::Parameter { .. } if self.tree.kind(digits) == SyntaxKind::Float => {
                return ConstantValue::Float(
                    (sign * written.parse::<f64>().unwrap_or(0.0)).to_bits(),
                );
            }
            _ => {
                self.unsupported(node, "this number literal");
                return ConstantValue::None;
            }
        };
        if self.tree.kind(digits) == SyntaxKind::Imaginary {
            let imaginary = written.trim_end_matches('j').parse::<f64>().unwrap_or(0.0);
            // `-2j` negates `0+2j`, so its real part is `-0.0`, as Python's `(-0-2j)` shows.
            return ConstantValue::Complex {
                real: (sign * 0.0_f64).to_bits(),
                imaginary: (sign * imaginary).to_bits(),
            };
        }
        let value = match self.tree.kind(digits) {
            SyntaxKind::Integer => {
                integer(&written).map(|value| if negative { -value } else { value })
            }
            _ => None,
        };
        match (primitive, value) {
            (Primitive::Integer(_), Some(value)) => ConstantValue::Integer(value),
            (Primitive::Complex(_), _) => ConstantValue::Complex {
                real: (sign * written.parse::<f64>().unwrap_or(0.0)).to_bits(),
                imaginary: 0.0_f64.to_bits(),
            },
            #[expect(clippy::cast_precision_loss, reason = "an integer literal as a float")]
            (_, Some(value)) => ConstantValue::Float((value as f64).to_bits()),
            (_, None) => {
                ConstantValue::Float((sign * written.parse::<f64>().unwrap_or(0.0)).to_bits())
            }
        }
    }

    /// The number `value` as a constant of the number type `ty`: an integer, a float, a complex
    /// number with no imaginary part, or for `bool`, whether it is not zero.
    pub(crate) fn number_constant(&self, ty: Type<'db>, value: i64) -> Constant<'db> {
        #[expect(clippy::cast_precision_loss, reason = "0 and 1 are exact")]
        let float = value as f64;
        let value = match ty.kind(self.db) {
            TypeKind::Primitive(Primitive::Bool) => ConstantValue::Bool(value != 0),
            TypeKind::Primitive(Primitive::Float(_)) => ConstantValue::Float(float.to_bits()),
            TypeKind::Primitive(Primitive::Complex(_)) => ConstantValue::Complex {
                real: float.to_bits(),
                imaginary: 0.0_f64.to_bits(),
            },
            _ => ConstantValue::Integer(i128::from(value)),
        };
        Constant { ty, value }
    }
}

/// The value of an integer literal written in lowercase without underscores: `42`, `0xff`,
/// `0o17`, `0b101`. `None` if it does not fit, which the checker already reports.
pub(crate) fn integer(written: &str) -> Option<i128> {
    let (radix, digits) = [("0x", 16), ("0o", 8), ("0b", 2)]
        .into_iter()
        .find_map(|(prefix, radix)| written.strip_prefix(prefix).map(|rest| (radix, rest)))
        .unwrap_or((10, written));
    i128::from_str_radix(digits, radix).ok()
}

impl Builder<'_> {
    /// The value of `pi`, `e`, `tau`, `inf` or `nan` from `math`, when `node` names one, as
    /// `pi` or `math.pi`: Python's values, the `f64`s nearest the mathematical constants.
    fn math_constant(&self, node: NodeId) -> Option<f64> {
        let name = match self.tree.kind(node) {
            SyntaxKind::Name => node,
            SyntaxKind::Attribute => self.tree.child(node, Role::Attribute)?,
            _ => return None,
        };
        let Some(Definition::Standard {
            module: "math",
            member: Some(member),
        }) = self.definition(name)
        else {
            return None;
        };
        match member.as_str() {
            "pi" => Some(std::f64::consts::PI),
            "e" => Some(std::f64::consts::E),
            "tau" => Some(std::f64::consts::TAU),
            "inf" => Some(f64::INFINITY),
            "nan" => Some(f64::NAN),
            _ => None,
        }
    }
}
