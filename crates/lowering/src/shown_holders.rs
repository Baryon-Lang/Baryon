//! A tuple, a list, a dict or a `T | None` that holds an array with missing values or a
//! frame, as text (decision 0045, roadmap 3.24.4 and 3.24.5): what `print((bids, 1))`,
//! `f"{[bids, asks]}"`, `print({"bids": bids})` and `print((df, 1))` write.
//!
//! The backend writes a tuple or a list in one step when it knows every value in it. An
//! array with missing values is stored as its values and a validity bitmap, which the
//! backend would write as a tuple of two arrays, so its text comes from the standard
//! library's `optional_shown`. A frame is stored as a tuple of its columns, and its text is
//! the table the library's `frame_text` makes. This module therefore writes such a holder
//! piece by piece, as Python writes one: the brackets, a comma and a space between two
//! items, and each item's text. An item with no such array in it is written by the backend as it
//! stands; only a `str` needs help, since inside a tuple it is written in quotes, which the
//! library's `quoted` does (`<std>/frame_holders.byn`).
//!
//! A tuple's items are known when compiling, so a tuple is one `Format`. A list is a loop
//! that gathers the text of each item into a `list[str]`, which the library's `listed`
//! joins; a dict is the same loop over its entries, each written `key: value`, joined by
//! `braced`. A `T | None` is a test: the text `None`, or the text of the value inside.
//! Cost: one text for each array or frame, and for a list or a dict one more for each item.

use crate::builder::Builder;
use mir::{
    Argument, BinaryOperator, Builtin, Callee, Constant, Local, Operand, Piece, Place, Projection,
};
use mir::{Rvalue, StatementKind, TerminatorKind};
use syntax::NodeId;
use types::{FormatSpec, Type, TypeDb, TypeKind};

/// Whether `ty` is an array with missing values or a frame, or a tuple, a list, a dict or
/// a `T | None` that holds one at any depth: the values whose text this module makes.
fn holds_gaps<'db>(db: &'db dyn TypeDb, ty: Type<'db>) -> bool {
    match ty.kind(db) {
        TypeKind::OptionalArray(_) | TypeKind::DataFrame { .. } => true,
        TypeKind::Tuple(items) => items.iter().any(|&item| holds_gaps(db, item)),
        &TypeKind::List(item) | &TypeKind::Optional(item) => holds_gaps(db, item),
        &TypeKind::Dict { value, .. } => holds_gaps(db, value),
        _ => false,
    }
}

impl<'db> Builder<'db> {
    /// The text of the tuple, list, dict or `T | None` of type `ty` borrowed at `place`, when
    /// it holds an array with missing values or a frame: a new `str`, dropped with the
    /// statement's temporaries. `None` for any other value, and after a report.
    pub(crate) fn holder_text(
        &mut self,
        place: &Place,
        ty: Type<'db>,
        node: NodeId,
    ) -> Option<Place> {
        let holder = matches!(
            ty.kind(self.db),
            TypeKind::Tuple(_) | TypeKind::List(_) | TypeKind::Dict { .. } | TypeKind::Optional(_)
        );
        if !holder || !holds_gaps(self.db, ty) {
            return None;
        }
        let text = self.held_text(place, ty, node)?;
        self.drop_later(text);
        Some(Place::from(text))
    }

