//! `for` loops over collections: a rank-1 `Array` or `Fixed` array, a list, a tuple whose items
//! have one type, and the characters of a `str`. The `loops` module calls these for a `for`
//! statement and the `comprehensions` module for each clause of a comprehension.
//!
//! A loop over values that are not copied, such as `list[str]`, borrows each element, as the
//! reference's values and references chapter says: the loop variable is the element, read in
//! place, never a copy and never dropped. Lowering makes each read of the variable a read of the
//! element's place, which is exact while every use of the variable is inside a loop that binds
//! it; a variable also used outside one is reported as not lowered yet.
//!
//! A loop over a `str` is one of these too: it counts the characters once with
//! [`Builtin::LengthStr`], then each pass takes the next with [`Builtin::CharacterStr`], a
//! one-character `str` that borrows the string's own bytes, so the loop is linear in the length.

use crate::builder::Builder;
use crate::loops::Pass;
use database::Definition;
use mir::{
    Argument, BinaryOperator, BlockId, Builtin, Constant, Local, Operand, Place, Projection,
    Rvalue, TerminatorKind,
};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

/// How a collection loop reaches its elements.
enum Elements {
    /// At an index, in an array or a list.
    Indexed,
    /// Through a switch on the counter, in a tuple of this many items.
    Tuple(usize),
    /// One character at a time from this byte cursor, in a `str`.
    Text(Local),
}

impl<'db> Builder<'db> {
    /// `for target in collection: body`, over a rank-1 `Array` or `Fixed` array, a list, a tuple
    /// whose items have one type, or a `str`. The collection is computed once and borrowed for
    /// the loop, as the reference's values and references chapter says, so the body cannot
    /// change it. Each pass binds `target` to the next element, which it may unpack:
    /// `for x, y in points`. An element of a copy type is copied; one that is not copied is
    /// borrowed (the module documentation).
    ///
    /// An array's or list's element is read at the counter without a bounds check, since the
    /// counter stays below the length, which cannot change while the loop borrows the
    /// collection. A tuple's items have no index to read at, so each pass switches on the
    /// counter to the block that copies its item; LLVM unrolls the short loop. A `str`'s
    /// characters are read from a byte cursor, which each pass moves on.
    pub(crate) fn collection_loop(
        &mut self,
        statement: NodeId,
        target: NodeId,
        iterator: NodeId,
        pass: Pass<'_, 'db>,
    ) {
        let ty = self.ty(iterator);
        let element = match ty.kind(self.db) {
            TypeKind::Array { element, rank: 1 } | TypeKind::List(element) => *element,
            TypeKind::Fixed { element, shape } if shape.rank() == 1 => *element,
            TypeKind::Tuple(items) if !items.is_empty() => items[0],
            TypeKind::Str => ty,
            _ => {
                return self.unsupported(iterator, "`for` loops over values of this type");
            }
        };
        let borrowed = !self.is_copy(element);
        if borrowed && !self.confined(statement, target) {
            return self.unsupported(
                target,
                "a loop variable that borrows an element and is also used outside the loop",
            );
        }
        let collection = self.borrow(iterator);
        let int = self.int_type();
        let (length, items) = match ty.kind(self.db) {
            TypeKind::Tuple(items) => {
                let count = i128::try_from(items.len()).unwrap_or(i128::MAX);
                let count = Operand::Constant(Constant::integer(int, count));
                (count, Elements::Tuple(items.len()))
            }
            TypeKind::Str => {
                // The loop runs once per character and reads each from a byte cursor, which
                // each pass moves on.
                let characters = self.temporary(int, iterator);
                let text = vec![Argument::Borrow(collection.clone())];
                self.text_call(Place::from(characters), Builtin::LengthStr, text, iterator);
                let cursor = self.temporary(int, iterator);
                let start = Rvalue::Use(Operand::Constant(Constant::integer(int, 0)));
                self.assign(Place::from(cursor), start, iterator);
                (
                    Operand::Copy(Place::from(characters)),
                    Elements::Text(cursor),
                )
            }
            _ => (self.length(&collection, ty, 0, iterator), Elements::Indexed),
        };
        let limit = self.temporary(int, iterator);
        self.assign(Place::from(limit), Rvalue::Use(length), iterator);
        let counter = self.temporary(int, iterator);
        self.assign(
            Place::from(counter),
            Rvalue::Use(Operand::Constant(Constant::integer(int, 0))),
            iterator,
        );

        let header = self.new_block();
        self.terminate(TerminatorKind::Goto(header), statement);
        self.start(header);
        let (body, next, end) = (self.new_block(), self.new_block(), self.new_block());
        self.loop_test((BinaryOperator::Less, counter, limit, body, end), iterator);
        self.start(body);
        let aliases = self.aliases.clone();
        self.bind_pass(target, (&collection, items), counter, element);
        pass(self, next, end);
        self.aliases = aliases;
        self.start(next);
        self.assign(
            Place::from(counter),
            Rvalue::Binary(
                BinaryOperator::Add,
                Operand::Copy(Place::from(counter)),
                Operand::Constant(Constant::integer(int, 1)),
            ),
            iterator,
        );
        self.terminate(TerminatorKind::Goto(header), statement);
        self.start(end);
    }

