//! The text form of a body, for tests and for reading what lowering and the passes produce.
//!
//! The form follows Rust's MIR dumps, so it reads familiarly to anyone who has seen one: locals
//! are `_0`, `_1` and so on, blocks are `bb0`, `bb1`, and each operation names its operator in
//! full. The header block of a `simd` loop reads `bb2 (simd):`. Each outlined `prange` body
//! follows the function, as `body0: fn(…)`, `body1` and so on.
//! It is for people, not a format another stage parses.

use crate::{
    Argument, BinaryOperator, BlockId, Body, Callee, Check, ConstantValue, Elementwise, LocalKind,
    Operand, Piece, Place, Projection, Rvalue, StatementKind, TerminatorKind, UnaryOperator,
};
use database::Db;
use std::fmt::{self, Write};
use types::{Mode, Type};

impl<'db> Body<'db> {
    /// A value that writes the body's text form. The database is needed to write types and the
    /// names of called functions.
    ///
    /// ```
    /// use mir::{Body, Terminator, TerminatorKind};
    /// use source::{FileId, Span};
    /// use types::{Float, Primitive, Type, TypeKind};
    /// # #[salsa::db]
    /// # #[derive(Clone, Default)]
    /// # struct Database { storage: salsa::Storage<Self> }
    /// # #[salsa::db]
    /// # impl salsa::Database for Database {}
    /// # #[salsa::db]
    /// # impl database::Db for Database {
    /// #     fn parse(&self, _: database::File) -> &database::ParsedFile { unreachable!() }
    /// #     fn resolve(&self, _: database::Package, _: database::File) -> &database::Resolution {
    /// #         unreachable!()
    /// #     }
    /// # }
    /// # let database = Database::default();
    /// let none = Type::new(&database, TypeKind::None);
    /// let span = Span::new(FileId::from_index(0), 0, 10);
    /// let mut body = Body::new(none, span);
    /// body.push_block(Terminator { kind: TerminatorKind::Return, span });
    ///
    /// assert_eq!(
    ///     body.display(&database).to_string(),
    ///     "fn() -> None {\n    let _0: None; // return\n\n    bb0: {\n        return;\n    }\n}\n",
    /// );
    /// ```
    #[must_use]
    pub fn display<'a>(&'a self, db: &'a dyn Db) -> DisplayedBody<'a, 'db> {
        DisplayedBody { db, body: self }
    }
}

/// A body ready to write as text. Created by [`Body::display`].
#[derive(Clone, Copy)]
pub struct DisplayedBody<'a, 'db> {
    /// The database the body's types and items live in.
    db: &'a dyn Db,
    /// The body to write.
    body: &'a Body<'db>,
}

