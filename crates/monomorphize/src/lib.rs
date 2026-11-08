//! Monomorphization: an instance of a generic function body at concrete types.
//!
//! Lowering builds one mid-level IR body per function, generic or not. Inside a generic body,
//! types may contain type parameters, such as `T` in `def scale[T: Number](x: T, k: T) -> T`.
//! An *instance* is a function and the types it is called with; this crate turns the generic body
//! into the instance's body by substituting each type parameter with its argument everywhere a
//! type appears: in every local, constant, conversion, aggregate, and the type arguments of every
//! call (decision 0018). Arithmetic on `T` needs no rewriting: once `T` is `f64`, `Multiply` on
//! two `f64` locals is primitive arithmetic, and traits are standard and statically dispatched in
//! the MVP, so no trait calls remain to resolve.
//!
//! The instance query runs [`instantiate`], then drop elaboration and array expansion, and marks
//! the result [`Phase::Ready`](mir::Phase). The driver collects which instances a program needs
//! (roadmap 1.9.5): it starts from the functions that are not generic and follows each call of a
//! generic function at concrete types, since that needs both lowering and this crate, and only an
//! application crate may depend on two implementations.
//!
//! A number literal in a generic body, `2` in `x * 2` for `T: Number`, keeps the value written
//! until substitution gives `T` a number type, then is written at that type: `2.0` for `f64`.
//!
//! # Example
//!
//! ```
//! use mir::{BinaryOperator, Body, Local, LocalKind, Operand, Place, Rvalue, Statement,
//!     StatementKind, Terminator, TerminatorKind};
//! use source::{FileId, Span};
//! use types::{Float, Mode, Primitive, Type, TypeKind};
//!
//! # #[salsa::db]
//! # #[derive(Clone, Default)]
//! # struct Database { storage: salsa::Storage<Self> }
//! # #[salsa::db]
//! # impl salsa::Database for Database {}
//! # #[salsa::db]
//! # impl database::Db for Database {
//! #     fn parse(&self, _: database::File) -> &database::ParsedFile { unreachable!() }
//! #     fn resolve(&self, _: database::Package, _: database::File) -> &database::Resolution {
//! #         unreachable!()
//! #     }
//! # }
//! # fn type_parameter(database: &Database) -> Type<'_> {
//! #     let file = database::File::new(
//! #         database,
//! #         FileId::from_index(0),
//! #         source::SourceFile::new("a.byn", "T").unwrap(),
//! #     );
//! #     let mut builder = syntax::TreeBuilder::new(FileId::from_index(0));
//! #     builder.leaf(syntax::SyntaxKind::Name, syntax::Role::Root, 0, 1);
//! #     let name = builder.finish().root();
//! #     Type::new(database, TypeKind::Parameter { file, name })
//! # }
//! let database = Database::default();
//! let span = Span::new(FileId::from_index(0), 0, 1);
//! let t = type_parameter(&database); // `T` in `def square[T: Number](x: T) -> T`
//! let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
//!
//! // def square[T: Number](x: T) -> T: return x * x
//! let mut generic = Body::new(t, span);
//! let x = generic.push_local(t, LocalKind::Parameter { name: "x".into(), mode: Mode::Borrow }, span);
//! let entry = generic.push_block(Terminator { kind: TerminatorKind::Return, span });
//! generic.blocks[entry.index()].statements.push(Statement {
//!     kind: StatementKind::Assign(
//!         Local::RETURN.into(),
//!         Rvalue::Binary(BinaryOperator::Multiply, Operand::Copy(x.into()), Operand::Copy(x.into())),
//!     ),
//!     span,
//! });
//!
//! let instance = monomorphize::instantiate(&database, &generic, &[t], &[f64]);
//! assert!(instance.display(&database).to_string().starts_with("fn(_1: f64) -> f64 {"));
//! ```

use mir::{
    Argument, Body, Callee, Constant, ConstantValue, Elementwise, Operand, Rvalue, StatementKind,
    TerminatorKind,
};
use types::{Nominal, Primitive, Type, TypeKind};

/// The body of `generic` at concrete types: each of `parameters`, the function's type parameters
/// in order as its signature lists them, replaced by the argument at the same position in
/// `arguments`, in every type the body and its outlined `prange` bodies mention. The phase is
/// unchanged; drop elaboration and array expansion still follow.
///
/// Cost: one visit of every type in the body. Types are interned, so a type that mentions no
/// parameter is returned as it is, and rebuilding one that does costs one intern per node.
///
/// # Panics
///
/// If `parameters` and `arguments` differ in length. The type checker infers one argument per
/// type parameter at every call, so a mismatch is a compiler bug.
#[must_use]
pub fn instantiate<'db>(
    db: &'db dyn salsa::Database,
    generic: &Body<'db>,
    parameters: &[Type<'db>],
    arguments: &[Type<'db>],
) -> Body<'db> {
    assert_eq!(
        parameters.len(),
        arguments.len(),
        "one type argument per type parameter"
    );
    let substitution = Substitution {
        db,
        parameters,
        arguments,
    };
    let mut body = generic.clone();
    substitution.body(&mut body);
    body
}

impl<'db> Substitution<'_, 'db> {
    /// Substitutes every type in `body`, its outlined `prange` bodies included.
    fn body(&self, body: &mut Body<'db>) {
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
struct Substitution<'a, 'db> {
    /// The database types are interned in.
    db: &'db dyn salsa::Database,
    /// The type parameters, as [`TypeKind::Parameter`] types.
    parameters: &'a [Type<'db>],
    /// The type that replaces the parameter at the same position.
    arguments: &'a [Type<'db>],
}

impl<'db> Substitution<'_, 'db> {
    /// `ty` with every parameter replaced, interned.
    fn ty(&self, ty: Type<'db>) -> Type<'db> {
        let list = |types: &[Type<'db>]| types.iter().map(|&ty| self.ty(ty)).collect::<Vec<_>>();
        let kind = match ty.kind(self.db) {
            TypeKind::Parameter { .. } => {
                return self
                    .parameters
                    .iter()
                    .position(|&parameter| parameter == ty)
                    .map_or(ty, |position| self.arguments[position]);
            }
            TypeKind::Primitive(_) | TypeKind::Str | TypeKind::None | TypeKind::Unknown => {
                return ty;
            }
            TypeKind::Tuple(items) => TypeKind::Tuple(list(items)),
            TypeKind::List(item) => TypeKind::List(self.ty(*item)),
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

#[cfg(test)]
mod tests;
