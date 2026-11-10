//! Monomorphization: an instance of a generic function body at concrete types.
//!
//! Lowering builds one mid-level IR body per function, generic or not. Inside a generic body,
//! types may contain type parameters, such as `T` in `def scale[T: Number](x: T, k: T) -> T`.
//! An *instance* is a function and the types it is called with; this crate turns the generic body
//! into the instance's body by substituting each type parameter with its argument everywhere a
//! type appears: in every local, constant, conversion, aggregate, and the type arguments of every
//! call (decision 0018). Arithmetic on `T` needs little rewriting: once `T` is `f64`, `Multiply` on
//! two `f64` locals is primitive arithmetic. Once `T` is an integer, `+`, `-`, `*` and `-x` on it
//! gain the overflow check lowering writes for integers (decision 0030), and once it is an array
//! they become whole-array expressions. A call of a user trait's method, `body.energy()` on a
//! `T: Body`, becomes a direct call of the receiver type's own method in [`trait_calls`]
//! (decision 0031), so no trait call reaches the backend.
//!
//! The instance query runs [`instantiate`], then drop elaboration and array expansion, and marks
//! the result [`Phase::Ready`](mir::Phase). The driver collects which instances a program needs
//! (roadmap 1.9.5): it starts from the functions that are not generic and follows each call of a
//! generic function at concrete types, since that needs both lowering and this crate, and only an
//! application crate may depend on two implementations.
//!
//! A method of a generic class, such as `Pair[T]`'s, is generic in the class's type parameters,
//! which come first in its instance's type arguments, then the method's own (roadmap 1.9.18).
//!
//! A `Callable` parameter is an implicit type parameter (decision 0026). When two of one
//! signature receive different functions, their shared `Callable` type names neither, so
//! [`instantiate`] gives each parameter's local, and each local copied from it, its function's
//! own type, and [`forward_callables`] gives each call that passes such a local on the type
//! argument of that function.
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

mod callables;

pub use callables::forward_callables;
use mir::{
    Argument, BinaryOperator, Body, Callee, Check, Constant, ConstantValue, Elementwise,
    LocalDeclaration, LocalKind, Operand, Place, Projection, Rvalue, Statement, StatementKind,
    Terminator, TerminatorKind, UnaryOperator,
};
use types::{Integer, Item, Nominal, Primitive, Type, TypeKind};

/// The body of `generic` at concrete types: each of `parameters`, the function's type parameters
/// in order as its signature lists them, replaced by the argument at the same position in
/// `arguments`, in every type the body and its outlined `prange` bodies mention. The phase is
/// unchanged; drop elaboration and array expansion still follow.
///
/// A `Callable` parameter is an implicit type parameter (decision 0026): `parameters` may end
/// with one `Callable` type per `Callable` parameter, in the order the signature lists them, and
/// `arguments` with the function's own type that the instance received for each. A call of the
/// parameter then becomes a direct call of that function. Two `Callable` parameters of one
/// signature have the same type, so each parameter's local takes its own function's type
/// directly; the `Callable` type itself is replaced elsewhere in the body only when every
/// parameter of that type received the same function.
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
    // A `Callable` type that received two different functions names neither; it stays as it
    // is, and only the parameters' own locals learn which function each one holds.
    let (listed, replacing): (Vec<Type<'db>>, Vec<Type<'db>>) = parameters
        .iter()
        .zip(arguments)
        .filter(|&(&parameter, &argument)| {
            !matches!(parameter.kind(db), TypeKind::Callable { .. })
                || parameters
                    .iter()
                    .zip(arguments)
                    .all(|(&other, &received)| other != parameter || received == argument)
        })
        .map(|(&parameter, &argument)| (parameter, argument))
        .unzip();
    let substitution = Substitution {
        db,
        parameters: &listed,
        arguments: &replacing,
    };
    let mut body = generic.clone();
    let integers: Vec<(Type<'db>, Integer)> = parameters
        .iter()
        .zip(arguments)
        .filter_map(|(&parameter, argument)| match argument.kind(db) {
            TypeKind::Primitive(Primitive::Integer(integer)) => Some((parameter, *integer)),
            _ => None,
        })
        .collect();
    checked_arithmetic(db, &mut body, &integers);
    substitution.body(&mut body);
    let received = parameters
        .iter()
        .zip(arguments)
        .filter(|(parameter, _)| matches!(parameter.kind(db), TypeKind::Callable { .. }))
        .map(|(_, &argument)| argument);
    callable_parameters(db, &mut body, received);
    callables::copied(db, &mut body);
    direct_calls(db, &mut body);
    whole_array_arithmetic(db, &mut body);
    body
}

