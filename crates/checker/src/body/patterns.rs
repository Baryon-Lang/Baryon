//! `match` statements (language reference, "Syntax", and "Types", "Enums"): each case's
//! pattern is checked against the subject's type, captures are bound, and the cases must cover
//! every value of the subject.
//!
//! Version 0 patterns are literals, enum members, `True`, `False` and `None`, tuples, class
//! patterns such as `Dirichlet(value=v)` (the `class_patterns` module, decision 0032),
//! captures, the wildcard `_`, and `a | b` of these. Mapping and star patterns are errors. After
//! `case None:`, the later cases of a `T | None` subject match a `T`. The
//! `exhaustive` module decides whether the cases cover every value and names one they miss:
//!
//! ```text
//! error: this `match` does not handle `Boundary.ABSORBING`
//! ```

use super::exhaustive::{self, Pattern, Space};
use super::flow::{End, Skip};
use super::{Body, relations};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Declaration, Item, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// Checks a `match` statement: its subject, each case's pattern, guard and body, and that
    /// the cases without a guard cover every value of the subject.
    ///
    /// Returns whether the statement can fall through: when a case's body can, or when the
    /// cases do not cover every value.
    pub(super) fn match_statement(&mut self, statement: NodeId) -> bool {
        let Some(value) = self.tree.child(statement, Role::Value) else {
            return true;
        };
        let subject = self.expression(value, None);
        let entry = self.state.clone();
        let mut ends = Vec::new();
        let mut rows = Vec::new();
        // After an unguarded `case None:`, the later cases see a `T | None` subject as a `T`.
        let mut remaining = subject;
        let cases: Vec<NodeId> = self.tree.children_with(statement, Role::Case).collect();
        for case in cases {
            self.state.clone_from(&entry);
            let written = self.tree.child(case, Role::Pattern);
            let pattern = match written {
                Some(pattern) => self.pattern(pattern, remaining),
                None => Pattern::Wild,
            };
            if let Some(guard) = self.tree.child(case, Role::Condition) {
                self.condition(Some(guard));
            } else {
                if let (Pattern::None, TypeKind::Optional(value)) =
                    (&pattern, remaining.kind(self.db))
                {
                    remaining = *value;
                }
                rows.push(pattern);
            }
            if self.block(case, Role::Body) {
                let label = "not assigned when this case matches";
                ends.push(End {
                    state: self.state.clone(),
                    skip: Skip {
                        node: written.unwrap_or(case),
                        label,
                    },
                });
            }
        }
        // A subject of unknown type has had its error; its cases count as covering it.
        let missing = (!relations::is_open(self.db, subject))
            .then(|| exhaustive::missing(&rows, &self.space(subject)))
            .flatten();
        if let Some(example) = missing {
            self.not_exhaustive(value, subject, &example);
            let label = "not assigned when no case matches";
            ends.push(End {
                state: entry.clone(),
                skip: Skip { node: value, label },
            });
        }
        self.merge(entry, &ends)
    }

    /// Reports a `match` on `subject`, at `value`, whose cases miss `example`.
    fn not_exhaustive(&mut self, value: NodeId, subject: Type<'db>, example: &str) {
        let guards = "a `case` with an `if` guard may not match, so it does not count";
        let (message, hint) = if example == "_" {
            (
                format!(
                    "this `match` does not handle every value of type `{}`",
                    self.show(subject)
                ),
                format!("add `case _:` at the end; {guards}"),
            )
        } else {
            (
                format!("this `match` does not handle `{example}`"),
                format!(
                    "add `case {example}:`, or `case _:` for every value not matched yet; {guards}"
                ),
            )
        };
        self.lowerer
            .error(value, &message, "not every value is handled", Some(&hint));
    }

    /// Checks a pattern against the subject type `ty`, binds its captures, and reduces it for
    /// exhaustiveness. A pattern with an error counts as a wildcard, so one mistake gives one
    /// message.
    pub(super) fn pattern(&mut self, node: NodeId, ty: Type<'db>) -> Pattern {
        if let TypeKind::Union(members) = ty.kind(self.db)
            && !matches!(
                self.tree.kind(node),
                SyntaxKind::PatternAs | SyntaxKind::PatternOr
            )
        {
            return self.member_pattern(node, ty, members);
        }
        self.plain_pattern(node, ty)
    }

    /// [`pattern`](Self::pattern) on a subject that is not a union, or a capture or `a | b`
    /// on one.
    pub(super) fn plain_pattern(&mut self, node: NodeId, ty: Type<'db>) -> Pattern {
        match self.tree.kind(node) {
            SyntaxKind::PatternAs => {
                if let Some(name) = self.tree.child(node, Role::Name) {
                    self.bind(name, ty, None);
                }
                match self.tree.child(node, Role::Pattern) {
                    Some(inner) => self.pattern(inner, ty),
                    None => Pattern::Wild,
                }
            }
            SyntaxKind::PatternOr => {
                let items: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
                Pattern::Or(
                    items
                        .into_iter()
                        .map(|item| self.pattern(item, ty))
                        .collect(),
                )
            }
            SyntaxKind::PatternValue => match self.tree.child(node, Role::Value) {
                Some(value) => {
                    let (pattern, _) = self.value_pattern(value, ty);
                    self.record_member(node, &pattern);
                    pattern
                }
                None => Pattern::Wild,
            },
            SyntaxKind::PatternSingleton => self.singleton_pattern(node, ty),
            SyntaxKind::PatternSequence => self.tuple_pattern(node, ty),
            SyntaxKind::PatternClass => self.class_pattern(node, ty),
            _ => {
                self.lowerer.error(
                    node,
                    "this kind of pattern is not supported",
                    "not supported",
                    Some("a `case` matches literals, enum members, `True`, `False`, `None`, tuples, classes, names and `_`"),
                );
                self.assume_captured(node);
                Pattern::Wild
            }
        }
    }

    /// Records, for lowering, which enum member the value pattern `node` matches.
    pub(super) fn record_member(&mut self, node: NodeId, pattern: &Pattern) {
        if let Pattern::Member(index) = *pattern
            && let Ok(index) = u32::try_from(index)
        {
            self.typed.patterns.insert(types::Key(node), index);
        }
    }

    /// A literal or an enum member, such as `0`, `"x"` or `Boundary.PERIODIC`, with the type
    /// of the value it compares against.
    pub(super) fn value_pattern(&mut self, value: NodeId, ty: Type<'db>) -> (Pattern, Type<'db>) {
        let expected = match ty.kind(self.db) {
            TypeKind::Optional(inner) => *inner,
            _ => ty,
        };
        let found = self.expression(value, Some(expected));
        if !relations::compatible(self.db, expected, found) {
            self.pattern_error(value, found, ty);
            return (Pattern::Wild, found);
        }
        if let TypeKind::Enum(nominal) = found.kind(self.db)
            && self.tree.kind(value) == SyntaxKind::Attribute
            && let Some(attribute) = self.tree.child(value, Role::Attribute)
        {
            let item = Item::new(self.db, self.lowerer.package, nominal.file, nominal.name);
            let wanted = self.tree.text(attribute, self.text);
            if let Declaration::Enum { variants } = &self.db.declaration(item).declaration
                && let Some(index) = variants.iter().position(|variant| variant == wanted)
            {
                return (Pattern::Member(index), found);
            }
            return (Pattern::Wild, found);
        }
        (Pattern::Literal, found)
    }

    /// `True`, `False` or `None`.
    fn singleton_pattern(&mut self, node: NodeId, ty: Type<'db>) -> Pattern {
        let written = self.tree.text(node, self.text);
        let (pattern, found) = match written {
            "None" => (Pattern::None, Type::new(self.db, TypeKind::None)),
            _ => (
                Pattern::Bool(written == "True"),
                Type::new(self.db, TypeKind::Primitive(Primitive::Bool)),
            ),
        };
        if relations::compatible(self.db, ty, found) {
            pattern
        } else {
            self.pattern_error(node, found, ty);
            Pattern::Wild
        }
    }

    /// `(a, b)`: a tuple of the subject's length.
    fn tuple_pattern(&mut self, node: NodeId, ty: Type<'db>) -> Pattern {
        let items: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        let types = match ty.kind(self.db) {
            TypeKind::Tuple(types) if types.len() == items.len() => types.clone(),
            TypeKind::Unknown => vec![ty; items.len()],
            _ => {
                let message = format!(
                    "a tuple pattern of {} items cannot match a value of type `{}`",
                    items.len(),
                    self.show(ty)
                );
                self.lowerer
                    .error(node, &message, "does not match the subject", None);
                self.assume_captured(node);
                return Pattern::Wild;
            }
        };
        let patterns = items
            .into_iter()
            .zip(types)
            .map(|(item, ty)| self.pattern(item, ty))
            .collect();
        Pattern::Tuple(patterns)
    }

    /// Counts the names a pattern with an error captures as assigned, so a read of one is not
    /// reported a second time.
    pub(super) fn assume_captured(&mut self, pattern: NodeId) {
        let mut captured = std::collections::BTreeSet::new();
        self.pattern_names(pattern, &mut captured);
        self.state.assigned.extend(captured);
    }

    /// Reports a pattern whose type cannot match the subject's.
    fn pattern_error(&mut self, node: NodeId, found: Type<'db>, subject: Type<'db>) {
        let message = format!(
            "a pattern of type `{}` cannot match a value of type `{}`",
            self.show(found),
            self.show(subject)
        );
        self.lowerer
            .error(node, &message, "does not match the subject", None);
    }

    /// What the values of `ty` can look like, for exhaustiveness.
    fn space(&self, ty: Type<'db>) -> Space {
        self.space_within(ty, &mut Vec::new())
    }

    /// [`space`](Self::space), inside the structs in `open`. A struct inside itself, an error
    /// the class reports, is left open rather than taken apart forever.
    pub(super) fn space_within(&self, ty: Type<'db>, open: &mut Vec<Type<'db>>) -> Space {
        match ty.kind(self.db) {
            TypeKind::Union(members) => self.union_space(members, open),
            TypeKind::Struct(_) if open.contains(&ty) => Space::Open,
            TypeKind::Struct(nominal) => {
                open.push(ty);
                let space = self.struct_space(ty, nominal, open);
                open.pop();
                space
            }
            TypeKind::Primitive(Primitive::Bool) => Space::Bool,
            TypeKind::Optional(inner) => Space::Optional(Box::new(self.space_within(*inner, open))),
            TypeKind::Tuple(items) => Space::Tuple(
                items
                    .iter()
                    .map(|&item| self.space_within(item, open))
                    .collect(),
            ),
            TypeKind::Enum(nominal) => {
                let item = Item::new(self.db, self.lowerer.package, nominal.file, nominal.name);
                let Declaration::Enum { variants } = &self.db.declaration(item).declaration else {
                    return Space::Open;
                };
                let class = self
                    .db
                    .parse(nominal.file)
                    .tree()
                    .text(nominal.name, nominal.file.source(self.db).text());
                Space::Enum(
                    variants
                        .iter()
                        .map(|variant| format!("{class}.{variant}"))
                        .collect(),
                )
            }
            _ => Space::Open,
        }
    }
}
