//! The spellings of a grouping's reductions (decision 0045 and its amendment of 2026-11-19,
//! roadmap 3.13.22): `df.groupby(k).agg(name=("column", "reduction"))`, and the reductions
//! written as methods, `df.groupby(k).sum()`, `df.groupby(k)["v"].mean()`,
//! `df.groupby(k)[["v", "n"]].max()`, `df.groupby(k).v.min()` and `size()`.
//!
//! Every spelling is the same work, which [`Builder::frame_agg`] builds: this module reads
//! from the syntax where the grouping is and which column each column of the result
//! reduces, and how. A method reduces each column into one of its own name, so there the
//! columns are read from the type the checker gave the result.

use crate::builder::Builder;
use crate::frame_groups::REDUCTIONS;
use mir::{Constant, ConstantValue, Local, Operand, Place, Rvalue};
use syntax::{NodeId, Role, SyntaxKind};
use types::Column;

impl<'db> Builder<'db> {
    /// The call `frame.groupby(...)` inside the call `node`, a reduction of a grouping:
    /// what the reduction is a method of, or what the columns it is a method of were
    /// chosen from, by `[...]` or by `.name`.
    pub(crate) fn grouping_call(&self, node: NodeId) -> Option<NodeId> {
        let function = self.tree.child(node, Role::Function)?;
        let value = self.tree.child(function, Role::Value)?;
        match self.tree.kind(value) {
            SyntaxKind::Subscript | SyntaxKind::Attribute => self.tree.child(value, Role::Value),
            _ => Some(value),
        }
    }

    /// What the `groupby` inside the call `node`, a reduction of a grouping, was given: the
    /// node that names the key columns, its first argument or `by=`, and whether the groups
    /// come sorted by their keys, which `sort=False` turns off (decision 0045's amendment,
    /// point 3). `as_index=False` changes nothing, so it is not read.
    pub(crate) fn grouping_arguments(&self, node: NodeId) -> Option<(NodeId, bool)> {
        let grouping = self.grouping_call(node)?;
        let by = match self.keyword_of(grouping, "by") {
            Some(by) => by,
            None => self.written(grouping).first().copied()?,
        };
        let sorted = self
            .keyword_of(grouping, "sort")
            .is_none_or(|sort| self.text_of(sort) != "False");
        Some((by, sorted))
    }

    /// The order of the groups of a grouping, from `firsts`, the position of the first row
    /// of each group with the groups sorted by their keys. When `sorted`, that is the order:
    /// `firsts` as it is, and nothing to reorder by. For `sort=False`, `firsts` put in
    /// rising order, which is the order the groups first appear in the frame, and the place
    /// of the array that says which sorted group each place of that order holds, by which
    /// the reduced columns are reordered. The statement drops both. `None` after a report.
    /// Cost for `sort=False`: a sort of one number for each group.
    pub(crate) fn first_seen(
        &mut self,
        firsts: Local,
        sorted: bool,
        node: NodeId,
    ) -> Option<(Local, Option<Place>)> {
        if sorted {
            return Some((firsts, None));
        }
        let int = self.int_type();
        let boolean = self.boolean_type();
        let positions = self.positions_type();
        let count = self.temporary(int, node);
        let length = Rvalue::Length(Place::from(firsts), 0);
        self.assign(Place::from(count), length, node);
        let start = vec![Place::from(count)];
        let start = self.frame_call("row_positions", Vec::new(), start, positions, node)?;
        self.drop_later(start);
        let rising = Operand::Constant(Constant {
            ty: boolean,
            value: ConstantValue::Bool(true),
        });
        let rising = self.held(boolean, rising, node);
        let arguments = vec![Place::from(firsts), Place::from(start), Place::from(rising)];
        let turn = self.frame_call("ordered", vec![int], arguments, positions, node)?;
        self.drop_later(turn);
        let arguments = vec![Place::from(firsts), Place::from(turn)];
        let seen = self.frame_call("taken", vec![int], arguments, positions, node)?;
        self.drop_later(seen);
        Some((seen, Some(Place::from(turn))))
    }

    /// What the call `node`, a reduction of a grouping, makes of each column of `made`,
    /// the columns of its result after the keys, in their order: the node an error about
    /// the column points at, the name of the column read, and the reduction, as the number
    /// [`REDUCTIONS`] holds it at. `None` when the syntax is not what the checker passed.
    ///
    /// `agg` names all three in each argument, or in each entry of its dict. A method, and
    /// `agg` with one name, reads the column that has the new
    /// column's name; `size` reads none, so the name it is given is not looked up.
    pub(crate) fn group_reductions(
        &self,
        node: NodeId,
        made: &[Column<'db>],
    ) -> Option<Vec<(NodeId, String, i128)>> {
        let numbered = |how: &str| {
            let how = REDUCTIONS.iter().position(|name| *name == how)?;
            i128::try_from(how).ok()
        };
        let function = self.tree.child(node, Role::Function)?;
        let method = self.tree.child(function, Role::Attribute)?;
        let method = self.text_of(method);
        if method != "agg" {
            let how = numbered(method)?;
            let read = made.iter().map(|new| (node, new.name.clone(), how));
            return Some(read.collect());
        }
        let written = self.written(node);
        match *written.as_slice() {
            // `agg("sum")` is `sum()`.
            [form] if self.tree.kind(form) == SyntaxKind::StringLiteral => {
                let how = numbered(self.written_text(form))?;
                let read = made.iter().map(|new| (node, new.name.clone(), how));
                return Some(read.collect());
            }
            // `agg({"v": "sum"})` reduces the column each entry names its own way.
            [form] if self.tree.kind(form) == SyntaxKind::Dictionary => {
                let entries = self.tree.children_with(form, Role::Item);
                return entries
                    .map(|entry| {
                        let read = self.tree.child(entry, Role::Key)?;
                        let how = self.tree.child(entry, Role::Value)?;
                        let how = numbered(self.written_text(how))?;
                        Some((entry, self.written_text(read).to_owned(), how))
                    })
                    .collect();
            }
            _ => {}
        }
        let mut reductions = Vec::with_capacity(made.len());
        for argument in written {
            let pair: Vec<NodeId> = self
                .tree
                .children_with(self.keyword_value(argument), Role::Item)
                .collect();
            let (&read, &how) = (pair.first()?, pair.get(1)?);
            let how = numbered(self.written_text(how))?;
            reductions.push((argument, self.written_text(read).to_owned(), how));
        }
        Some(reductions)
    }
}
