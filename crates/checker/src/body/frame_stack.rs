//! Stacking frames (decision 0045, roadmap 3.13.9): `concat([first, second])`, the rows of
//! the first frame, then those of the second, and so on.
//!
//! The frames have one schema: the same columns, in the same order, with the same types.
//! The compiler checks that, so no column of the result has a missing value, and the result
//! has the type of the first frame. The frames are written out in a list, since each is
//! read where it stands and none is moved.

use super::calls::Arguments;
use super::{Body, relations};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Type, TypeKind};

impl<'db> Body<'db> {
    /// The call `node` of the built-in `concat`: the type of the first frame of its list,
    /// after checking that every other frame has the same columns.
    pub(super) fn frame_concat(&mut self, node: NodeId, arguments: &Arguments) -> Type<'db> {
        let list = match (arguments.positional.as_slice(), arguments.keywords.len()) {
            (&[list], 0) if self.tree.kind(list) == SyntaxKind::List => list,
            _ => {
                // The frames of a list are read here too, so that none is reported as moved.
                let given = arguments.positional.iter().copied();
                let given = given.chain(arguments.keywords.iter().map(|&(_, value)| value));
                for argument in given.collect::<Vec<_>>() {
                    if self.tree.kind(argument) == SyntaxKind::List {
                        let frames: Vec<NodeId> =
                            self.tree.children_with(argument, Role::Item).collect();
                        for frame in frames {
                            self.expression(frame, None);
                        }
                    } else {
                        self.expression(argument, None);
                    }
                }
                return self.lowerer.error(
                    node,
                    "`concat` takes the frames to stack, written out in a list",
                    "wrong arguments",
                    Some(
                        "for example `concat([january, february])`; pandas's `axis=`, \
                         `join=` and `ignore_index=` are not built",
                    ),
                );
            }
        };
        let frames: Vec<NodeId> = self.tree.children_with(list, Role::Item).collect();
        if frames.is_empty() {
            return self.lowerer.error(
                list,
                "`concat` needs at least one frame to stack",
                "no frames",
                None,
            );
        }
        let mut first: Option<Type<'db>> = None;
        let mut complete = true;
        for frame in frames {
            let found = self.expression(frame, first);
            if relations::is_open(self.db, found) {
                complete = false;
                continue;
            }
            let TypeKind::DataFrame { columns, .. } = found.kind(self.db) else {
                let message = format!("`concat` stacks frames, found `{}`", self.show(found));
                self.lowerer
                    .error(frame, &message, "not a `DataFrame`", None);
                complete = false;
                continue;
            };
            let Some(first) = first else {
                first = Some(found);
                continue;
            };
            let TypeKind::DataFrame {
                columns: wanted, ..
            } = first.kind(self.db)
            else {
                continue;
            };
            if columns != wanted {
                let message = format!(
                    "`concat` stacks frames of one schema: this frame is `{}`, the first is `{}`",
                    self.show(found),
                    self.show(first)
                );
                self.lowerer.error(
                    frame,
                    &message,
                    "other columns",
                    Some(
                        "give the frames the same columns, in one order and of one type, \
                         with `df[[...]]`, `rename` or `assign` first",
                    ),
                );
                complete = false;
            }
        }
        match first {
            Some(first) if complete => first,
            _ => self.lowerer.unknown(),
        }
    }
}
