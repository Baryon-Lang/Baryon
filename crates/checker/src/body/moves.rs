//! Moves (language reference, "Values and references", "Moves" and "Parameters"): a value that
//! is not a copy type moves when it is assigned, returned, passed as `Owned`, or placed in a
//! struct, tuple or list being built. After a move, the old owner holds nothing until it is
//! assigned again, and reading it is an error that points at the move:
//!
//! ```text
//! error: `a` was moved
//!    |
//! 11 |     b = a
//!    |         - `a` moved here
//! 12 |     total = a.sum()
//!    |             ^ read after the move
//! ```
//!
//! Only a local variable or an `Owned` parameter can be moved. A read-only or `InOut` parameter
//! belongs to the caller, so moving it, including returning it, is an error that suggests a
//! copy. Moving a field out moves the whole struct. Moving an element out of an array or a
//! list is an error, as is moving a field out of an element, `people[0].name`: the container
//! still owns the element and would free it a second time. Keeping a slice is an error too: a
//! slice is a view of its array, a borrow, which can be passed to a call or assigned to but not
//! stored.
//!
//! A variable moved on some path counts as moved after the paths join, and one moved inside a
//! loop, and not assigned again before the loop's next pass, is reported at the move.

use super::Body;
use super::flow::State;
use crate::traits::{Bound, Trait};
use database::Definition;
use diagnostics::Diagnostic;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Key, Mode, StandardType, Type, TypeKind};

impl<'db> Body<'db> {
    /// Whether a value of `ty` moves when it is used as a value, rather than being copied.
    pub(super) fn moves(&self, ty: Type<'db>) -> bool {
        match ty.kind(self.db) {
            TypeKind::Str
            | TypeKind::TextArray
            | TypeKind::List(_)
            | TypeKind::Dict { .. }
            | TypeKind::Array { .. }
            | TypeKind::DataFrame { .. }
            | TypeKind::Standard(_) => true,
            // A union of `Copy` members is `Copy`; otherwise it moves (decision 0032).
            TypeKind::Tuple(items) | TypeKind::Union(items) => {
                items.iter().any(|&item| self.moves(item))
            }
            TypeKind::Optional(value) => self.moves(*value),
            TypeKind::Struct(_) => !crate::forms::is_copy(self.db, ty),
            // A bound that is not a trait has had its error; its values count as copies.
            TypeKind::Parameter { file, name } => !matches!(
                crate::traits::written_bound(self.db, self.lowerer.package, *file, *name),
                Some(Bound::Other | Bound::Trait(Trait::Copy | Trait::Number | Trait::Float))
            ),
            _ => false,
        }
    }

