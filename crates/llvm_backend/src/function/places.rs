//! Places and operands inside one function: the address of a place, the type of a struct's
//! field, and the LLVM value of an operand, loading it when it lives in memory.

use super::{Function, Passing};

/// Where an assigned value goes: a promoted local's register, or memory.
pub(crate) enum Destination {
    /// The promoted local with this index, which takes the value as a register.
    Local(usize),
    /// The address to store the value at.
    Address(String),
}
use crate::Unsupported;
use crate::literal::{float, wrap};
use crate::ty::Repr;
use mir::{ConstantValue, Operand, Place, Projection};
use types::{Type, TypeKind};

impl<'db> Function<'_, '_, 'db> {
    /// The address of `place` and the type of the value there.
    pub(crate) fn place(&mut self, place: &Place) -> Result<(String, Type<'db>), Unsupported> {
        let local = place.local.index();
        if self.ssa.is_promoted(local) {
            // Its address is needed after all: the function is written again with it in a
            // stack slot (the `ssa` module).
            self.ssa.escaped.insert(local);
        }
        let mut ty = self.body.locals[local].ty;
        let mut address = match self.passing[local] {
            Passing::Slot => format!("%_{local}"),
            Passing::Pointer => format!("%a{local}"),
        };
        for projection in &place.projections {
            let position = match projection {
                Projection::Field(position) => position,
                Projection::Index(indices) => {
                    (address, ty) = self.element(&address, ty, indices)?;
                    continue;
                }
                Projection::Error(position) => {
                    (address, ty) = self.error_payload(&address, ty, *position)?;
                    continue;
                }
                Projection::Slice(dimensions) => {
                    (address, ty) = self.view(&address, ty, dimensions)?;
                    continue;
                }
                Projection::Member(position) => {
                    (address, ty) = self.member_payload(&address, ty, *position)?;
                    continue;
                }
                Projection::Value => {
                    let TypeKind::Optional(value) = ty.kind(self.types.db) else {
                        return Err(self.types.unsupported(ty));
                    };
                    let container = self.types.value(ty)?;
                    address = self.emit(&format!(
                        "getelementptr inbounds {container}, ptr {address}, i32 0, i32 0"
                    ));
                    ty = *value;
                    continue;
                }
            };
            let container = self.types.value(ty)?;
            ty = self.field(ty, *position)?;
            let field = self.fresh();
            self.line(&format!(
                "{field} = getelementptr inbounds {container}, ptr {address}, i32 0, i32 {position}"
            ));
            address = field;
        }
        Ok((address, ty))
    }

    /// Where to write a value assigned to `place`, and its type: a promoted local, which takes
    /// the value as a register, or an address to store it at (the `ssa` module).
    pub(crate) fn destination(
        &mut self,
        place: &Place,
    ) -> Result<(Destination, Type<'db>), Unsupported> {
        let local = place.local.index();
        if place.projections.is_empty() && self.ssa.is_promoted(local) {
            return Ok((Destination::Local(local), self.body.locals[local].ty));
        }
        let (address, ty) = self.place(place)?;
        Ok((Destination::Address(address), ty))
    }

    /// Writes `value`, of LLVM type `llvm`, to `destination`.
    pub(crate) fn store(&mut self, destination: &Destination, llvm: &str, value: &str) {
        match destination {
            Destination::Local(local) => self.ssa.assign(*local, value.to_owned()),
            Destination::Address(address) => {
                self.line(&format!("store {llvm} {value}, ptr {address}"));
            }
        }
    }

    /// The value held at `place` and its type, or `None` for a `None` value: read from a
    /// promoted local's register, or loaded from the place's address.
    pub(crate) fn place_value(
        &mut self,
        place: &Place,
    ) -> Result<Option<(String, Type<'db>)>, Unsupported> {
        let local = place.local.index();
        if place.projections.is_empty() && self.ssa.is_promoted(local) {
            return Ok(Some((self.ssa.read(local), self.body.locals[local].ty)));
        }
        let (address, ty) = self.place(place)?;
        Ok(self.load(&address, ty)?.map(|value| (value, ty)))
    }

