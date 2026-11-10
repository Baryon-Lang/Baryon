//! Unions of structs (decision 0032): building one from a member, reading which member it
//! holds, and reading that member.
//!
//! A union's layout is the one [`types::layout`] gives: a tag, the smallest unsigned integer
//! that counts the members, then room for the largest member at the most aligned member's
//! alignment `A`. In LLVM it is `{ iT, [n x iA] }`, the array covering the payload, so the
//! payload starts at offset `A` exactly as the layout says. A member is stored into and read
//! from the payload through its address, so the payload's own LLVM type never needs to be the
//! member's.

use crate::Unsupported;
use crate::function::Function;
use crate::print::Form;
use crate::ty::{Repr, Types};
use mir::{Operand, Place};
use types::{Type, TypeKind};

impl<'db> Types<'_, 'db> {
    /// The LLVM type of the union `ty`, `{ iT, [n x iA] }`, as the module describes.
    pub(crate) fn union(&self, ty: Type<'db>) -> Result<String, Unsupported> {
        let layout = types::layout(self.db, ty, self.declared).map_err(|_| self.unsupported(ty))?;
        let align = layout.align();
        let count = (layout.size() - align) / align;
        Ok(format!(
            "{{ i{}, [{count} x i{}] }}",
            self.tag_bits(ty)?,
            align * 8
        ))
    }

    /// The width in bits of the tag of the union `ty`: the smallest unsigned integer that
    /// counts its members, as `types::layout` sizes it.
    fn tag_bits(&self, ty: Type<'db>) -> Result<u32, Unsupported> {
        let TypeKind::Union(members) = ty.kind(self.db) else {
            return Err(self.unsupported(ty));
        };
        Ok(match members.len() {
            0..=0x100 => 8,
            0x101..=0x1_0000 => 16,
            _ => 32,
        })
    }
}

impl<'db> Function<'_, '_, 'db> {
    /// Writes the union of type `ty` that holds the value of `operand`, one of its members, to
    /// `address`: the member's position as the tag, then the member in the payload.
    pub(crate) fn store_union(
        &mut self,
        address: &str,
        ty: Type<'db>,
        operand: &Operand<'db>,
    ) -> Result<(), Unsupported> {
        let TypeKind::Union(members) = ty.kind(self.types.db) else {
            return Err(self.types.unsupported(ty));
        };
        // `None`, the one member with no value, loads as nothing.
        let loaded = self.operand(operand)?;
        let member = loaded
            .as_ref()
            .map_or_else(|| Type::new(self.types.db, TypeKind::None), |&(_, ty)| ty);
        let position = members
            .iter()
            .position(|&candidate| candidate == member)
            .ok_or_else(|| self.types.unsupported(ty))?;
        let llvm = self.types.union(ty)?;
        let bits = self.types.tag_bits(ty)?;
        let tag = self.emit(&format!(
            "getelementptr inbounds {llvm}, ptr {address}, i32 0, i32 0"
        ));
        self.line(&format!("store i{bits} {position}, ptr {tag}"));
        if let Some((value, _)) = loaded
            && let Repr::Value(value_type) = self.types.repr(member)?
        {
            let payload = self.emit(&format!(
                "getelementptr inbounds {llvm}, ptr {address}, i32 0, i32 1"
            ));
            self.line(&format!("store {value_type} {value}, ptr {payload}"));
        }
        Ok(())
    }

    /// Which member the union at `place` holds, as an `i64`: its tag, widened.
    pub(crate) fn member_index(&mut self, place: &Place) -> Result<String, Unsupported> {
        let (address, ty) = self.place(place)?;
        let llvm = self.types.union(ty)?;
        let bits = self.types.tag_bits(ty)?;
        let tag = self.emit(&format!(
            "getelementptr inbounds {llvm}, ptr {address}, i32 0, i32 0"
        ));
        let tag = self.emit(&format!("load i{bits}, ptr {tag}"));
        Ok(self.emit(&format!("zext i{bits} {tag} to i64")))
    }