    /// Moves the value of the expression `node`, of type `ty`, when it is not a copy type:
    /// records its variable as moved, or reports a value that cannot be moved.
    pub(super) fn consume(&mut self, node: NodeId, ty: Type<'db>) {
        if !self.moves(ty) {
            return;
        }
        match self.tree.kind(node) {
            SyntaxKind::Name | SyntaxKind::Attribute if self.borrowed_by_loop(node, "moved") => {}
            SyntaxKind::Name => self.move_variable(node, node),
            SyntaxKind::Attribute if self.is_frame_attribute(node) => {}
            // `df[i]` is a new struct each time it is read, so keeping it moves nothing.
            SyntaxKind::Subscript if self.is_frame_row(node) => {}
            SyntaxKind::Attribute | SyntaxKind::Subscript if self.is_column_read(node) => {
                self.moved_out_of_frame(node);
            }
            // `path.name` and the other parts of a `Path` are new values each time.
            SyntaxKind::Attribute if self.is_path_part(node) => {}
            // A field of an element, `people[0].name`, is part of the element: moving it out
            // is moving out of an element, the error below.
            SyntaxKind::Attribute if self.inside_element(node) => self.moved_out_of_element(node),
            SyntaxKind::Attribute => {
                // Moving a field out moves the whole struct: one with a hole cannot be used.
                if let Some(root) = self.place_root(node) {
                    self.move_variable(root, node);
                }
            }
            // `a if c else b` keeps whichever branch runs, so each branch is moved: a borrowed
            // element or a variable in either one is the same error as moving it directly.
            SyntaxKind::Conditional => {
                self.kept.insert(node);
                for role in [Role::Value, Role::Else] {
                    if let Some(branch) = self.tree.child(node, role) {
                        self.consume(branch, ty);
                    }
                }
            }
            // `os.environ["HOME"]` is a new `str` each time it is read, so keeping it moves
            // nothing out of anything.
            SyntaxKind::Subscript
                if self
                    .tree
                    .child(node, Role::Value)
                    .is_some_and(|value| self.is_environ(value)) => {}
            // `header["OBJECT"]` is a call of a library function, which gives a new value.
            SyntaxKind::Subscript if self.typed.calls.contains_key(&Key(node)) => {}
            // A slice of a list is a new list, and a slice of a `str` a new `str`, as in
            // Python, so keeping either moves nothing.
            SyntaxKind::Subscript
                if self.is_view(node)
                    && matches!(ty.kind(self.db), TypeKind::List(_) | TypeKind::Str) => {}
            SyntaxKind::Subscript if self.is_view(node) => {
                // A view borrows the array, and a borrow cannot be stored (reference,
                // "Slices").
                let hint = format!(
                    "a slice can be passed to a call or assigned to, but to keep it, copy it: `{}.copy()`",
                    self.tree.text(node, self.text)
                );
                self.lowerer.error(
                    node,
                    "a slice of an array cannot be kept",
                    "a view of the array's memory",
                    Some(&hint),
                );
            }
            // `a[mask]` is a new array, so keeping it moves nothing.
            SyntaxKind::Subscript if self.is_selection(node) => {}
            SyntaxKind::Subscript => self.moved_out_of_element(node),
            _ => {}
        }
    }

    /// Whether the field access `node` reads a field of an element, as `people[0].name` and
    /// `grid[i].cell.label` do: whether a subscript sits between it and its variable.
    fn inside_element(&self, node: NodeId) -> bool {
        let mut place = node;
        while let Some(value) = self.tree.child(place, Role::Value) {
            match self.tree.kind(value) {
                SyntaxKind::Subscript => return true,
                SyntaxKind::Attribute => place = value,
                _ => return false,
            }
        }
        false
    }

    /// Reports `node`, an element or a field of one, moved out of its container. The container
    /// still owns the element and frees it, so the move would free it twice; a copy is the
    /// way to keep it.
    fn moved_out_of_element(&mut self, node: NodeId) {
        let hint = format!(
            "move a copy instead: `{}.copy()`",
            self.tree.text(node, self.text)
        );
        self.lowerer.error(
            node,
            "a value cannot be moved out of an element",
            "moved out of an element",
            Some(&hint),
        );
    }

    /// Whether the subscript `node` is a slice, `a[1:n]` or `grid[:, 0]`, or adds a new axis,
    /// `b[:, None]`: a view of its array rather than an element.
    pub(super) fn is_view(&self, node: NodeId) -> bool {
        let Some(index) = self.tree.child(node, Role::Index) else {
            return false;
        };
        let slice =
            |part: NodeId| matches!(self.tree.kind(part), SyntaxKind::Slice | SyntaxKind::None);
        slice(index)
            || (self.tree.kind(index) == SyntaxKind::Tuple
                && self.tree.children_with(index, Role::Item).any(slice))
    }

    /// The variable a place such as `atoms.forces[i]` starts at: its `Name` node, or `None`
    /// when it starts at something else, such as a call.
    pub(super) fn place_root(&self, node: NodeId) -> Option<NodeId> {
        match self.tree.kind(node) {
            SyntaxKind::Name => Some(node),
            SyntaxKind::Attribute | SyntaxKind::Subscript => {
                self.place_root(self.tree.child(node, Role::Value)?)
            }
            _ => None,
        }
    }

