//! The text of a missing key for its panic, as Python's `repr` writes it: `'Ar'`, `3`,
//! `(1, 2)`, `True` or `Phase.GAS`. It is built only on the failing path.

use crate::builder::Builder;
use mir::{
    BlockId, Constant, ConstantValue, Local, Operand, Piece, Place, Projection, Rvalue,
    TerminatorKind,
};
use syntax::NodeId;
use types::{Declaration, FormatSpec, Item, Type, TypeKind};

impl<'db> Builder<'db> {
    /// A new `str` holding the key at `key`, of type `ty`, as Python's `repr` writes it, for a
    /// missing key's panic: `'Ar'`, `3`, `(1, 2)`, `True` or `Phase.GAS`.
    pub(super) fn key_text(&mut self, key: &Place, ty: Type<'db>, node: NodeId) -> Operand<'db> {
        let mut pieces = Vec::new();
        self.key_pieces(key, ty, &mut pieces, node);
        let text_type = Type::new(self.db, TypeKind::Str);
        let text = self.temporary(text_type, node);
        self.assign(Place::from(text), Rvalue::Format(pieces), node);
        Operand::Move(Place::from(text))
    }

    /// Appends the pieces that write the key part at `key`, of type `ty`, to `pieces`.
    fn key_pieces(&mut self, key: &Place, ty: Type<'db>, pieces: &mut Vec<Piece>, node: NodeId) {
        let text = |text: &str| Piece::Text(text.to_owned());
        match ty.kind(self.db) {
            TypeKind::Str => {
                pieces.push(text("'"));
                pieces.push(Piece::Value(key.clone(), FormatSpec::default()));
                pieces.push(text("'"));
            }
            TypeKind::Tuple(items) => {
                pieces.push(text("("));
                for (field, &item) in (0u32..).zip(items) {
                    if field > 0 {
                        pieces.push(text(", "));
                    }
                    let part = key.clone().project(Projection::Field(field));
                    self.key_pieces(&part, item, pieces, node);
                }
                pieces.push(text(if items.len() == 1 { ",)" } else { ")" }));
            }
            TypeKind::Enum(nominal) => {
                let name = self.enum_member_text(key, ty, nominal, node);
                pieces.push(Piece::Value(Place::from(name), FormatSpec::default()));
            }
            _ => pieces.push(Piece::Value(key.clone(), FormatSpec::default())),
        }
    }

    /// A `str` local holding `Class.MEMBER` for the enum value at `key`: a switch on the value
    /// to one block per member, each writing its name.
    fn enum_member_text(
        &mut self,
        key: &Place,
        ty: Type<'db>,
        nominal: &types::Nominal<'db>,
        node: NodeId,
    ) -> Local {
        let text_type = Type::new(self.db, TypeKind::Str);
        let name = self.temporary(text_type, node);
        let item = Item::new(self.db, self.package, nominal.file, nominal.name);
        let Declaration::Enum { variants } = &self.db.declaration(item).declaration else {
            return name;
        };
        let class = ty.display(self.db).to_string();
        let blocks: Vec<BlockId> = variants.iter().map(|_| self.new_block()).collect();
        let (otherwise, join) = (self.new_block(), self.new_block());
        self.terminate(
            TerminatorKind::Switch {
                value: Operand::Copy(key.clone()),
                cases: (0..).zip(blocks.iter().copied()).collect(),
                otherwise,
            },
            node,
        );
        self.start(otherwise);
        self.terminate(TerminatorKind::Unreachable, node);
        for (variant, block) in variants.iter().zip(blocks) {
            self.start(block);
            let constant = Constant {
                ty: text_type,
                value: ConstantValue::Str(format!("{class}.{variant}")),
            };
            self.assign(
                Place::from(name),
                Rvalue::Use(Operand::Constant(constant)),
                node,
            );
            self.terminate(TerminatorKind::Goto(join), node);
        }
        self.start(join);
        name
    }
}