impl fmt::Debug for DisplayedBody<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for DisplayedBody<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let writer = Writer { db: self.db };
        let body = self.body;
        let parameters = body
            .locals
            .iter()
            .enumerate()
            .skip(1)
            .take(body.parameter_count as usize)
            .map(|(index, local)| {
                let ty = writer.ty(local.ty);
                match local.kind {
                    LocalKind::Parameter {
                        mode: Mode::InOut, ..
                    } => format!("_{index}: InOut[{ty}]"),
                    LocalKind::Parameter {
                        mode: Mode::Owned, ..
                    } => format!("_{index}: Owned[{ty}]"),
                    _ => format!("_{index}: {ty}"),
                }
            })
            .collect::<Vec<_>>();
        let returns = body
            .locals
            .first()
            .map_or_else(|| "?".to_owned(), |local| writer.ty(local.ty));
        writeln!(formatter, "fn({}) -> {returns} {{", parameters.join(", "))?;
        for (index, local) in body.locals.iter().enumerate() {
            let note = match &local.kind {
                LocalKind::Return => " // return".to_owned(),
                LocalKind::Parameter { name, .. } => format!(" // parameter {name}"),
                LocalKind::Variable { name } => format!(" // {name}"),
                LocalKind::Temporary => String::new(),
            };
            writeln!(
                formatter,
                "    let _{index}: {};{note}",
                writer.ty(local.ty)
            )?;
        }
        for (index, block) in body.blocks.iter().enumerate() {
            let mark = if body.vectorized.contains(&BlockId::new(index)) {
                " (simd)"
            } else {
                ""
            };
            writeln!(formatter, "\n    bb{index}{mark}: {{")?;
            for statement in &block.statements {
                let text = match &statement.kind {
                    StatementKind::Assign(place, rvalue) => {
                        format!("{} = {}", place_text(place), writer.rvalue(rvalue))
                    }
                    StatementKind::Drop(place) => format!("drop({})", place_text(place)),
                    StatementKind::Accumulate {
                        place,
                        operator,
                        value,
                    } => format!(
                        "accumulate {} {} {}",
                        place_text(place),
                        operator.symbol(),
                        writer.operand(value)
                    ),
                };
                writeln!(formatter, "        {text};")?;
            }
            writeln!(
                formatter,
                "        {};",
                writer.terminator(&block.terminator.kind)
            )?;
            writeln!(formatter, "    }}")?;
        }
        writeln!(formatter, "}}")?;
        for (position, outlined) in body.outlined.iter().enumerate() {
            write!(formatter, "\nbody{position}: {}", outlined.display(self.db))?;
        }
        Ok(())
    }
}

/// Writes the parts of a body that need the database.
#[derive(Clone, Copy)]
struct Writer<'a> {
    /// The database types and items are read from.
    db: &'a dyn Db,
}

