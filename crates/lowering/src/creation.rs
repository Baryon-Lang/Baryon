//! Creating arrays (language reference, "Creating arrays"): `Array.zeros`, `Array.ones` and
//! `Array.fill` allocate an `Array` and set each element in a loop nest; the same functions on a
//! `Fixed` type, such as `Vec3.zeros()`, build the value from its elements.

use crate::builder::Builder;
use mir::{BinaryOperator, Constant, Local, Operand, Place, Projection, Rvalue, TerminatorKind};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

impl<'db> Builder<'db> {
    /// `Array.zeros(shape)`, `Array.ones(shape)` and `Array.fill(shape, value)` into `place`:
    /// a new array of the call's type, then a loop nest that sets every element.
    pub(crate) fn new_array(
        &mut self,
        place: &Place,
        function: &str,
        arguments: &[NodeId],
        node: NodeId,
    ) {
        let ty = self.ty(node);
        let element = self.element(ty);
        let Some(&shape) = arguments.first() else {
            return;
        };
        let entries: Vec<NodeId> = if self.tree.kind(shape) == SyntaxKind::Tuple {
            self.tree.children_with(shape, Role::Item).collect()
        } else {
            vec![shape]
        };
        let int = self.int_type();
        let lengths: Vec<Operand<'db>> = entries
            .into_iter()
            .map(|entry| {
                let operand = self.operand_as(entry, int);
                let local = self.temporary(int, entry);
                self.assign(Place::from(local), Rvalue::Use(operand), entry);
                Operand::Copy(Place::from(local))
            })
            .collect();
        let Some(value) = self.fill_value(function, element, arguments.get(1).copied(), node)
        else {
            return;
        };
        self.assign(place.clone(), Rvalue::NewArray(ty, lengths.clone()), node);
        self.loop_nest(&lengths, &mut Vec::new(), node, &mut |builder, indices| {
            let element = place.clone().project(Projection::Index(indices.to_vec()));
            builder.assign(element, Rvalue::Use(value.clone()), node);
        });
    }

    /// `Vec3.zeros()`, `Vec3.ones()` and `Vec3.fill(value)` into `place`: a `Fixed` array
    /// built from its elements.
    pub(crate) fn new_fixed(
        &mut self,
        place: Place,
        function: &str,
        arguments: &[NodeId],
        node: NodeId,
    ) {
        let ty = self.ty(node);
        let TypeKind::Fixed { element, shape } = ty.kind(self.db) else {
            return self.unsupported(node, "this call");
        };
        let (element, count) = (*element, shape.elements());
        let Some(value) = self.fill_value(function, element, arguments.first().copied(), node)
        else {
            return;
        };
        let Ok(count) = usize::try_from(count) else {
            return;
        };
        self.assign(place, Rvalue::Aggregate(ty, vec![value; count]), node);
    }

    /// The value every element of a new array takes: 0 for `zeros`, 1 for `ones`, the argument
    /// `value` for `fill`, as the element type.
    fn fill_value(
        &mut self,
        function: &str,
        element: Type<'db>,
        value: Option<NodeId>,
        node: NodeId,
    ) -> Option<Operand<'db>> {
        let number = match function {
            "zeros" => 0,
            "ones" => 1,
            "fill" => return Some(self.operand_as(value?, element)),
            _ => {
                self.unsupported(node, &format!("`{function}`"));
                return None;
            }
        };
        // An array of `Fixed` elements, such as `Array.zeros(n, dtype=Vec3)`, fills each with
        // a `Fixed` value built from the number.
        let &TypeKind::Fixed {
            element: inner,
            ref shape,
        } = element.kind(self.db)
        else {
            return Some(Operand::Constant(self.number_constant(element, number)));
        };
        let count = usize::try_from(shape.elements()).ok()?;
        let constant = Operand::Constant(self.number_constant(inner, number));
        let local = self.temporary(element, node);
        self.assign(
            Place::from(local),
            Rvalue::Aggregate(element, vec![constant; count]),
            node,
        );
        Some(Operand::Copy(Place::from(local)))
    }

    /// Runs `each` once for every index into an array with `lengths`, in row-major order, one
    /// loop per dimension with the last innermost, so memory is visited in order (decision
    /// 0008). `each` gets one index local per dimension. `counters` holds the indices of the
    /// loops already open.
    pub(crate) fn loop_nest(
        &mut self,
        lengths: &[Operand<'db>],
        counters: &mut Vec<Local>,
        node: NodeId,
        each: &mut dyn FnMut(&mut Self, &[Local]),
    ) {
        let Some(length) = lengths.get(counters.len()) else {
            return each(self, counters);
        };
        let int = self.int_type();
        let counter = self.temporary(int, node);
        self.assign(
            Place::from(counter),
            Rvalue::Use(Operand::Constant(Constant::integer(int, 0))),
            node,
        );
        let header = self.new_block();
        self.terminate(TerminatorKind::Goto(header), node);
        self.start(header);
        let condition = self.temporary(self.boolean_type(), node);
        self.assign(
            Place::from(condition),
            Rvalue::Binary(
                BinaryOperator::Less,
                Operand::Copy(Place::from(counter)),
                length.clone(),
            ),
            node,
        );
        let (body, end) = (self.new_block(), self.new_block());
        self.terminate(
            TerminatorKind::Branch {
                condition: Operand::Copy(Place::from(condition)),
                then: body,
                otherwise: end,
            },
            node,
        );
        self.start(body);
        counters.push(counter);
        self.loop_nest(lengths, counters, node, each);
        counters.pop();
        self.assign(
            Place::from(counter),
            Rvalue::Binary(
                BinaryOperator::Add,
                Operand::Copy(Place::from(counter)),
                Operand::Constant(Constant::integer(int, 1)),
            ),
            node,
        );
        self.terminate(TerminatorKind::Goto(header), node);
        self.start(end);
    }
}
