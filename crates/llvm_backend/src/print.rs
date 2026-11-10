//! `print`: a call of [`Builtin::Print`](mir::Builtin::Print) writes each argument in turn.
//! Lowering passes the separator and the line end as arguments of their own, so this module
//! writes nothing between them.
//!
//! A number, a `bool` or a `str` is one runtime call (`baryon_rt_print_*`, roadmap 1.12.5),
//! which formats it as Python does. A signed integer is widened to `i64` and an unsigned one to
//! `u64`, so two functions cover every width; a float keeps its width, so an `f32` prints its own
//! shortest digits. A string literal, which lowering passes as a constant, becomes a byte
//! constant printed with `baryon_rt_print_str`. A `str` value prints its bytes the same way, with
//! the "not owned" bit masked off its length (decision 0022). Each runtime call names its stream
//! first: 1, standard output, for `print`.
//!
//! Every other value is taken apart here, at compile time, into those calls and byte constants,
//! as Python's `print` writes it (roadmap 1.10.12):
//!
//! | Value | Printed | Inside a tuple or struct |
//! | --- | --- | --- |
//! | `None` | `None` | `None` |
//! | `T \| None` | the `T`, or `None` | the same |
//! | a tuple | `(1, 2.5)`, `(1,)`, `()` | the same |
//! | an enum member | `Phase.GAS` | the same |
//! | a struct | `Point(x=1.0, y=2.0)` | the same |
//! | a `str` | its characters | quoted, as Python's `repr` writes it: `'heat'` |
//!
//! A tuple's items and a struct's fields print as Python's `repr` writes them, which differs from
//! `print` only for a `str`. An enum member prints `Phase.GAS` everywhere, where Python's `repr`
//! inside a container would write `<Phase.GAS: 1>`; the compiler does not keep an enum member's
//! value, only its position. The code grows with the type, not the value: a struct of ten fields
//! is ten runtime calls, and an enum is one `switch`. Arrays and lists wait on decision D.17.

use crate::Unsupported;
use crate::function::{Function, block_label};
use crate::ty::Number;
use mir::{Argument, BlockId, ConstantValue, Operand};
use types::{Declared, Type, TypeKind};

/// The runtime's number for standard output, where `print` writes.
pub(crate) const STDOUT: u8 = 1;

/// The runtime's number for standard error, where the report of an error that ends `main`
/// goes.
pub(crate) const STDERR: u8 = 2;

/// How a value is written: as `print` writes it, or as Python's `repr` writes it inside a tuple
/// or struct. The two differ only for a `str`, which `repr` quotes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Form {
    /// As `print` writes the value itself: a `str` unquoted.
    Print,
    /// As Python's `repr` writes it: a `str` quoted, `'heat'`.
    Repr,
}

