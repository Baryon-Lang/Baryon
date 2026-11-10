//! Expressions: each one either names a place, is a constant, or is computed into a place by
//! statements and, for `and`, `or`, conditionals and calls, blocks.

use crate::builder::Builder;
use database::Definition;
use mir::{Constant, ConstantValue, Operand, Place, Projection, Rvalue};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Declaration, Item, Primitive, Type, TypeKind};

impl<'db> Builder<'db> {
    /// Writes the value of the expression `node` into `place`, which has type `to`, converting
    /// it where the types differ: a `T` into a `T | None`, or one number type into another.
    pub(crate) fn write(&mut self, place: Place, to: Type<'db>, node: NodeId) {
        let ty = self.ty(node);
        let kind = self.tree.kind(node);
        if kind == SyntaxKind::Conditional {
            return self.conditional(place, to, node);
        }
        if ty != to {
            let operand = self.operand(node);
            let rvalue = self.coerce(operand, ty, to);
            return self.assign(place, rvalue, node);
        }
        match kind {
            _ if self.is_literal(node) => {
                let operand = self.operand(node);
                self.assign(place, Rvalue::Use(operand), node);
            }
            SyntaxKind::Attribute | SyntaxKind::Subscript if self.is_array_attribute(node) => {
                self.array_attribute(place, node);
            }
            SyntaxKind::Subscript if self.is_list_slice(node) => self.list_slice(&place, node),
            SyntaxKind::Name | SyntaxKind::Attribute if self.is_argv(node) => {
                self.argv(&place, node);
            }
            SyntaxKind::Name | SyntaxKind::Attribute | SyntaxKind::Subscript => {
                let operand = self.operand(node);
                self.assign(place, Rvalue::Use(operand), node);
            }
            SyntaxKind::BinaryOperation | SyntaxKind::UnaryOperation if self.is_array(ty) => {
                self.elementwise(place, node);
            }
            SyntaxKind::BinaryOperation => self.binary(place, node),
            SyntaxKind::UnaryOperation => self.unary(place, node),
            SyntaxKind::Compare => self.compare(&place, node),
            SyntaxKind::BooleanOperation => self.boolean(&place, node),
            SyntaxKind::Call => self.call(place, node),
            SyntaxKind::Tuple => {
                let items: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
                let operands = items.into_iter().map(|item| self.operand(item)).collect();
                self.assign(place, Rvalue::Aggregate(ty, operands), node);
            }
            SyntaxKind::List => self.list_display(place, node),
            SyntaxKind::Dictionary => self.dict_display(&place, node),
            SyntaxKind::FormattedString => self.formatted(place, node),
            SyntaxKind::ListComprehension => self.list_comprehension(&place, node),
            _ => self.unsupported(node, "this expression"),
        }
    }

