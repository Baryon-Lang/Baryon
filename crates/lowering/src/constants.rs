//! Constants: literals, `Final` constants and enum members, each the value it writes at the
//! type the checker gave it.

use crate::builder::Builder;
use database::Definition;
use mir::{Constant, ConstantValue};
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::{Declaration, Item, Primitive, Type, TypeKind, Value};

impl<'db> Builder<'db> {
    /// The constant `node` names: a `Final` constant, an enum member such as `Side.LEFT`, or a
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
        match self.tree.kind(node) {
            SyntaxKind::Name => {
                let &Definition::Item(file, name) = self.definition(node)? else {
                    return None;
                };
                let item = Item::new(self.db, self.package, file, name);
                let Declaration::Constant { value, .. } = &self.db.declaration(item).declaration
                else {
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
            SyntaxKind::Attribute => {
                let TypeKind::Enum(nominal) = ty.kind(self.db) else {
                    return None;
                };
                let value = self.tree.child(node, Role::Value)?;
                if self.tree.kind(value) != SyntaxKind::Name
                    || !matches!(self.definition(value), Some(Definition::Item(..)))
                {
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

    /// The value of the default of parameter `position` of `item`, for the call `node` that
    /// leaves that argument out: the literal or constant the default writes, at the type the
    /// checker gave it in `item`'s body, which is the parameter's type. A literal or constant
    /// has the same value whenever it is evaluated, so the call can write it itself. `None`
    /// after reporting a default that is any other expression, or belongs to a generic
    /// function, whose types name that function's type parameters.
    pub(crate) fn default_value(
        &mut self,
        item: Item<'db>,
        position: usize,
        node: NodeId,
    ) -> Option<Constant<'db>> {
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
        let default = tree
            .child(function, Role::Parameters)
            .into_iter()
            .flat_map(|parameters| tree.children(parameters))
            .filter(|&parameter| tree.kind(parameter) == SyntaxKind::Parameter)
            .filter(|&parameter| tree.child(parameter, Role::Name).is_some())
            .nth(position)
            .and_then(|parameter| tree.child(parameter, Role::Default))?;
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
            return Some(constant);
        }
        self.unsupported(
            node,
            "leaving out an argument whose default is not a literal or a constant",
        );
        None
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
                if let Some(text) = string(self.text_of(node)) {
                    ConstantValue::Str(text)
                } else {
                    self.unsupported(node, "strings with escapes, prefixes or several parts");
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
            return ConstantValue::Complex {
                real: 0.0_f64.to_bits(),
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

/// The text of a string literal with one pair of plain quotes and no escapes, such as `"done"`
/// or `'x'`; `None` for anything else, which a later slice decodes.
pub(crate) fn string(written: &str) -> Option<String> {
    let quote = written
        .chars()
        .next()
        .filter(|&quote| quote == '"' || quote == '\'')?;
    let inner = written.strip_prefix(quote)?.strip_suffix(quote)?;
    (!inner.contains(quote) && !inner.contains('\\')).then(|| inner.to_owned())
}
