//! Text columns (decision 0045): a column of `str` in a `DataFrame` is a text array,
//! `Array[str, 1]`. A frame with one is built from its columns, counted, copied and passed
//! around, and the column is read as a text array. Every other use of such a frame is
//! reported here as not built yet, so none reaches lowering by accident.

use super::Body;
use super::calls::Arguments;
use syntax::NodeId;
use types::{Type, TypeKind};

/// What a frame with a text column can do today, for messages.
const BUILT: &str = "a frame with a column of `str` is built from its columns, `DataFrame({...})`, \
                     and has `len`, `shape`, `columns`, `copy()`, `print`, `head`, `tail`, \
                     slices, masks, `sort_values`, `drop_duplicates`, `assign`, `drop`, \
                     `rename`, `df[[...]]`, `concat` and its columns, such as `df.symbol`, \
                     today; the rest comes with roadmap item 3.13.3";

impl<'db> Body<'db> {
    /// Whether `ty` is a frame with a column of `str`.
    pub(super) fn has_text_column(&self, ty: Type<'db>) -> bool {
        match ty.kind(self.db) {
            TypeKind::DataFrame { columns, .. } => columns
                .iter()
                .any(|column| matches!(column.element.kind(self.db), TypeKind::Str)),
            _ => false,
        }
    }

    /// Reports `what`, such as "the method `head`" or "`concat`", used at `node` on a frame
    /// with a text column, which does not have it yet, and gives the unknown type.
    pub(super) fn text_column_unbuilt(&mut self, node: NodeId, what: &str) -> Type<'db> {
        let message = format!("{what} is not built for a `DataFrame` with a column of `str` yet");
        self.lowerer
            .error(node, &message, "has a column of `str`", Some(BUILT))
    }

    /// Reports `df.groupby(...).agg(...)`, the call `node`, on a frame with a text column,
    /// after typing the arguments of `groupby`, the call `grouping`, and of `agg`, so that
    /// what they read is checked too.
    pub(super) fn text_column_grouped(
        &mut self,
        node: NodeId,
        grouping: Option<NodeId>,
        arguments: &Arguments,
    ) -> Type<'db> {
        if let Some(grouping) = grouping {
            let by = self.arguments(grouping);
            self.untyped(&by);
        }
        self.untyped(arguments);
        self.text_column_unbuilt(node, "`groupby`")
    }
}