    /// The type of item or field `position` of the tuple or struct type `ty`, or of part
    /// `position` of a complex number: 0 the real part, 1 the imaginary part.
    pub(crate) fn field(&self, ty: Type<'db>, position: u32) -> Result<Type<'db>, Unsupported> {
        let items = match ty.kind(self.types.db) {
            TypeKind::Primitive(types::Primitive::Complex(complex)) => {
                let part = Type::new(
                    self.types.db,
                    TypeKind::Primitive(types::Primitive::Float(complex.part())),
                );
                vec![part, part]
            }
            TypeKind::Tuple(items) => items.clone(),
            TypeKind::Struct(nominal) => match (self.types.declared)(nominal) {
                types::Declared::Struct(fields) => fields,
                types::Declared::Enum { .. } => Vec::new(),
            },
            _ => Vec::new(),
        };
        items
            .get(position as usize)
            .copied()
            .ok_or_else(|| self.types.unsupported(ty))
    }

    /// The value of `operand` and its type, or `None` for a `None` value, which has no value.
    pub(crate) fn operand(
        &mut self,
        operand: &Operand<'db>,
    ) -> Result<Option<(String, Type<'db>)>, Unsupported> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => self.place_value(place),
            Operand::Constant(constant) => {
                let value = match &constant.value {
                    // An absent `T | None` is all zeros, its flag false; `None` itself has no value.
                    ConstantValue::None
                        if matches!(constant.ty.kind(self.types.db), TypeKind::Optional(_)) =>
                    {
                        "zeroinitializer".to_owned()
                    }
                    ConstantValue::None => return Ok(None),
                    ConstantValue::Bool(value) => value.to_string(),
                    ConstantValue::Integer(value) => {
                        let bits = self.integer_bits(constant.ty)?;
                        wrap(*value, bits).to_string()
                    }
                    ConstantValue::Float(bits) => float(
                        f64::from_bits(*bits),
                        self.types.value(constant.ty)? == "float",
                    ),
                    ConstantValue::Str(text) => self.text_constant(text),
                    ConstantValue::Complex { real, imaginary } => {
                        let part = self
                            .complex_part(constant.ty)
                            .ok_or_else(|| self.types.unsupported(constant.ty))?;
                        Self::complex_constant(
                            part,
                            f64::from_bits(*real),
                            f64::from_bits(*imaginary),
                        )
                    }
                };
                Ok(Some((value, constant.ty)))
            }
        }
    }

    /// The value of `operand`, which has a type other than `None`.
    pub(crate) fn value(
        &mut self,
        operand: &Operand<'db>,
    ) -> Result<(String, Type<'db>), Unsupported> {
        self.operand(operand)?
            .ok_or_else(|| Unsupported::new("`None` where a value is needed"))
    }

    /// Loads the value of type `ty` at `address`; `None` for a `None` value.
    pub(crate) fn load(
        &mut self,
        address: &str,
        ty: Type<'db>,
    ) -> Result<Option<String>, Unsupported> {
        let Repr::Value(llvm) = self.types.repr(ty)? else {
            return Ok(None);
        };
        let value = self.fresh();
        self.line(&format!("{value} = load {llvm}, ptr {address}"));
        Ok(Some(value))
    }

    /// The width of the integer, `bool` or enum type `ty`.
    pub(super) fn integer_bits(&self, ty: Type<'db>) -> Result<u32, Unsupported> {
        match self.types.number(ty) {
            Some(crate::ty::Number::Integer { bits, .. }) => Ok(bits),
            Some(crate::ty::Number::Bool) => Ok(1),
            _ => Err(self.types.unsupported(ty)),
        }
    }
}