    /// Binds the loop target `target` to the element of `collection` for the pass at `counter`:
    /// the character at the cursor of a `str`, the element at the counter of an array or list,
    /// or the item at the counter of a tuple, whose items have type `element`. An element that
    /// is not copied is borrowed (the module documentation).
    fn bind_pass(
        &mut self,
        target: NodeId,
        (collection, items): (&Place, Elements),
        counter: Local,
        element: Type<'db>,
    ) {
        let borrowed = !self.is_copy(element);
        match items {
            Elements::Text(cursor) => {
                let character = self.temporary(element, target);
                self.text_call(
                    Place::from(character),
                    Builtin::CharacterStr,
                    vec![
                        Argument::Borrow(collection.clone()),
                        Argument::InOut(Place::from(cursor)),
                    ],
                    target,
                );
                self.bind_element(target, Place::from(character), element);
            }
            Elements::Indexed => {
                let place = collection.clone().project(Projection::Index(vec![counter]));
                self.bind_element(target, place, element);
            }
            Elements::Tuple(count) if borrowed => {
                // The item is copied bit for bit into a temporary that is never dropped, so the
                // switch's arms meet at one place the variable can borrow.
                let item = self.temporary(element, target);
                self.tuple_item(
                    target,
                    (collection, count),
                    counter,
                    &mut |builder, place| {
                        builder.assign(
                            Place::from(item),
                            Rvalue::Use(Operand::Copy(place)),
                            target,
                        );
                    },
                );
                self.bind_element(target, Place::from(item), element);
            }
            Elements::Tuple(count) => {
                self.tuple_item(
                    target,
                    (collection, count),
                    counter,
                    &mut |builder, place| {
                        builder.assign_part(target, place, element);
                    },
                );
            }
        }
    }

    /// Runs `write` on item number `counter` of the tuple at `tuple`, which has `count` items,
    /// for the loop target `target`: a switch on the counter goes to one block per item, each
    /// writes its item, and each joins the rest of the loop body.
    fn tuple_item(
        &mut self,
        target: NodeId,
        (tuple, count): (&Place, usize),
        counter: Local,
        write: &mut dyn FnMut(&mut Self, Place),
    ) {
        let blocks: Vec<BlockId> = (0..count).map(|_| self.new_block()).collect();
        let otherwise = self.new_block();
        self.terminate(
            TerminatorKind::Switch {
                value: Operand::Copy(Place::from(counter)),
                cases: (0..).zip(blocks.iter().copied()).collect(),
                otherwise,
            },
            target,
        );
        self.start(otherwise);
        // The counter stays below the count, so no other value reaches the switch.
        self.terminate(TerminatorKind::Unreachable, target);
        let mut join = None;
        for (field, block) in (0u32..).zip(blocks) {
            self.start(block);
            let place = tuple.clone().project(Projection::Field(field));
            write(self, place);
            self.jump(&mut join, target);
        }
        self.join(join);
    }

    /// Binds the loop target `target` to the element at `place`, of type `ty`: copies an element
    /// of a copy type into it, and makes each name that would hold a value that is not copied
    /// borrow its part of the element in place, until the loop ends. A target that unpacks
    /// does this for each part, so `for name, mass in species` borrows `name` and copies `mass`.
    pub(crate) fn bind_element(&mut self, target: NodeId, place: Place, ty: Type<'db>) {
        if self.is_copy(ty) {
            return self.assign_part(target, place, ty);
        }
        match (self.tree.kind(target), ty.kind(self.db)) {
            (SyntaxKind::Name, _) => {
                if let Some(&Definition::Local(binding)) = self.definition(target) {
                    self.aliases.insert(binding, place);
                }
            }
            (SyntaxKind::Tuple, TypeKind::Tuple(items)) => {
                let parts: Vec<NodeId> = self.tree.children_with(target, Role::Item).collect();
                for ((field, part), &item) in (0u32..).zip(parts).zip(items) {
                    let at = place.clone().project(Projection::Field(field));
                    self.bind_element(part, at, item);
                }
            }
            _ => self.unsupported(target, "this loop target"),
        }
    }

    /// Whether every use of the variables `target` binds lies inside a `for` statement whose
    /// target binds it again, `statement` or another, so reading each one as the element it
    /// borrows is exact: such a loop gives the variable its value at the top of every pass,
    /// before any read in it. Two loops that reuse a name, `for c in a:` then `for c in b:`,
    /// are confined; a read after the loop, or an assignment outside one, is not. A
    /// comprehension's variables belong to the comprehension alone, so they always are.
    pub(crate) fn confined(&self, statement: NodeId, target: NodeId) -> bool {
        if self.tree.kind(statement) != SyntaxKind::For {
            return true;
        }
        let (mut everywhere, mut names) = (Vec::new(), Vec::new());
        self.descendants(self.function, &mut everywhere);
        self.descendants(target, &mut names);
        let refers = |node: NodeId, binding: NodeId| {
            self.definition(node) == Some(&Definition::Local(binding))
        };
        names.into_iter().all(|name| {
            let Some(&Definition::Local(binding)) = self.definition(name) else {
                return true;
            };
            // The spans of the `for` statements whose targets bind the variable.
            let rebinding: Vec<_> = everywhere
                .iter()
                .filter(|&&node| self.tree.kind(node) == SyntaxKind::For)
                .filter(|&&node| {
                    let mut bound = Vec::new();
                    if let Some(target) = self.tree.child(node, Role::Target) {
                        self.descendants(target, &mut bound);
                    }
                    bound.into_iter().any(|bound| refers(bound, binding))
                })
                .map(|&node| self.tree.span(node))
                .collect();
            everywhere
                .iter()
                .filter(|&&node| refers(node, binding))
                .all(|&node| {
                    let span = self.tree.span(node);
                    rebinding.iter().any(|loop_span| {
                        loop_span.start() <= span.start() && span.end() <= loop_span.end()
                    })
                })
        })
    }
}
