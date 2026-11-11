//! Type substitution: every type a body names, with each type parameter replaced by the type
//! an instance gives it (decision 0018).

use mir::{
    Argument, Body, Callee, Constant, ConstantValue, Elementwise, Operand, Rvalue, StatementKind,
    TerminatorKind,
};
use types::{Nominal, Primitive, Type, TypeKind};

impl<'db> Substitution<'_, 'db> {
    /// Substitutes every type in `body`, its outlined `prange` bodies included.
    pub(crate) fn body(&self, body: &mut Body<'db>) {
        for outlined in &mut body.outlined {
            self.body(outlined);
        }
        for local in &mut body.locals {
            local.ty = self.ty(local.ty);
        }
        for block in &mut body.blocks {
            for statement in &mut block.statements {
                match &mut statement.kind {
                    StatementKind::Assign(_, rvalue) => self.rvalue(rvalue),
                    StatementKind::Accumulate { value, .. } => self.operand(value),
                    StatementKind::Drop(_) => {}
                }
            }
            self.terminator(&mut block.terminator.kind);
        }
    }
}

/// A mapping from type parameters to the types that replace them.
pub(crate) struct Substitution<'a, 'db> {
    /// The database types are interned in.
    pub(crate) db: &'db dyn salsa::Database,
    /// The type parameters, as [`TypeKind::Parameter`] types.
    pub(crate) parameters: &'a [Type<'db>],
    /// The type that replaces the parameter at the same position.
    pub(crate) arguments: &'a [Type<'db>],
}