    /// The text of the value of type `ty` stored at `place`, which is or holds an array
    /// with missing values or a frame: a new `str` the caller owns, and drops or moves. It
    /// is not left to the statement's end, since a list makes one in every turn of its
    /// loop. `None` after a report.
    fn held_text(&mut self, place: &Place, ty: Type<'db>, node: NodeId) -> Option<Local> {
        let text = Type::new(self.db, TypeKind::Str);
        match ty.kind(self.db) {
            &TypeKind::OptionalArray(element) => self.optional_array_text(place, element, node),
            TypeKind::Tuple(items) => {
                let mut pieces = vec![Piece::Text("(".to_owned())];
                let mut made = Vec::new();
                for (position, &item) in (0_u32..).zip(items) {
                    if position > 0 {
                        pieces.push(Piece::Text(", ".to_owned()));
                    }
                    let stored = place.clone().project(Projection::Field(position));
                    let (shown, owned) = self.item_text(stored, item, node)?;
                    pieces.push(Piece::Value(shown, FormatSpec::default()));
                    made.extend(owned);
                }
                // Python writes a tuple of one item with a comma after it.
                let end = if items.len() == 1 { ",)" } else { ")" };
                pieces.push(Piece::Text(end.to_owned()));
                let result = self.temporary(text, node);
                self.assign(Place::from(result), Rvalue::Format(pieces), node);
                for local in made.into_iter().rev() {
                    self.push(StatementKind::Drop(Place::from(local)), node);
                }
                Some(result)
            }
            &TypeKind::List(item) => self.list_text(place, (ty, item), node),
            &TypeKind::Dict { key, value } => self.dict_text(place, (key, value), node),
            &TypeKind::Optional(inner) => self.optional_text(place, inner, node),
            TypeKind::DataFrame { columns, .. } => {
                let storage = ty.frame_storage(self.db).unwrap_or(ty);
                let (table, made) = self.table_text(columns, storage, place, node)?;
                for local in made.into_iter().rev() {
                    self.push(StatementKind::Drop(Place::from(local)), node);
                }
                Some(table)
            }
            _ => {
                self.unsupported(node, "showing this value");
                None
            }
        }
    }

    /// The text of the list of type `list.0` stored at `place`, whose items, of type
    /// `list.1`, each are or hold an array with missing values: the text of each item,
    /// gathered in a `list[str]` and joined by the library's `listed`. A new `str` the
    /// caller owns. `None` after a report.
    fn list_text(
        &mut self,
        place: &Place,
        (ty, item): (Type<'db>, Type<'db>),
        node: NodeId,
    ) -> Option<Local> {
        let text = Type::new(self.db, TypeKind::Str);
        let list = Type::new(self.db, TypeKind::List(text));
        let texts = self.temporary(list, node);
        self.assign(
            Place::from(texts),
            Rvalue::Aggregate(list, Vec::new()),
            node,
        );
        let length = self.length(place, ty, 0, node);
        let mut complete = true;
        self.loop_nest(&[length], &mut Vec::new(), node, &mut |builder, indices| {
            let stored = place.clone().project(Projection::Index(indices.to_vec()));
            let Some(shown) = builder.held_text(&stored, item, node) else {
                complete = false;
                return;
            };
            builder.gathered(texts, shown, node);
        });
        let arguments = vec![Place::from(texts)];
        let result = self.frame_call("listed", Vec::new(), arguments, text, node);
        self.push(StatementKind::Drop(Place::from(texts)), node);
        result.filter(|_| complete)
    }

    /// The text of the `T | None` stored at `place`, whose `T` is `inner` and is or holds an
    /// array with missing values or a frame: `None`, or the text of the value inside. A new
    /// `str` the caller owns. `None` after a report.
    fn optional_text(&mut self, place: &Place, inner: Type<'db>, node: NodeId) -> Option<Local> {
        let text = Type::new(self.db, TypeKind::Str);
        let result = self.temporary(text, node);
        let present = self.temporary(self.boolean_type(), node);
        self.assign(Place::from(present), Rvalue::IsPresent(place.clone()), node);
        let (there, gone, end) = (self.new_block(), self.new_block(), self.new_block());
        self.branch(Operand::Copy(Place::from(present)), (there, gone), node);
        self.start(there);
        let value = place.clone().project(Projection::Value);
        let shown = self.held_text(&value, inner, node)?;
        let moved = Rvalue::Use(Operand::Move(Place::from(shown)));
        self.assign(Place::from(result), moved, node);
        self.terminate(TerminatorKind::Goto(end), node);
        self.start(gone);
        let missing = Rvalue::Format(vec![Piece::Text("None".to_owned())]);
        self.assign(Place::from(result), missing, node);
        self.terminate(TerminatorKind::Goto(end), node);
        self.start(end);
        Some(result)
    }

