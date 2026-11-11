//! `os.environ.copy()` (decision 0036, part 5): the environment as a new `dict[str, str]`.

use super::entry;
use crate::builder::Builder;
use mir::{Argument, BinaryOperator, Builtin, Constant, Operand, Place, Rvalue};
use syntax::NodeId;
use types::{Type, TypeKind};

impl Builder<'_> {
    /// `os.environ.copy()` into `place`: the empty `dict`, then every variable added in the
    /// operating system's order. [`Builtin::EnvironmentEntries`] gives a list of each name and
    /// value, last variable first, so popping twice gives the next variable's value and then
    /// its name; each name appears once, so each is a new key. Cost: one copy of every name
    /// and value, and the list that carries them, freed at the end of the statement.
    pub(crate) fn environ_copy(&mut self, place: &Place, node: NodeId) {
        let ty = self.ty(node);
        let str = Type::new(self.db, TypeKind::Str);
        let int = self.int_type();
        let list = Type::new(self.db, TypeKind::List(str));
        let entries = self.temporary(list, node);
        self.drop_later(entries);
        self.builtin_call(
            Builtin::EnvironmentEntries,
            Vec::new(),
            entries.into(),
            node,
        );
        self.assign(place.clone(), Rvalue::Aggregate(ty, Vec::new()), node);
        let length = self.length(&Place::from(entries), list, 0, node);
        let variables = self.temporary(int, node);
        let two = Operand::Constant(Constant::integer(int, 2));
        let halved = Rvalue::Binary(BinaryOperator::FloorDivide, length, two);
        self.assign(Place::from(variables), halved, node);
        let lengths = [Operand::Copy(Place::from(variables))];
        self.loop_nest(&lengths, &mut Vec::new(), node, &mut |builder, _| {
            let value = builder.temporary(str, node);
            let name = builder.temporary(str, node);
            let position = builder.temporary(int, node);
            for text in [value, name] {
                let popped = vec![Argument::InOut(Place::from(entries))];
                builder.builtin_call(Builtin::Pop, popped, text.into(), node);
            }
            let inserted = vec![
                Argument::InOut(place.clone()),
                Argument::Owned(Operand::Move(Place::from(name))),
            ];
            builder.builtin_call(Builtin::DictInsert, inserted, position.into(), node);
            let moved = Rvalue::Use(Operand::Move(Place::from(value)));
            builder.assign(entry(place.clone(), position, 1), moved, node);
        });
    }
}
