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

    /// What the call `node`, a reduction of a grouping, makes of each column of `made`,
    /// the columns of its result after the keys, in their order: the node an error about
    /// the column points at, the name of the column read, and the reduction, as the number
    /// [`REDUCTIONS`] holds it at. `None` when the syntax is not what the checker passed.
    ///
    /// `agg` names all three in each argument. A method reads the column that has the new
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
        let mut reductions = Vec::with_capacity(made.len());
        for argument in self.written(node) {
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