/// Checks for overflow the arithmetic of `body` and its outlined `prange` bodies whose type
/// parameter this instance makes an integer, as lowering checks the same arithmetic written on
/// an integer type (decision 0030). `integers` lists those parameters and their integer types.
///
/// Lowering writes `+`, `-` and `*` on a type parameter, and `-x` on one, into a local of the
/// parameter's type, so the destination names the arithmetic to check. Each becomes the form
/// lowering writes for an integer: a [`CheckedBinary`](Rvalue::CheckedBinary) into a
/// `tuple[T, bool]` local, an `Assert` that its flag is `False`, and the result, in a new block
/// that continues the rest of the old one. `-x` is `0 - x`, checked only for a signed integer,
/// as lowering does. The rewrite runs before substitution, so the new locals name the parameter
/// and substitution gives them the instance's types with the rest.
fn checked_arithmetic<'db>(
    db: &'db dyn salsa::Database,
    body: &mut Body<'db>,
    integers: &[(Type<'db>, Integer)],
) {
    if integers.is_empty() {
        return;
    }
    for outlined in &mut body.outlined {
        checked_arithmetic(db, outlined, integers);
    }
    let boolean = Type::new(db, TypeKind::Primitive(Primitive::Bool));
    let mut block = 0;
    while block < body.blocks.len() {
        let found =
            body.blocks[block]
                .statements
                .iter()
                .enumerate()
                .find_map(|(index, statement)| {
                    checked_operation(&body.locals, integers, &statement.kind)
                        .map(|operation| (index, operation, statement.span))
                });
        let Some((index, (place, operator, (left, right), ty), span)) = found else {
            block += 1;
            continue;
        };
        let rest = body.blocks[block].statements.split_off(index + 1);
        body.blocks[block].statements.pop();
        let pair = body.push_local(
            Type::new(db, TypeKind::Tuple(vec![ty, boolean])),
            LocalKind::Temporary,
            span,
        );
        let fits = body.push_local(boolean, LocalKind::Temporary, span);
        let flag = Operand::Copy(Place::from(pair).project(Projection::Field(1)));
        let result = Operand::Copy(Place::from(pair).project(Projection::Field(0)));
        let checks = [
            (
                Place::from(pair),
                Rvalue::CheckedBinary(operator, left, right),
            ),
            (Place::from(fits), Rvalue::Unary(UnaryOperator::Not, flag)),
        ];
        let assign = |(place, rvalue)| Statement {
            kind: StatementKind::Assign(place, rvalue),
            span,
        };
        body.blocks[block].statements.extend(checks.map(assign));
        let next = body.push_block(body.blocks[block].terminator.clone());
        body.blocks[next.index()].statements =
            std::iter::once(assign((place, Rvalue::Use(result))))
                .chain(rest)
                .collect();
        body.blocks[block].terminator = Terminator {
            kind: TerminatorKind::Assert {
                condition: Operand::Copy(Place::from(fits)),
                check: Check::NoOverflow(operator),
                target: next,
            },
            span,
        };
        block += 1;
    }
}

/// An integer `+`, `-`, `*` or `-x` of a type parameter in `integers` that `statement` computes
/// without a check: its destination, operator, operands and type. `-x` is `0 - x`.
fn checked_operation<'db>(
    locals: &[LocalDeclaration<'db>],
    integers: &[(Type<'db>, Integer)],
    statement: &StatementKind<'db>,
) -> Option<(
    Place,
    BinaryOperator,
    (Operand<'db>, Operand<'db>),
    Type<'db>,
)> {
    let StatementKind::Assign(place, rvalue) = statement else {
        return None;
    };
    if !place.projections.is_empty() {
        return None;
    }
    let ty = locals.get(place.local.index())?.ty;
    let &(_, integer) = integers.iter().find(|(parameter, _)| *parameter == ty)?;
    let (operator, operands) = match rvalue {
        Rvalue::Binary(
            operator @ (BinaryOperator::Add | BinaryOperator::Subtract | BinaryOperator::Multiply),
            left,
            right,
        ) => (*operator, (left.clone(), right.clone())),
        Rvalue::Unary(UnaryOperator::Negate, operand) if integer.is_signed() => {
            let zero = Operand::Constant(Constant {
                ty,
                value: ConstantValue::Integer(0),
            });
            (BinaryOperator::Subtract, (zero, operand.clone()))
        }
        _ => return None,
    };
    Some((place.clone(), operator, operands, ty))
}

/// Finds the method that implements a trait's method for a type (decision 0031): given the
/// trait's method and the receiver's type, the implementing method, the positions of the
/// embedded fields the receiver is reached through, outermost first, and the type arguments the
/// method is compiled at. `None` when the method is not a trait's. The application builds it
/// from the type checker, which knows which struct defines what.
pub type Implementation<'a, 'db> =
    dyn Fn(Item<'db>, Type<'db>) -> Option<(Item<'db>, Vec<u32>, Vec<Type<'db>>)> + 'a;

/// Turns each call of a trait's method in `body`, an instance's body after [`instantiate`],
/// into a direct call of the method that implements it for the receiver's type (decision
/// 0031), in `body` and its outlined `prange` bodies.
///
/// The type checker records a call `body.energy()` on a `T: Body` as calling `Body.energy` with
/// one type argument, `T`. Once `T` is `Particle`, `implementation` names `Particle.energy`,
/// and the call goes there directly, so the backend can inline it: a trait costs nothing at run
/// time. When the method comes from an embedded struct, the receiver steps into the embedded
/// fields, so `Ion` passes its `particle` as `self`.
///
/// Cost: one call of `implementation` per call in the body with one type argument.
///
/// ```
/// use mir::{Body, Terminator, TerminatorKind};
/// use source::{FileId, Span};
/// use types::{Type, TypeKind};
///
/// # #[salsa::db]
/// # #[derive(Clone, Default)]
/// # struct Database { storage: salsa::Storage<Self> }
/// # #[salsa::db]
/// # impl salsa::Database for Database {}
/// let database = Database::default();
/// let span = Span::new(FileId::from_index(0), 0, 1);
/// let none = Type::new(&database, TypeKind::None);
/// let mut body = Body::new(none, span);
/// body.push_block(Terminator { kind: TerminatorKind::Return, span });
///
/// // A body without trait calls is left as it is.
/// let before = body.clone();
/// monomorphize::trait_calls(&mut body, &|_, _| None);
/// assert_eq!(body, before);
/// ```
pub fn trait_calls<'db>(body: &mut Body<'db>, implementation: &Implementation<'_, 'db>) {
    for outlined in &mut body.outlined {
        trait_calls(outlined, implementation);
    }
    for block in &mut body.blocks {
        let TerminatorKind::Call {
            callee:
                Callee::Function {
                    item,
                    type_arguments,
                },
            arguments,
            ..
        } = &mut block.terminator.kind
        else {
            continue;
        };
        let [receiver_type] = type_arguments.as_slice() else {
            continue;
        };
        let Some((method, path, method_arguments)) = implementation(*item, *receiver_type) else {
            continue;
        };
        *item = method;
        *type_arguments = method_arguments;
        let Some(receiver) = arguments.first_mut() else {
            continue;
        };
        let place = match receiver {
            Argument::Borrow(place)
            | Argument::InOut(place)
            | Argument::Owned(Operand::Copy(place) | Operand::Move(place)) => place,
            Argument::Owned(Operand::Constant(_)) => continue,
        };
        place
            .projections
            .extend(path.into_iter().map(Projection::Field));
    }
}

/// Gives each `Callable` parameter's local of `body`, in order, the type of the function it
/// `received`, so a call of it becomes a direct call of that function even when another
/// `Callable` parameter shares its signature.
fn callable_parameters<'db>(
    db: &'db dyn salsa::Database,
    body: &mut Body<'db>,
    mut received: impl Iterator<Item = Type<'db>>,
) {
    for local in &mut body.locals {
        if matches!(local.kind, LocalKind::Parameter { .. })
            && matches!(
                local.ty.kind(db),
                TypeKind::Callable { .. } | TypeKind::Function { .. }
            )
            && let Some(function) = received.next()
        {
            local.ty = function;
        }
    }
}

/// Turns arithmetic whose result is now an array into a whole-array expression, in `body` and
/// its outlined `prange` bodies. Under a `VectorSpace` bound, `state + h * k` on a type
/// parameter lowers to scalar operators; once `S` is `Fixed[f64, (3,)]` or an `Array`, array
/// expansion must loop over the elements, as it does for the same expression written on
/// arrays. A scalar operand, such as `h`, is a leaf that every element uses.
fn whole_array_arithmetic<'db>(db: &'db dyn salsa::Database, body: &mut Body<'db>) {
    for outlined in &mut body.outlined {
        whole_array_arithmetic(db, outlined);
    }
    for block in &mut body.blocks {
        for statement in &mut block.statements {
            let StatementKind::Assign(place, rvalue) = &mut statement.kind else {
                continue;
            };
            if !place.projections.is_empty() {
                continue;
            }
            let ty = body.locals[place.local.index()].ty;
            if !matches!(ty.kind(db), TypeKind::Fixed { .. } | TypeKind::Array { .. }) {
                continue;
            }
            let leaf = |operand: &Operand<'db>| Box::new(Elementwise::Leaf(operand.clone()));
            let tree = match rvalue {
                Rvalue::Binary(operator, left, right) => {
                    Elementwise::Binary(*operator, leaf(left), leaf(right))
                }
                Rvalue::Unary(operator, operand) => Elementwise::Unary(*operator, leaf(operand)),
                _ => continue,
            };
            *rvalue = Rvalue::Elementwise(Box::new(tree));
        }
    }
}

/// Turns each call of a local whose type is now one function's own type into a direct call of
/// that function, in `body` and its outlined `prange` bodies. After substitution, a `Callable`
/// parameter holds the function this instance received (decision 0026), so the call needs no
/// function value and the backend can inline it.
fn direct_calls<'db>(db: &'db dyn salsa::Database, body: &mut Body<'db>) {
    for outlined in &mut body.outlined {
        direct_calls(db, outlined);
    }
    for block in &mut body.blocks {
        let TerminatorKind::Call { callee, .. } = &mut block.terminator.kind else {
            continue;
        };
        let Callee::Value(Operand::Copy(place) | Operand::Move(place)) = callee else {
            continue;
        };
        if !place.projections.is_empty() {
            continue;
        }
        if let TypeKind::Function { item, .. } = body.locals[place.local.index()].ty.kind(db) {
            *callee = Callee::Function {
                item: *item,
                type_arguments: Vec::new(),
            };
        }
    }
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
        if let Some(position) = self
            .parameters
            .iter()
            .position(|&parameter| parameter == ty)
        {
            return self.arguments[position];
        }
        let kind = match ty.kind(self.db) {
            TypeKind::Parameter { .. } => return ty,
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