    /// Frees what the union of type `ty` at `address` owns: a switch on its tag, and for each
    /// member that owns memory, a drop of the payload as that member.
    pub(crate) fn drop_union(&mut self, address: &str, ty: Type<'db>) -> Result<(), Unsupported> {
        let TypeKind::Union(members) = ty.kind(self.types.db) else {
            return Err(self.types.unsupported(ty));
        };
        let llvm = self.types.union(ty)?;
        let bits = self.types.tag_bits(ty)?;
        let tag = self.emit(&format!(
            "getelementptr inbounds {llvm}, ptr {address}, i32 0, i32 0"
        ));
        let tag = self.emit(&format!("load i{bits}, ptr {tag}"));
        let done = self.fresh_block();
        let mut owning = Vec::new();
        for (position, &member) in members.iter().enumerate() {
            if self.owns_memory(member) {
                owning.push((position, member, self.fresh_block()));
            }
        }
        let cases: Vec<String> = owning
            .iter()
            .map(|(position, _, block)| format!("i{bits} {position}, label %{block}"))
            .collect();
        self.line(&format!(
            "switch i{bits} {tag}, label %{done} [ {} ]",
            cases.join(" ")
        ));
        for (_, member, block) in owning {
            self.line_label(&block);
            let payload = self.emit(&format!(
                "getelementptr inbounds {llvm}, ptr {address}, i32 0, i32 1"
            ));
            self.drop_value(&payload, member)?;
            self.line(&format!("br label %{done}"));
        }
        self.line_label(&done);
        Ok(())
    }

    /// Prints the union `value`, of type `ty`, as the member it holds, in `form`: `Dirichlet(value=300.0)`,
    /// or `None`. The value goes to a stack slot so each member can be read from the payload.
    pub(crate) fn print_union(
        &mut self,
        stream: u8,
        value: &str,
        ty: Type<'db>,
        form: Form,
    ) -> Result<(), Unsupported> {
        let TypeKind::Union(members) = ty.kind(self.types.db) else {
            return Err(self.types.unsupported(ty));
        };
        let llvm = self.types.union(ty)?;
        let bits = self.types.tag_bits(ty)?;
        let slot = self.slot(&llvm);
        self.line(&format!("store {llvm} {value}, ptr {slot}"));
        let tag = self.emit(&format!("extractvalue {llvm} {value}, 0"));
        let done = self.fresh_block();
        let blocks: Vec<String> = members.iter().map(|_| self.fresh_block()).collect();
        let cases: Vec<String> = blocks
            .iter()
            .enumerate()
            .map(|(position, block)| format!("i{bits} {position}, label %{block}"))
            .collect();
        // Every tag names a member, so the default is never taken.
        self.line(&format!(
            "switch i{bits} {tag}, label %{} [ {} ]",
            blocks[0],
            cases.join(" ")
        ));
        for (&member, block) in members.iter().zip(blocks) {
            self.line_label(&block);
            let payload = self.emit(&format!(
                "getelementptr inbounds {llvm}, ptr {slot}, i32 0, i32 1"
            ));
            match self.load(&payload, member)? {
                Some(held) => self.print_value(stream, &held, member, form)?,
                None => self.print_text(stream, b"None"),
            }
            self.line(&format!("br label %{done}"));
        }
        self.line_label(&done);
        Ok(())
    }

    /// The address of the member at `position` of the union of type `ty` at `address`, and
    /// that member's type.
    pub(crate) fn member_payload(
        &mut self,
        address: &str,
        ty: Type<'db>,
        position: u32,
    ) -> Result<(String, Type<'db>), Unsupported> {
        let TypeKind::Union(members) = ty.kind(self.types.db) else {
            return Err(self.types.unsupported(ty));
        };
        let member = *members
            .get(position as usize)
            .ok_or_else(|| self.types.unsupported(ty))?;
        let llvm = self.types.union(ty)?;
        let payload = self.emit(&format!(
            "getelementptr inbounds {llvm}, ptr {address}, i32 0, i32 1"
        ));
        Ok((payload, member))
    }
}