    /// The text of the dict stored at `place`, whose values, of type `types.1`, each are or
    /// hold an array with missing values or a frame: `key: value` for each entry in
    /// insertion order, gathered in a `list[str]` and joined by the library's `braced`. The
    /// entries are stepped through as a `for` loop over the dict steps through them. A new
    /// `str` the caller owns. `None` after a report.
    fn dict_text(
        &mut self,
        place: &Place,
        (key, value): (Type<'db>, Type<'db>),
        node: NodeId,
    ) -> Option<Local> {
        let text = Type::new(self.db, TypeKind::Str);
        let list = Type::new(self.db, TypeKind::List(text));
        let int = self.int_type();
        let texts = self.temporary(list, node);
        let empty = Rvalue::Aggregate(list, Vec::new());
        self.assign(Place::from(texts), empty, node);
        let position = self.temporary(int, node);
        let zero = Operand::Constant(Constant::integer(int, 0));
        self.next_entry(place, position, zero, node);
        let (header, body, end) = (self.new_block(), self.new_block(), self.new_block());
        self.terminate(TerminatorKind::Goto(header), node);
        self.start(header);
        let more = self.at_least_zero(position, node);
        self.branch(more, (body, end), node);
        self.start(body);
        let entry = place.clone().project(Projection::Index(vec![position]));
        let mut pieces = Vec::new();
        let mut made = Vec::new();
        for (field, ty) in [(0, key), (1, value)] {
            let stored = entry.clone().project(Projection::Field(field));
            let (shown, owned) = self.item_text(stored, ty, node)?;
            pieces.push(Piece::Value(shown, FormatSpec::default()));
            made.extend(owned);
        }
        pieces.insert(1, Piece::Text(": ".to_owned()));
        let shown = self.temporary(text, node);
        self.assign(Place::from(shown), Rvalue::Format(pieces), node);
        for local in made.into_iter().rev() {
            self.push(StatementKind::Drop(Place::from(local)), node);
        }
        self.gathered(texts, shown, node);
        let from = self.temporary(int, node);
        let one = Operand::Constant(Constant::integer(int, 1));
        let after = Rvalue::Binary(
            BinaryOperator::Add,
            Operand::Copy(Place::from(position)),
            one,
        );
        self.assign(Place::from(from), after, node);
        self.next_entry(place, position, Operand::Copy(Place::from(from)), node);
        self.terminate(TerminatorKind::Goto(header), node);
        self.start(end);
        let arguments = vec![Place::from(texts)];
        let result = self.frame_call("braced", Vec::new(), arguments, text, node);
        self.push(StatementKind::Drop(Place::from(texts)), node);
        result
    }

    /// Adds the text `shown`, which moves, to the end of the `list[str]` `texts`.
    fn gathered(&mut self, texts: Local, shown: Local, node: NodeId) {
        let nothing = self.temporary(Type::new(self.db, TypeKind::None), node);
        self.then(node, |next| TerminatorKind::Call {
            callee: Callee::Builtin(Builtin::Append),
            arguments: vec![
                Argument::InOut(Place::from(texts)),
                Argument::Owned(Operand::Move(Place::from(shown))),
            ],
            destination: Place::from(nothing),
            target: next,
            error: None,
        });
    }

    /// What a tuple's `Format` reads for its item of type `ty` stored at `place`: the place
    /// of the item's text, and the local to drop afterwards when that text is new. An item
    /// that is or holds an array with missing values has the text this module makes; a
    /// `str` or a `str | None` is put in quotes by the library, as Python writes one inside
    /// a tuple; any other item is read where it is, and the backend writes it. `None`
    /// after a report.
    fn item_text(
        &mut self,
        place: Place,
        ty: Type<'db>,
        node: NodeId,
    ) -> Option<(Place, Option<Local>)> {
        let text = Type::new(self.db, TypeKind::Str);
        let quoting = match ty.kind(self.db) {
            TypeKind::Str => Some("quoted"),
            TypeKind::Optional(inner) if matches!(inner.kind(self.db), TypeKind::Str) => {
                Some("quoted_or_none")
            }
            _ => None,
        };
        let made = if holds_gaps(self.db, ty) {
            self.held_text(&place, ty, node)?
        } else if let Some(function) = quoting {
            self.frame_call(function, Vec::new(), vec![place], text, node)?
        } else {
            return Some((place, None));
        };
        Some((Place::from(made), Some(made)))
    }
}