    /// The value of `node` as an operand: a constant, a copy or move of the place it names, or
    /// a temporary it is computed into. A value that does not copy moves.
    pub(crate) fn operand(&mut self, node: NodeId) -> Operand<'db> {
        if self.is_literal(node) {
            return Operand::Constant(self.literal(node));
        }
        if let Some(constant) = self.named_constant(node) {
            return Operand::Constant(constant);
        }
        let ty = self.ty(node);
        if !self.is_array_attribute(node)
            && !self.is_list_slice(node)
            && let Some(place) = self.place(node)
        {
            if place.is_view() {
                // A view is a borrow: where a value is needed, its elements are copied.
                let copy = self.copy_view(place, ty, node);
                return Operand::Move(Place::from(copy));
            }
            return self.take(place, ty);
        }
        if matches!(
            self.tree.kind(node),
            SyntaxKind::Name | SyntaxKind::Attribute | SyntaxKind::Subscript
        ) && !self.is_array_attribute(node)
            && !self.is_list_slice(node)
            && !self.is_argv(node)
        {
            // A name of something lowering does not handle, already reported by `place`.
            return Operand::Constant(Constant {
                ty,
                value: ConstantValue::None,
            });
        }
        let temporary = self.temporary(ty, node);
        self.write(Place::from(temporary), ty, node);
        self.take(Place::from(temporary), ty)
    }

    /// The value of `node` converted to the number type `to`.
    pub(crate) fn operand_as(&mut self, node: NodeId, to: Type<'db>) -> Operand<'db> {
        let ty = self.ty(node);
        let operand = self.operand(node);
        self.convert(operand, ty, to, node)
    }

    /// `operand`, of type `from`, converted to type `to`, through a temporary when it must be.
    pub(crate) fn convert(
        &mut self,
        operand: Operand<'db>,
        from: Type<'db>,
        to: Type<'db>,
        node: NodeId,
    ) -> Operand<'db> {
        if from == to {
            return operand;
        }
        let rvalue = self.coerce(operand, from, to);
        let temporary = self.temporary(to, node);
        self.assign(Place::from(temporary), rvalue, node);
        self.take(Place::from(temporary), to)
    }

    /// The rvalue that turns `operand`, of type `from`, into a value of type `to`: `None` or a
    /// `T` into a `T | None`, a member into its union (decision 0032), or a number into another
    /// number type.
    pub(crate) fn coerce(
        &self,
        operand: Operand<'db>,
        from: Type<'db>,
        to: Type<'db>,
    ) -> Rvalue<'db> {
        if from == to {
            return Rvalue::Use(operand);
        }
        if let Some(value) = self.optional_value(to) {
            if matches!(from.kind(self.db), TypeKind::None) {
                return Rvalue::Use(Operand::Constant(Constant {
                    ty: to,
                    value: ConstantValue::None,
                }));
            }
            if value == from {
                return Rvalue::Aggregate(to, vec![operand]);
            }
        }
        if self
            .union_members(to)
            .is_some_and(|members| members.contains(&from))
        {
            return Rvalue::Aggregate(to, vec![operand]);
        }
        match (from.kind(self.db), to.kind(self.db)) {
            (TypeKind::Primitive(_), TypeKind::Primitive(_)) => Rvalue::Convert(operand, to),
            _ => Rvalue::Use(operand),
        }
    }

    /// A copy of the value at `place` of type `ty`, or a move when the type does not copy.
    pub(crate) fn take(&self, place: Place, ty: Type<'db>) -> Operand<'db> {
        if self.is_copy(ty) {
            Operand::Copy(place)
        } else {
            Operand::Move(place)
        }
    }

    /// The place `node` names: a variable or parameter, or a field of a place. A read of a
    /// `T | None` variable the checker narrowed to `T` reads the value inside, and a loop
    /// variable that borrows an element reads the element.
    pub(crate) fn place(&mut self, node: NodeId) -> Option<Place> {
        if self.is_argv(node) {
            // `sys.argv` names no place: each read builds a new list.
            return None;
        }
        match self.tree.kind(node) {
            SyntaxKind::Name => {
                if let Some(&Definition::Local(binding)) = self.definition(node)
                    && let Some(element) = self.aliases.get(&binding)
                {
                    return Some(element.clone());
                }
                let Some(local) = self.variable(node) else {
                    self.unsupported(node, "names other than variables, parameters and constants");
                    return None;
                };
                let held = self.local_type(local);
                let read = self.typed.expression(node);
                let place = Place::from(local);
                Some(match read {
                    Some(read) if read != held && self.optional_value(held) == Some(read) => {
                        place.project(Projection::Value)
                    }
                    _ => place,
                })
            }
            SyntaxKind::Attribute => {
                let value = self.tree.child(node, Role::Value)?;
                let fields = self.field(value, node)?;
                // A part of any complex value, such as `(z * w).real`, reads the value's
                // temporary.
                let base = if self.is_complex(self.ty(value)) {
                    self.borrow(value)
                } else if matches!(
                    self.tree.kind(value),
                    SyntaxKind::Name | SyntaxKind::Attribute | SyntaxKind::Subscript
                ) {
                    self.place(value)?
                } else {
                    self.unsupported(node, "fields of a value that is not a variable");
                    return None;
                };
                Some(
                    fields
                        .into_iter()
                        .fold(base, |place, field| place.project(Projection::Field(field))),
                )
            }
            SyntaxKind::Subscript => self.subscript(node),
            _ => None,
        }
    }

    /// A place holding the value of `node`, for a borrow: the place it names, or a temporary
    /// it is computed into, dropped when the statement ends. `a.shape` and `a.size` name no place,
    /// so they are computed too. When `node` names a place this
    /// slice cannot lower, the error is reported once, and the value is not computed again.
    pub(crate) fn borrow(&mut self, node: NodeId) -> Place {
        let reported = self.reported();
        if !self.is_literal(node)
            && self.named_constant(node).is_none()
            && !self.is_array_attribute(node)
            && !self.is_list_slice(node)
            && let Some(place) = self.place(node)
        {
            return place;
        }
        let ty = self.ty(node);
        let temporary = self.temporary(ty, node);
        if self.reported() > reported {
            return Place::from(temporary);
        }
        self.write(Place::from(temporary), ty, node);
        self.drop_later(temporary);
        Place::from(temporary)
    }

    /// The positions of the fields to step through for `attribute` on the struct `value`
    /// holds: the embedded fields the checker found the name through (decision 0031), then the
    /// field itself, so `ion.m` is `ion.particle.m`; or the part `attribute` names of a complex
    /// number. `None` with an error when `value` is neither.
    fn field(&mut self, value: NodeId, attribute: NodeId) -> Option<Vec<u32>> {
        let name = self.tree.child(attribute, Role::Attribute)?;
        if self.is_complex(self.ty(value)) {
            return Self::complex_part(self.text_of(name)).map(|part| vec![part]);
        }
        let TypeKind::Struct(nominal) = self.ty(value).kind(self.db) else {
            self.unsupported(attribute, "attributes other than struct fields");
            return None;
        };
        let mut path = self.typed.promotion(attribute).to_vec();
        let mut class = (nominal.file, nominal.name);
        for &step in &path {
            let fields = self.fields(class)?;
            let TypeKind::Struct(inner) = fields.get(usize::try_from(step).ok()?)?.ty.kind(self.db)
            else {
                return None;
            };
            class = (inner.file, inner.name);
        }
        let wanted = self.text_of(name);
        let position = self
            .fields(class)?
            .iter()
            .position(|field| field.name == wanted)?;
        path.push(u32::try_from(position).ok()?);
        Some(path)
    }

    /// The fields of the struct declared by the class whose `Name` node is `class.1` in the
    /// file `class.0`.
    pub(crate) fn fields(
        &self,
        class: (database::File, NodeId),
    ) -> Option<&'db [types::Field<'db>]> {
        let item = Item::new(self.db, self.package, class.0, class.1);
        match &self.db.declaration(item).declaration {
            Declaration::Struct { fields, .. } => Some(fields),
            _ => None,
        }
    }

    /// The `bool` type.
    pub(crate) fn boolean_type(&self) -> Type<'db> {
        Type::new(self.db, TypeKind::Primitive(Primitive::Bool))
    }
}
