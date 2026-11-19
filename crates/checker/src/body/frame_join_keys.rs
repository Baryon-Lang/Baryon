//! The keys and the suffixes of a `merge` (decision 0045, roadmap 3.13.15): `on=`, or
//! `left_on=` with `right_on=` for a key whose column has another name in each frame, and
//! `suffixes=`, the endings that tell apart two columns the frames both have.
//!
//! All of them are literals, since the joined frame's type lists its columns: which columns
//! it has depends on the keys, and their names on the suffixes.

use super::Body;
use syntax::{NodeId, Role, SyntaxKind};
use types::Type;

impl<'db> Body<'db> {
    /// The keys of a `merge`: for each, its column's name in the left frame and in the right.
    /// `written` is the value of `left_on=` and of `right_on=`, each one string literal or a
    /// list of them, or the value of `on=` twice; `frames` are the types of the two frames,
    /// the left first. Each name must be a column of its frame, and the two sides must name
    /// as many columns, at least one. `None` after an error.
    pub(super) fn join_keys(
        &mut self,
        written: [NodeId; 2],
        frames: [Type<'db>; 2],
    ) -> Option<Vec<(String, String)>> {
        let [left, right] = written;
        let first = self.key_names(left, frames[0]);
        if left == right {
            // `on=`: one name in both frames; the caller checks the right frame has it.
            return Some(
                first?
                    .into_iter()
                    .map(|name| (name.clone(), name))
                    .collect(),
            );
        }
        let second = self.key_names(right, frames[1]);
        let (first, second) = (first?, second?);
        if first.len() != second.len() {
            let message = format!(
                "`left_on` names {} and `right_on` names {}",
                counted(first.len()),
                counted(second.len())
            );
            let label = "another number of columns";
            let note = "the first of each is one key, the second the next, and so on";
            self.lowerer.error(right, &message, label, Some(note));
            return None;
        }
        Some(first.into_iter().zip(second).collect())
    }

    /// The column names `node` writes, one string literal or a list of them, each a column
    /// of the frame of type `frame`, at least one. `None` after an error.
    fn key_names(&mut self, node: NodeId, frame: Type<'db>) -> Option<Vec<String>> {
        let names = if self.tree.kind(node) == SyntaxKind::List {
            self.column_names(node, frame)
        } else {
            self.column_name(node, frame).map(|name| vec![name])
        }?;
        if names.is_empty() {
            let message = "`merge` needs at least one column to join on";
            self.lowerer.error(node, message, "no columns", None);
            return None;
        }
        Some(names)
    }

    /// The two endings `suffixes`, the `suffixes=` of a `merge`, writes: `_x` and `_y` when
    /// it is left out, as in pandas. It is a tuple or a list of two, each a string literal
    /// or `None`, which adds nothing, as `""` does. `None` after an error.
    pub(super) fn join_suffixes(&mut self, suffixes: Option<NodeId>) -> Option<[String; 2]> {
        let Some(suffixes) = suffixes else {
            return Some(["_x".to_owned(), "_y".to_owned()]);
        };
        let items: Vec<NodeId> = match self.tree.kind(suffixes) {
            SyntaxKind::Tuple | SyntaxKind::List => {
                self.tree.children_with(suffixes, Role::Item).collect()
            }
            _ => Vec::new(),
        };
        let ending = |body: &Self, item: NodeId| match body.tree.kind(item) {
            SyntaxKind::None => Some(String::new()),
            _ => body.literal_text(item).map(str::to_owned),
        };
        if let &[first, second] = items.as_slice()
            && let (Some(first), Some(second)) = (ending(self, first), ending(self, second))
        {
            return Some([first, second]);
        }
        self.expression(suffixes, None);
        self.lowerer.error(
            suffixes,
            "`suffixes` is two string literals, such as `(\"_left\", \"_right\")`",
            "not two suffixes",
            Some(
                "the joined frame's column names depend on them, so they must be known when \
                 compiling; `None` or `\"\"` adds nothing to a name",
            ),
        );
        None
    }
}

/// `count` columns, in words a message can use: `1 column`, `2 columns`.
fn counted(count: usize) -> String {
    if count == 1 {
        "1 column".to_owned()
    } else {
        format!("{count} columns")
    }
}