impl<'db> Function<'_, '_, 'db> {
    /// Writes `print(arguments…)` and goes to `target`.
    pub(crate) fn print(
        &mut self,
        arguments: &[Argument<'db>],
        target: BlockId,
    ) -> Result<(), Unsupported> {
        for argument in arguments {
            self.print_argument(argument)?;
        }
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// Prints one argument. A `None` has no value to load, so it prints its name directly.
    fn print_argument(&mut self, argument: &Argument<'db>) -> Result<(), Unsupported> {
        let printed = match argument {
            Argument::Owned(Operand::Constant(constant)) => {
                if let ConstantValue::Str(text) = &constant.value {
                    self.print_text(STDOUT, text.as_bytes());
                    return Ok(());
                }
                self.operand(&Operand::Constant(constant.clone()))?
            }
            Argument::Owned(operand) => self.operand(operand)?,
            Argument::Borrow(place) | Argument::InOut(place) => {
                let (address, ty) = self.place(place)?;
                self.load(&address, ty)?.map(|value| (value, ty))
            }
        };
        if let Some((value, ty)) = printed {
            return self.print_value(STDOUT, &value, ty, Form::Print);
        }
        self.print_text(STDOUT, b"None");
        Ok(())
    }

    /// Prints `value`, of type `ty`, to `stream` in `form`: numbers, `bool` and `str` through
    /// the runtime, anything else taken apart into them.
    pub(crate) fn print_value(
        &mut self,
        stream: u8,
        value: &str,
        ty: Type<'db>,
        form: Form,
    ) -> Result<(), Unsupported> {
        match ty.kind(self.types.db) {
            TypeKind::Str if form == Form::Repr => self.print_quoted(stream, value),
            TypeKind::Str => {
                let (bytes, length) = self.text_parts(value);
                self.print_bytes(stream, &bytes, &length);
            }
            TypeKind::Optional(inner) => self.print_optional(stream, value, ty, *inner, form)?,
            TypeKind::Tuple(items) => self.print_tuple(stream, value, ty, items)?,
            TypeKind::Struct(nominal) => {
                let fields = match (self.types.declared)(nominal) {
                    Declared::Struct(fields) => fields,
                    Declared::Enum { .. } => return Err(self.printing(ty)),
                };
                self.print_struct(stream, value, ty, nominal, &fields)?;
            }
            TypeKind::Enum(nominal) => self.print_enum(stream, value, ty, nominal)?,
            TypeKind::Primitive(_) => self.print_number(stream, value, ty)?,
            _ => return Err(self.printing(ty)),
        }
        Ok(())
    }

    /// Prints the `T | None` `value`, of type `ty`: its `T` in `form` when present, else
    /// `None`. A branch on the presence flag picks which.
    fn print_optional(
        &mut self,
        stream: u8,
        value: &str,
        ty: Type<'db>,
        inner: Type<'db>,
        form: Form,
    ) -> Result<(), Unsupported> {
        let llvm = self.types.value(ty)?;
        let present = self.emit(&format!("extractvalue {llvm} {value}, 1"));
        let (some, none, done) = (self.fresh_block(), self.fresh_block(), self.fresh_block());
        self.line(&format!("br i1 {present}, label %{some}, label %{none}"));
        self.line_label(&some);
        let held = self.emit(&format!("extractvalue {llvm} {value}, 0"));
        self.print_value(stream, &held, inner, form)?;
        self.line(&format!("br label %{done}"));
        self.line_label(&none);
        self.print_text(stream, b"None");
        self.line(&format!("br label %{done}"));
        self.line_label(&done);
        Ok(())
    }

    /// Prints the tuple `value`, of type `ty`, as Python does: `(1, 'a')`, with a comma after a
    /// lone item, `(1,)`, so it does not read as a parenthesized value.
    fn print_tuple(
        &mut self,
        stream: u8,
        value: &str,
        ty: Type<'db>,
        items: &[Type<'db>],
    ) -> Result<(), Unsupported> {
        let llvm = self.types.value(ty)?;
        self.print_text(stream, b"(");
        for (position, &item) in items.iter().enumerate() {
            if position > 0 {
                self.print_text(stream, b", ");
            }
            let held = self.emit(&format!("extractvalue {llvm} {value}, {position}"));
            self.print_value(stream, &held, item, Form::Repr)?;
        }
        if items.len() == 1 {
            self.print_text(stream, b",");
        }
        self.print_text(stream, b")");
        Ok(())
    }

    /// Prints the struct `value`, of type `ty`, whose field types are `fields`, as a dataclass
    /// prints: `Point(x=1.0, y=2.0)`, the class's name without its type arguments, then each
    /// field by name, in declaration order.
    fn print_struct(
        &mut self,
        stream: u8,
        value: &str,
        ty: Type<'db>,
        nominal: &types::Nominal<'db>,
        fields: &[Type<'db>],
    ) -> Result<(), Unsupported> {
        let names = (self.types.names)(nominal);
        if names.members.len() != fields.len() {
            return Err(self.printing(ty));
        }
        let llvm = self.types.value(ty)?;
        self.print_text(stream, format!("{}(", names.class).as_bytes());
        for (position, (field, &field_ty)) in names.members.iter().zip(fields).enumerate() {
            let separator = if position == 0 { "" } else { ", " };
            self.print_text(stream, format!("{separator}{field}=").as_bytes());
            let held = self.emit(&format!("extractvalue {llvm} {value}, {position}"));
            self.print_value(stream, &held, field_ty, Form::Repr)?;
        }
        self.print_text(stream, b")");
        Ok(())
    }

    /// Prints the enum member `value`, of type `ty`, as `Phase.GAS`: a `switch` on its position
    /// to one block per member, each printing its own name. The last member is the `switch`'s
    /// default, since the value is always one of the members.
    fn print_enum(
        &mut self,
        stream: u8,
        value: &str,
        ty: Type<'db>,
        nominal: &types::Nominal<'db>,
    ) -> Result<(), Unsupported> {
        let names = (self.types.names)(nominal);
        let llvm = self.types.value(ty)?;
        let Some((last, others)) = names.members.split_last() else {
            return Err(self.printing(ty));
        };
        let blocks: Vec<String> = others.iter().map(|_| self.fresh_block()).collect();
        let (default, done) = (self.fresh_block(), self.fresh_block());
        let cases: Vec<String> = blocks
            .iter()
            .enumerate()
            .map(|(position, block)| format!("{llvm} {position}, label %{block}"))
            .collect();
        self.line(&format!(
            "switch {llvm} {value}, label %{default} [{}]",
            cases.join(" ")
        ));
        let members = others.iter().zip(&blocks).chain([(last, &default)]);
        for (member, block) in members {
            self.line_label(block);
            self.print_text(stream, format!("{}.{member}", names.class).as_bytes());
            self.line(&format!("br label %{done}"));
        }
        self.line_label(&done);
        Ok(())
    }

    /// Prints the number or `bool` `value`, of type `ty`, with the runtime function for its
    /// type.
    fn print_number(&mut self, stream: u8, value: &str, ty: Type<'db>) -> Result<(), Unsupported> {
        let number = self.types.number(ty).ok_or_else(|| self.printing(ty))?;
        let (function, parameter, value) = match number {
            Number::Bool => ("baryon_rt_print_bool", "i1 zeroext", value.to_owned()),
            Number::Float(32) => ("baryon_rt_print_f32", "float", value.to_owned()),
            Number::Float(_) => ("baryon_rt_print_f64", "double", value.to_owned()),
            Number::Integer { bits, signed } => {
                let function = if signed {
                    "baryon_rt_print_i64"
                } else {
                    "baryon_rt_print_u64"
                };
                let value = match (bits, signed) {
                    (64, _) => value.to_owned(),
                    (_, true) => self.emit(&format!("sext i{bits} {value} to i64")),
                    (_, false) => self.emit(&format!("zext i{bits} {value} to i64")),
                };
                (function, "i64", value)
            }
        };
        self.declare(
            function,
            &format!("declare void @{function}(i32, {parameter}) nounwind"),
        );
        self.line(&format!(
            "call void @{function}(i32 {stream}, {parameter} {value})"
        ));
        Ok(())
    }

    /// The error for a value of type `ty` this backend cannot print yet.
    fn printing(&self, ty: Type<'db>) -> Unsupported {
        Unsupported::new(format!(
            "printing values of type `{}`",
            ty.display(self.types.db)
        ))
    }

    /// Prints the bytes `text` to `stream` through a byte constant.
    pub(crate) fn print_text(&mut self, stream: u8, text: &[u8]) {
        let constant = self.constants.bytes(text);
        self.print_bytes(stream, &constant, &text.len().to_string());
    }

    /// Prints the `str` value `value` to `stream` quoted, as Python's `repr` writes it.
    fn print_quoted(&mut self, stream: u8, value: &str) {
        let (bytes, length) = self.text_parts(value);
        self.declare(
            "baryon_rt_print_str_quoted",
            "declare void @baryon_rt_print_str_quoted(i32, ptr, i64) nounwind",
        );
        self.line(&format!(
            "call void @baryon_rt_print_str_quoted(i32 {stream}, ptr {bytes}, i64 {length})"
        ));
    }

    /// Prints the `length` bytes at `bytes` to `stream`.
    fn print_bytes(&mut self, stream: u8, bytes: &str, length: &str) {
        self.declare(
            "baryon_rt_print_str",
            "declare void @baryon_rt_print_str(i32, ptr, i64) nounwind",
        );
        self.line(&format!(
            "call void @baryon_rt_print_str(i32 {stream}, ptr {bytes}, i64 {length})"
        ));
    }
}