impl<'db> Substitution<'_, 'db> {
    /// `ty` with every parameter replaced, interned.
    fn ty(&self, ty: Type<'db>) -> Type<'db> {
        let list = |types: &[Type<'db>]| types.iter().map(|&ty| self.ty(ty)).collect::<Vec<_>>();
        if let Some(position) = self
            .parameters
            .iter()
            .position(|&parameter| parameter == ty)
        {
            return self.arguments[position];
        }
        let kind = match ty.kind(self.db) {
            // Version 0 has no generic unions (decision 0032): the checker rejects a member
            // that mentions a type parameter, so a union has nothing to replace, and rebuilding
            // its list here could break the order `types::union` keeps.
            TypeKind::Parameter { .. } | TypeKind::Union(_) => return ty,
            TypeKind::Primitive(_)
            | TypeKind::Str
            | TypeKind::None
            | TypeKind::Function { .. }
            | TypeKind::Standard(_)
            | TypeKind::Unknown => {
                return ty;
            }
            TypeKind::Tuple(items) => TypeKind::Tuple(list(items)),
            TypeKind::List(item) => TypeKind::List(self.ty(*item)),
            TypeKind::Dict { key, value } => TypeKind::Dict {
                key: self.ty(*key),
                value: self.ty(*value),
            },
            TypeKind::Optional(value) => TypeKind::Optional(self.ty(*value)),
            TypeKind::Array { element, rank } => TypeKind::Array {
                element: self.ty(*element),
                rank: *rank,
            },
            TypeKind::Fixed { element, shape } => TypeKind::Fixed {
                element: self.ty(*element),
                shape: shape.clone(),
            },
            TypeKind::Struct(nominal) => TypeKind::Struct(self.nominal(nominal)),
            TypeKind::Enum(nominal) => TypeKind::Enum(self.nominal(nominal)),
            TypeKind::Callable {
                parameters,
                returns,
            } => TypeKind::Callable {
                parameters: list(parameters),
                returns: self.ty(*returns),
            },
            TypeKind::Fallible { value, errors } => TypeKind::Fallible {
                value: self.ty(*value),
                errors: list(errors),
            },
        };
        Type::new(self.db, kind)
    }

    /// A class type with its type arguments substituted.
    fn nominal(&self, nominal: &Nominal<'db>) -> Nominal<'db> {
        Nominal {
            file: nominal.file,
            name: nominal.name,
            arguments: nominal.arguments.iter().map(|&ty| self.ty(ty)).collect(),
        }
    }

    /// Substitutes the types an rvalue names.
    fn rvalue(&self, rvalue: &mut Rvalue<'db>) {
        match rvalue {
            Rvalue::Use(operand) | Rvalue::Unary(_, operand) => self.operand(operand),
            Rvalue::Binary(_, left, right) | Rvalue::CheckedBinary(_, left, right) => {
                self.operand(left);
                self.operand(right);
            }
            Rvalue::Convert(operand, ty) => {
                self.operand(operand);
                *ty = self.ty(*ty);
            }
            Rvalue::Aggregate(ty, operands) | Rvalue::NewArray(ty, operands) => {
                *ty = self.ty(*ty);
                for operand in operands {
                    self.operand(operand);
                }
            }
            Rvalue::Length(..)
            | Rvalue::IsPresent(_)
            | Rvalue::ErrorIndex(_)
            | Rvalue::MemberIndex(_)
            | Rvalue::Format(_) => {}
            Rvalue::Elementwise(tree) => self.elementwise(tree),
        }
    }

    /// Substitutes the types in the leaves of a whole-array expression.
    fn elementwise(&self, tree: &mut Elementwise<'db>) {
        match tree {
            Elementwise::Leaf(operand) => self.operand(operand),
            Elementwise::Unary(_, operand) => self.elementwise(operand),
            Elementwise::Binary(_, left, right) => {
                self.elementwise(left);
                self.elementwise(right);
            }
        }
    }

    /// Substitutes the type of a constant operand; places have no types of their own.
    ///
    /// A number literal of a type parameter's type, `2` in `x * 2` for `T: Number`, keeps the
    /// value as written, an integer or a float. Once `T` is known it is written at that type:
    /// `2` becomes `2.0` when `T` is `f64`, and a real number becomes a complex one with no
    /// imaginary part when `T` is complex.
    fn operand(&self, operand: &mut Operand<'db>) {
        let Operand::Constant(Constant { ty, value }) = operand else {
            return;
        };
        let generic = *ty;
        *ty = self.ty(generic);
        if *ty == generic {
            return;
        }
        let TypeKind::Primitive(primitive) = ty.kind(self.db) else {
            return;
        };
        #[expect(
            clippy::cast_precision_loss,
            reason = "a literal written as an integer"
        )]
        let real = match *value {
            ConstantValue::Integer(integer) => integer as f64,
            ConstantValue::Float(bits) => f64::from_bits(bits),
            _ => return,
        };
        match primitive {
            Primitive::Float(_) => *value = ConstantValue::Float(real.to_bits()),
            Primitive::Complex(_) => {
                *value = ConstantValue::Complex {
                    real: real.to_bits(),
                    imaginary: 0.0_f64.to_bits(),
                };
            }
            Primitive::Integer(_) | Primitive::Bool => {}
        }
    }

    /// Substitutes the types of the values passed as `Owned` arguments.
    fn arguments(&self, arguments: &mut [Argument<'db>]) {
        for argument in arguments {
            if let Argument::Owned(operand) = argument {
                self.operand(operand);
            }
        }
    }

    /// Substitutes the types a terminator names, including a call's type arguments.
    fn terminator(&self, terminator: &mut TerminatorKind<'db>) {
        match terminator {
            TerminatorKind::Goto(_) | TerminatorKind::Return | TerminatorKind::Unreachable => {}
            TerminatorKind::Branch { condition, .. } => self.operand(condition),
            TerminatorKind::Switch { value, .. } => self.operand(value),
            TerminatorKind::Raise(error) => self.operand(error),
            TerminatorKind::Call {
                callee, arguments, ..
            } => {
                match callee {
                    Callee::Function { type_arguments, .. } => {
                        for ty in type_arguments {
                            *ty = self.ty(*ty);
                        }
                    }
                    Callee::Value(operand) => self.operand(operand),
                    Callee::Builtin(_) => {}
                }
                self.arguments(arguments);
            }
            TerminatorKind::ParallelFor {
                count, arguments, ..
            } => {
                self.operand(count);
                self.arguments(arguments);
            }
            TerminatorKind::Assert {
                condition, check, ..
            } => {
                self.operand(condition);
                for operand in check.operands_mut() {
                    self.operand(operand);
                }
            }
        }
    }
}
