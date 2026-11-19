//! A tuple or a list that holds an array with missing values, as text (decision 0045,
//! roadmap 3.24.4): what `print((bids, 1))` and `f"{[bids, asks]}"` write.
//!
//! The backend writes a tuple or a list in one step when it knows every value in it. An
//! array with missing values is stored as its values and a validity bitmap, which the
//! backend would write as a tuple of two arrays, so its text comes from the standard
//! library's `optional_shown`. This module therefore writes such a tuple or list piece by
//! piece, as Python writes one: the brackets, a comma and a space between two items, and
//! each item's text. An item with no such array in it is written by the backend as it
//! stands; only a `str` needs help, since inside a tuple it is written in quotes, which the
//! library's `quoted` does (`<std>/frame_holders.byn`).
//!
//! A tuple's items are known when compiling, so a tuple is one `Format`. A list is a loop
//! that gathers the text of each item into a `list[str]`, which the library's `listed`
//! joins. Cost: one text for each array, and for a list one more for each item.

use crate::builder::Builder;
use mir::{Argument, Builtin, Callee, Local, Operand, Piece, Place, Projection};
use mir::{Rvalue, StatementKind, TerminatorKind};
use syntax::NodeId;
use types::{FormatSpec, Type, TypeDb, TypeKind};

/// Whether `ty` is an array with missing values, or a tuple or a list that holds one at any
/// depth: the values whose text this module makes.
fn holds_gaps<'db>(db: &'db dyn TypeDb, ty: Type<'db>) -> bool {
    match ty.kind(db) {
        TypeKind::OptionalArray(_) => true,
        TypeKind::Tuple(items) => items.iter().any(|&item| holds_gaps(db, item)),
        &TypeKind::List(item) => holds_gaps(db, item),
        _ => false,
    }
}

impl<'db> Builder<'db> {
    /// The text of the tuple or list of type `ty` borrowed at `place`, when it holds an
    /// array with missing values: a new `str`, dropped with the statement's temporaries.
    /// `None` for any other value, and after a report.
    pub(crate) fn holder_text(
        &mut self,
        place: &Place,
        ty: Type<'db>,
        node: NodeId,
    ) -> Option<Place> {
        let holder = matches!(ty.kind(self.db), TypeKind::Tuple(_) | TypeKind::List(_));
        if !holder || !holds_gaps(self.db, ty) {
            return None;
        }
        let text = self.held_text(place, ty, node)?;
        self.drop_later(text);
        Some(Place::from(text))
    }

    /// The text of the value of type `ty` stored at `place`, which is or holds an array
    /// with missing values: a new `str` the caller owns, and drops or moves. It is not left
    /// to the statement's end, since a list makes one in every turn of its loop. `None`
    /// after a report.
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
            let nothing = builder.temporary(Type::new(builder.db, TypeKind::None), node);
            builder.then(node, |next| TerminatorKind::Call {
                callee: Callee::Builtin(Builtin::Append),
                arguments: vec![
                    Argument::InOut(Place::from(texts)),
                    Argument::Owned(Operand::Move(Place::from(shown))),
                ],
                destination: Place::from(nothing),
                target: next,
                error: None,
            });
        });
        let arguments = vec![Place::from(texts)];
        let result = self.frame_call("listed", Vec::new(), arguments, text, node);
        self.push(StatementKind::Drop(Place::from(texts)), node);
        result.filter(|_| complete)
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
