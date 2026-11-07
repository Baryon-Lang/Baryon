//! Constants: literals, `Final` constants and enum members, each the value it writes at the
//! type the checker gave it.

use crate::builder::Builder;
use database::Definition;
use mir::{Constant, ConstantValue};
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::{Declaration, Item, Primitive, Type, TypeKind, Value};

impl<'db> Builder<'db> {
    /// The constant `node` names: a `Final` constant, or an enum member such as `Side.LEFT`.
    pub(crate) fn named_constant(&mut self, node: NodeId) -> Option<Constant<'db>> {
        let ty = self.ty(node);
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
        let &TypeKind::Primitive(primitive) = ty.kind(self.db) else {
            self.unsupported(node, "number literals of a type parameter's type");
            return ConstantValue::None;
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
