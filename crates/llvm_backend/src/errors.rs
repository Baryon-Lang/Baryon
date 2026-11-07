//! Errors (decisions 0005 and 0018): what a function that can fail returns, `raise`, and
//! reading the error a failed call left.
//!
//! An error value, the local an error edge writes, has type `Raises[None, E1 | E2 …]`. Its
//! layout is the one [`types::layout`] gives: a one-byte tag, the error's position in the list,
//! then room for the largest error. In LLVM it is `{ i8, [n x iA] }`, where the array has the
//! payload's alignment `A` and covers its size. [`Error`](mir::Projection::Error) reads the
//! payload as one error type, and [`ErrorIndex`](mir::Rvalue::ErrorIndex) reads the tag.
//!
//! A function that can fail writes its error through its last parameter, `%error`, and returns
//! whether it failed beside its value (see the `call` module). Failing costs a store and a
//! branch; succeeding costs a flag in a register. Nothing unwinds.

use crate::Unsupported;
use crate::function::Function;
use crate::ty::Repr;
use mir::{Operand, Place};
use types::{Type, TypeKind};

impl<'db> Function<'_, '_, 'db> {
    /// The LLVM return type of the function: its value's type, or `void` for `None`; and for a
    /// function that can fail, `{ T, i1 }` with the failure flag, or `i1` alone.
    pub(crate) fn returns(&self) -> Result<String, Unsupported> {
        let value = self.types.repr(self.body.locals[0].ty)?;
        Ok(match (value, self.errors.is_empty()) {
            (Repr::Empty, true) => "void".to_owned(),
            (Repr::Value(llvm), true) => llvm,
            (Repr::Empty, false) => "i1".to_owned(),
            (Repr::Value(llvm), false) => format!("{{ {llvm}, i1 }}"),
        })
    }

    /// Returns the value in the return local, with the flag that says the function succeeded
    /// when it can fail.
    pub(crate) fn return_value(&mut self) -> Result<(), Unsupported> {
        let returns = self.returns()?;
        match self.types.repr(self.body.locals[0].ty)? {
            Repr::Empty if self.errors.is_empty() => self.line("ret void"),
            Repr::Empty => self.line("ret i1 false"),
            Repr::Value(llvm) => {
                let value = self.emit(&format!("load {llvm}, ptr %_0"));
                if self.errors.is_empty() {
                    self.line(&format!("ret {llvm} {value}"));
                } else {
                    let result = self.emit(&format!(
                        "insertvalue {returns} {{ {llvm} poison, i1 false }}, {llvm} {value}, 0"
                    ));
                    self.line(&format!("ret {returns} {result}"));
                }
            }
        }
        Ok(())
    }

    /// Fails with the error `operand`: writes it and its position in the function's error list
    /// through `%error`, and returns with the failure flag set.
    pub(crate) fn raise(&mut self, operand: &Operand<'db>) -> Result<(), Unsupported> {
        let (value, ty) = self.value(operand)?;
        let Some(position) = self.errors.iter().position(|&error| error == ty) else {
            return Err(Unsupported::new(format!(
                "raising `{}`, which the function does not list",
                ty.display(self.types.db)
            )));
        };
        let fallible = self.fallible()?;
        let tag = self.emit(&format!(
            "getelementptr inbounds {fallible}, ptr %error, i32 0, i32 0"
        ));
        self.line(&format!("store i8 {position}, ptr {tag}"));
        let payload = self.emit(&format!(
            "getelementptr inbounds {fallible}, ptr %error, i32 0, i32 1"
        ));
        let llvm = self.types.value(ty)?;
        self.line(&format!("store {llvm} {value}, ptr {payload}"));
        let returns = self.returns()?;
        match self.types.repr(self.body.locals[0].ty)? {
            Repr::Empty => self.line("ret i1 true"),
            Repr::Value(value) => {
                self.line(&format!("ret {returns} {{ {value} poison, i1 true }}"));
            }
        }
        Ok(())
    }

    /// The LLVM type of the function's own error value, `Raises[None, …]` over its error list.
    fn fallible(&self) -> Result<String, Unsupported> {
        let none = Type::new(self.types.db, TypeKind::None);
        let ty = Type::new(
            self.types.db,
            TypeKind::Fallible {
                value: none,
                errors: self.errors.to_vec(),
            },
        );
        self.types.value(ty)
    }

    /// The address of the error payload of the error value of type `ty` at `address`, read as
    /// the error at `position` in its list, and that error's type.
    pub(crate) fn error_payload(
        &mut self,
        address: &str,
        ty: Type<'db>,
        position: u32,
    ) -> Result<(String, Type<'db>), Unsupported> {
        let TypeKind::Fallible { errors, .. } = ty.kind(self.types.db) else {
            return Err(self.types.unsupported(ty));
        };
        let error = *errors
            .get(position as usize)
            .ok_or_else(|| self.types.unsupported(ty))?;
        let llvm = self.types.value(ty)?;
        let payload = self.emit(&format!(
            "getelementptr inbounds {llvm}, ptr {address}, i32 0, i32 1"
        ));
        Ok((payload, error))
    }

    /// Which error the error value at `place` holds, as an `i64`: its tag, widened.
    pub(crate) fn error_index(&mut self, place: &Place) -> Result<String, Unsupported> {
        let (address, ty) = self.place(place)?;
        let tag = self.error_tag(&address, ty)?;
        Ok(self.emit(&format!("zext i8 {tag} to i64")))
    }

    /// The tag of the error value of type `ty` at `address`, an `i8`: the position of the error
    /// it holds in its list.
    pub(crate) fn error_tag(
        &mut self,
        address: &str,
        ty: Type<'db>,
    ) -> Result<String, Unsupported> {
        let llvm = self.types.value(ty)?;
        let tag = self.emit(&format!(
            "getelementptr inbounds {llvm}, ptr {address}, i32 0, i32 0"
        ));
        Ok(self.emit(&format!("load i8, ptr {tag}")))
    }
}