impl Writer<'_> {
    /// A type as a program spells it.
    fn ty(self, ty: Type<'_>) -> String {
        ty.display(self.db).to_string()
    }

    /// An operand: `copy _1`, `move _2`, or a constant with its type, `2.0_f64`.
    fn operand(self, operand: &Operand<'_>) -> String {
        match operand {
            Operand::Copy(place) => format!("copy {}", place_text(place)),
            Operand::Move(place) => format!("move {}", place_text(place)),
            Operand::Constant(constant) => {
                let ty = self.ty(constant.ty);
                match &constant.value {
                    ConstantValue::Bool(true) => "True".to_owned(),
                    ConstantValue::Bool(false) => "False".to_owned(),
                    ConstantValue::Integer(value) => format!("{value}_{ty}"),
                    ConstantValue::Float(bits) => format!("{:?}_{ty}", f64::from_bits(*bits)),
                    ConstantValue::Complex { real, imaginary } => format!(
                        "({:?}+{:?}j)_{ty}",
                        f64::from_bits(*real),
                        f64::from_bits(*imaginary)
                    ),
                    ConstantValue::Str(text) => format!("{text:?}"),
                    ConstantValue::None => "None".to_owned(),
                }
            }
        }
    }

    /// Operands separated by commas.
    fn operands(self, operands: &[Operand<'_>]) -> String {
        let texts = operands.iter().map(|operand| self.operand(operand));
        texts.collect::<Vec<_>>().join(", ")
    }

    /// An rvalue: `Add(copy _1, copy _2)`, `f32(copy _1)`, `len(_1, 0)`.
    fn rvalue(self, rvalue: &Rvalue<'_>) -> String {
        match rvalue {
            Rvalue::Use(operand) => self.operand(operand),
            Rvalue::Unary(operator, operand) => {
                format!("{}({})", unary_name(*operator), self.operand(operand))
            }
            Rvalue::Binary(operator, left, right) => format!(
                "{}({}, {})",
                binary_name(*operator),
                self.operand(left),
                self.operand(right)
            ),
            Rvalue::CheckedBinary(operator, left, right) => format!(
                "Checked{}({}, {})",
                binary_name(*operator),
                self.operand(left),
                self.operand(right)
            ),
            Rvalue::Convert(operand, ty) => format!("{}({})", self.ty(*ty), self.operand(operand)),
            Rvalue::Aggregate(ty, operands) => {
                format!("{}({})", self.ty(*ty), self.operands(operands))
            }
            Rvalue::NewArray(ty, lengths) => {
                format!("new {}({})", self.ty(*ty), self.operands(lengths))
            }
            Rvalue::Length(place, dimension) => format!("len({}, {dimension})", place_text(place)),
            Rvalue::IsPresent(place) => format!("is_present({})", place_text(place)),
            Rvalue::ErrorIndex(place) => format!("error_index({})", place_text(place)),
            Rvalue::Elementwise(tree) => format!("elementwise {}", self.elementwise(tree)),
            Rvalue::Format(pieces) => {
                let pieces: Vec<String> = pieces
                    .iter()
                    .map(|piece| match piece {
                        Piece::Text(text) => format!("{text:?}"),
                        Piece::Value(place, spec) => {
                            format!("{{{}:{spec}}}", place_text(place))
                        }
                    })
                    .collect();
                format!("format({})", pieces.join(", "))
            }
        }
    }

    /// A whole-array expression tree, written as nested operations.
    fn elementwise(self, tree: &Elementwise<'_>) -> String {
        match tree {
            Elementwise::Leaf(operand) => self.operand(operand),
            Elementwise::Unary(operator, operand) => {
                format!("{}({})", unary_name(*operator), self.elementwise(operand))
            }
            Elementwise::Binary(operator, left, right) => format!(
                "{}({}, {})",
                binary_name(*operator),
                self.elementwise(left),
                self.elementwise(right)
            ),
        }
    }

    /// A terminator, without its closing semicolon.
    fn terminator(self, terminator: &TerminatorKind<'_>) -> String {
        match terminator {
            TerminatorKind::Goto(target) => format!("goto -> bb{}", target.index()),
            TerminatorKind::Branch {
                condition,
                then,
                otherwise,
            } => format!(
                "branch {} -> [true: bb{}, false: bb{}]",
                self.operand(condition),
                then.index(),
                otherwise.index()
            ),
            TerminatorKind::Switch {
                value,
                cases,
                otherwise,
            } => {
                let mut text = format!("switch {} -> [", self.operand(value));
                for (case, target) in cases {
                    let _ = write!(text, "{case}: bb{}, ", target.index());
                }
                let _ = write!(text, "otherwise: bb{}]", otherwise.index());
                text
            }
            TerminatorKind::Return => "return".to_owned(),
            TerminatorKind::Raise(error) => format!("raise {}", self.operand(error)),
            TerminatorKind::Call {
                callee,
                arguments,
                destination,
                target,
                error,
            } => {
                let arguments = self.arguments(arguments);
                let error = error.as_ref().map_or_else(String::new, |edge| {
                    format!(", error _{}: bb{}", edge.error.index(), edge.target.index())
                });
                format!(
                    "{} = call {}({}) -> [return: bb{}{error}]",
                    place_text(destination),
                    self.callee(callee),
                    arguments.join(", "),
                    target.index()
                )
            }
            TerminatorKind::ParallelFor {
                body,
                count,
                arguments,
                reductions,
                accumulations,
                target,
            } => {
                let mut values = self.arguments(arguments);
                values.extend(reductions.iter().map(|reduction| {
                    let operator = reduction.operator.name();
                    format!("reduce {operator} {}", place_text(&reduction.place))
                }));
                values.extend(
                    accumulations
                        .iter()
                        .map(|place| format!("accumulate {}", place_text(place))),
                );
                format!(
                    "parallel_for body{body}({}; {}) -> bb{}",
                    self.operand(count),
                    values.join(", "),
                    target.index()
                )
            }
            TerminatorKind::Assert {
                condition,
                check,
                target,
            } => format!(
                "assert({}, {}) -> bb{}",
                self.operand(condition),
                self.check(check),
                target.index()
            ),
            TerminatorKind::Unreachable => "unreachable".to_owned(),
        }
    }

    /// Call arguments, each marked with its mode: `_1`, `inout _2`, `owned copy _3`.
    fn arguments(self, arguments: &[Argument<'_>]) -> Vec<String> {
        arguments
            .iter()
            .map(|argument| match argument {
                Argument::Borrow(place) => place_text(place),
                Argument::InOut(place) => format!("inout {}", place_text(place)),
                Argument::Owned(operand) => format!("owned {}", self.operand(operand)),
            })
            .collect()
    }

    /// The function a call calls: its name and type arguments, the operand holding it, or a
    /// built-in's name.
    fn callee(self, callee: &Callee<'_>) -> String {
        match callee {
            Callee::Function {
                item,
                type_arguments,
            } => {
                let file = item.file(self.db);
                let name = self
                    .db
                    .parse(file)
                    .tree()
                    .text(item.name(self.db), file.source(self.db).text());
                if type_arguments.is_empty() {
                    name.to_owned()
                } else {
                    let arguments = type_arguments.iter().map(|&ty| self.ty(ty));
                    format!("{name}[{}]", arguments.collect::<Vec<_>>().join(", "))
                }
            }
            Callee::Value(operand) => format!("({})", self.operand(operand)),
            Callee::Builtin(builtin) => builtin.name().to_owned(),
        }
    }

    /// What an assert checks, as its panic message will say it.
    fn check(self, check: &Check<'_>) -> String {
        match check {
            Check::IndexInBounds {
                dimension,
                index,
                length,
            } => format!(
                "index in bounds in dimension {dimension}: {} < {}",
                self.operand(index),
                self.operand(length)
            ),
            Check::NoOverflow(operator) => format!("no overflow: {}", binary_name(*operator)),
            Check::NonZeroDivisor => "non-zero divisor".to_owned(),
            Check::SameLength {
                dimension,
                expected,
                found,
            } => format!(
                "same length in dimension {dimension}: {} == {}",
                self.operand(found),
                self.operand(expected)
            ),
            Check::Assertion => "assertion".to_owned(),
        }
    }
}

/// A place: `_1`, `_1.0`, `_1[_2, _3]`, `_1.value`, `_1.error[0]`.
fn place_text(place: &Place) -> String {
    let mut text = format!("_{}", place.local.index());
    for projection in &place.projections {
        let _ = match projection {
            Projection::Field(field) => write!(text, ".{field}"),
            Projection::Index(indices) => {
                let indices = indices.iter().map(|local| format!("_{}", local.index()));
                write!(text, "[{}]", indices.collect::<Vec<_>>().join(", "))
            }
            Projection::Value => write!(text, ".value"),
            Projection::Error(position) => write!(text, ".error[{position}]"),
        };
    }
    text
}

/// The full name of a one-operand operator.
fn unary_name(operator: UnaryOperator) -> &'static str {
    match operator {
        UnaryOperator::Negate => "Negate",
        UnaryOperator::Not => "Not",
        UnaryOperator::BitNot => "BitNot",
    }
}

/// The full name of a two-operand operator.
fn binary_name(operator: BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Add => "Add",
        BinaryOperator::Subtract => "Subtract",
        BinaryOperator::Multiply => "Multiply",
        BinaryOperator::Divide => "Divide",
        BinaryOperator::FloorDivide => "FloorDivide",
        BinaryOperator::Remainder => "Remainder",
        BinaryOperator::BitAnd => "BitAnd",
        BinaryOperator::BitOr => "BitOr",
        BinaryOperator::BitXor => "BitXor",
        BinaryOperator::ShiftLeft => "ShiftLeft",
        BinaryOperator::ShiftRight => "ShiftRight",
        BinaryOperator::Equal => "Equal",
        BinaryOperator::NotEqual => "NotEqual",
        BinaryOperator::Less => "Less",
        BinaryOperator::LessEqual => "LessEqual",
        BinaryOperator::Greater => "Greater",
        BinaryOperator::GreaterEqual => "GreaterEqual",
    }
}