    /// Moves the variable named at `name`, at the move site `site`: all of it, or the field of
    /// it that `site` names.
    fn move_variable(&mut self, name: NodeId, site: NodeId) {
        let Some(&Definition::Local(binding)) = self.lowerer.definition(name) else {
            return;
        };
        let text = self.tree.text(name, self.text);
        match self.modes.get(&binding) {
            Some(Mode::Borrow) => {
                let message =
                    format!("`{text}` is borrowed from the caller, so it cannot be moved");
                let hint = format!(
                    "move a copy instead: `{}.copy()`, or take the parameter as `Owned[...]`",
                    self.tree.text(site, self.text)
                );
                self.lowerer
                    .error(site, &message, "moved here", Some(&hint));
            }
            Some(Mode::InOut) => {
                let message = format!(
                    "`{text}` is borrowed from the caller as `InOut`, so it cannot be moved"
                );
                let hint = format!(
                    "move a copy instead: `{}.copy()`, or assign it a new value in place",
                    self.tree.text(site, self.text)
                );
                self.lowerer
                    .error(site, &message, "moved here", Some(&hint));
            }
            Some(Mode::Owned) | None => {
                self.state.moved.insert(binding, site);
            }
        }
    }

    /// Reports a read, at `node`, of the variable `binding` after it was moved, and forgets the
    /// move so that each move is reported once.
    pub(super) fn read_moved(&mut self, node: NodeId, binding: NodeId) {
        let Some(site) = self.state.moved.remove(&binding) else {
            return;
        };
        let name = self.tree.text(node, self.text);
        if self.loop_ends.contains(&site) {
            let diagnostic = Diagnostic::error(format!("`{name}` is read after its loop ended"))
                .with_primary(self.tree.span(node), "read after the loop")
                .with_secondary(
                    self.tree.span(site),
                    format!("`{name}` borrows each element only while the loop runs"),
                )
                .with_note(format!(
                    "to keep an element, copy it inside the loop: `last = {name}.copy()`"
                ));
            self.lowerer.diagnostics.push(diagnostic);
            return;
        }
        let standard = self
            .locals
            .get(&binding)
            .and_then(|ty| match ty.kind(self.db) {
                TypeKind::Standard(standard) => Some(*standard),
                _ => None,
            });
        let advice = if standard == Some(StandardType::Philox) {
            // A generator has no copy: two copies would draw the same numbers (decision 0028).
            "a generator cannot be copied, so no two draw the same numbers; make another with its \
             own `stream`"
                .to_owned()
        } else if standard == Some(StandardType::File) {
            "a file has one owner, and `close()` or `with` ends it; open it again to read it again"
                .to_owned()
        } else {
            format!(
                "to keep both, move a copy: `{}.copy()`",
                self.tree.text(site, self.text)
            )
        };
        let diagnostic = Diagnostic::error(format!("`{name}` was moved"))
            .with_primary(self.tree.span(node), "read after the move")
            .with_secondary(self.tree.span(site), format!("`{name}` moved here"))
            .with_note(advice);
        self.lowerer.diagnostics.push(diagnostic);
    }

    /// Reports each variable the loop `statement` moved and did not assign anywhere in it,
    /// given what was known before the loop, `entry`, and at the end of its body, `end`: the
    /// next pass would read a moved value.
    pub(super) fn moved_in_loop(
        &mut self,
        statement: NodeId,
        entry: &State<'db>,
        end: &State<'db>,
    ) {
        for (&binding, &site) in &end.moved {
            if entry.moved.contains_key(&binding)
                || !entry.assigned.contains(&binding)
                || self.assigns(statement, binding)
            {
                continue;
            }
            let name = self.tree.text(binding, self.text);
            let message = format!("`{name}` is moved in one pass of the loop and read in the next");
            self.lowerer.error(
                site,
                &message,
                "moved here, inside a loop",
                Some(
                    "move a copy, `.copy()`, or assign the variable a new value later in the loop",
                ),
            );
        }
    }

    /// Whether any statement inside `node` assigns the variable `binding`.
    fn assigns(&self, node: NodeId, binding: NodeId) -> bool {
        let mut bound = std::collections::BTreeSet::new();
        self.statement_bindings(node, &mut bound);
        bound.contains(&binding)
    }
}
