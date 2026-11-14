//! Taking the missing values out (decision 0045, part 3, roadmap 3.13.6): `dropna()` and
//! `fillna(value)` of an array with missing values and of a frame.
//!
//! Both narrow the type, which is how the program proves a column is complete: the result of
//! `values.dropna()` or `values.fillna(0.0)` is an `Array[T, 1]`, and a frame's column that
//! held `T | None` holds `T` afterwards, so what follows reads numbers with no check for
//! `None`.

use super::calls::Arguments;
use super::{Body, relations};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Column, Type, TypeKind};

impl<'db> Body<'db> {
    /// `values.dropna()` or `values.fillna(value)` on an array with missing values whose
    /// elements are `element`s, the call `node` of the method `name`: an `Array[element, 1]`
    /// of the elements that are there, or of every element with `value` where one was
    /// `None`. `None`, with nothing checked, for any other method.
    pub(super) fn optional_array_narrowed(
        &mut self,
        node: NodeId,
        element: Type<'db>,
        name: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        // Text with no gaps is a text array, the type of `Array[str, 1]`.
        let narrowed = match element.kind(self.db) {
            TypeKind::Str => Type::new(self.db, TypeKind::TextArray),
            _ => Type::new(self.db, TypeKind::Array { element, rank: 1 }),
        };
        let given = (
            arguments.positional.as_slice(),
            arguments.keywords.as_slice(),
        );
        match (name, given) {
            ("dropna", ([], [])) => Some(narrowed),
            ("fillna", _) => {
                let value = match self.fill_value(node, arguments) {
                    Ok(value) => value,
                    Err(unknown) => return Some(unknown),
                };
                let found = self.expression(value, Some(element));
                self.expect(value, element, found, "the array holds");
                Some(narrowed)
            }
            ("dropna", _) => {
                self.untyped(arguments);
                Some(self.lowerer.error(
                    node,
                    "`dropna` of an array takes no arguments",
                    "wrong arguments",
                    Some("`values.dropna()` gives the elements that are not `None`"),
                ))
            }
            _ => None,
        }
    }

    /// The one argument of `fillna`, given by position or as `value=`. The unknown type
    /// after reporting any other arguments at `node`.
    fn fill_value(&mut self, node: NodeId, arguments: &Arguments) -> Result<NodeId, Type<'db>> {
        match (
            arguments.positional.as_slice(),
            arguments.keywords.as_slice(),
        ) {
            (&[value], []) => Ok(value),
            ([], &[(name, value)]) if self.tree.text(name, self.text) == "value" => Ok(value),
            _ => {
                self.untyped(arguments);
                Err(self.lowerer.error(
                    node,
                    "`fillna` takes the value that replaces each `None`",
                    "wrong arguments",
                    Some("for example `values.fillna(0.0)`"),
                ))
            }
        }
    }

    /// `df.dropna()` or `df.fillna(value)` on a frame of type `base`, the call `function` of
    /// the method `callee`. `dropna()` gives the rows with no `None` in any column, and
    /// `fillna` every row with the value in place of each `None`; in both, a column that
    /// held `T | None` holds `T` in the result. `fillna` takes one value for every column
    /// that may hold `None`, or a dict from a column's name to its value, which fills the
    /// columns it names and leaves the others as they are. A frame with no column to narrow
    /// gives a copy of its own type. `None`, with nothing checked, for any other method.
    pub(super) fn missing_method(
        &mut self,
        function: NodeId,
        base: Type<'db>,
        callee: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let TypeKind::DataFrame { columns, .. } = base.kind(self.db) else {
            return None;
        };
        let given = arguments.positional.len() + arguments.keywords.len();
        let filled: Vec<bool> = match (callee, given) {
            ("dropna", 0) => vec![true; columns.len()],
            ("dropna", _) => {
                self.untyped(arguments);
                return Some(self.lowerer.error(
                    function,
                    "`dropna` of a `DataFrame` takes no arguments yet",
                    "wrong arguments",
                    Some(
                        "`df.dropna()` drops each row with a `None` in any column; to look at \
                         some columns only, take them first: `df[[\"bid\", \"ask\"]].dropna()`",
                    ),
                ));
            }
            ("fillna", _) => {
                let value = match self.fill_value(function, arguments) {
                    Ok(value) => value,
                    Err(unknown) => return Some(unknown),
                };
                match self.filled_columns(value, base, columns) {
                    Some(filled) => filled,
                    None => return Some(Type::new(self.db, TypeKind::Unknown)),
                }
            }
            _ => return None,
        };
        let narrowed: Vec<Column<'db>> = columns
            .iter()
            .zip(filled)
            .map(|(column, filled)| match column.element.kind(self.db) {
                &TypeKind::Optional(element) if filled => Column {
                    name: column.name.clone(),
                    element,
                },
                _ => column.clone(),
            })
            .collect();
        if &narrowed == columns {
            return Some(base);
        }
        Some(self.frame_of_columns(narrowed))
    }

    /// Which of `columns`, those of the frame of type `base`, `fillna(value)` fills, after
    /// checking each value against the column it fills. One value fills every column that
    /// may hold `None`, so it must have the type of each. A dict fills the columns its keys
    /// name, each of which must be one that may hold `None`. `None` after an error in a
    /// dict's keys.
    fn filled_columns(
        &mut self,
        value: NodeId,
        base: Type<'db>,
        columns: &[Column<'db>],
    ) -> Option<Vec<bool>> {
        let optional = |body: &Self, column: &Column<'db>| match column.element.kind(body.db) {
            &TypeKind::Optional(element) => Some(element),
            _ => None,
        };
        if self.tree.kind(value) != SyntaxKind::Dictionary {
            let elements: Vec<Type<'db>> = columns
                .iter()
                .filter_map(|column| optional(self, column))
                .collect();
            let found = self.expression(value, elements.first().copied());
            for element in elements {
                if relations::compatible(self.db, element, found)
                    || self.unchecked(value, element, found)
                {
                    continue;
                }
                let message = format!(
                    "expected `{}`, found `{}`",
                    self.show(element),
                    self.show(found)
                );
                let label = format!("a column holds `{}`", self.show(element));
                self.lowerer.error(
                    value,
                    &message,
                    &label,
                    Some(
                        "one value fills every column that may hold `None`; give each column \
                         its own with a dict, `df.fillna({\"bid\": 0.0, \"size\": 0})`",
                    ),
                );
            }
            return Some(vec![true; columns.len()]);
        }
        let mut filled = vec![false; columns.len()];
        let mut complete = true;
        let entries: Vec<NodeId> = self.tree.children_with(value, Role::Item).collect();
        for entry in entries {
            let key = self.tree.child(entry, Role::Key);
            let given = self.tree.child(entry, Role::Value);
            let (Some(key), Some(given)) = (key, given) else {
                self.lowerer.error(
                    entry,
                    "`**` cannot give the values `fillna` fills with",
                    "unpacks a dict",
                    None,
                );
                complete = false;
                continue;
            };
            let position = self
                .column_name(key, base)
                .and_then(|name| columns.iter().position(|column| column.name == name));
            let element = position.and_then(|position| optional(self, &columns[position]));
            let found = self.expression(given, element);
            match (position, element) {
                (Some(position), Some(element)) => {
                    self.expect(given, element, found, "the column holds");
                    filled[position] = true;
                }
                (Some(position), None) => {
                    let name = &columns[position].name;
                    let message = format!("the column `{name}` cannot hold `None`");
                    self.lowerer
                        .error(key, &message, "has nothing to fill", None);
                    complete = false;
                }
                (None, _) => complete = false,
            }
        }
        complete.then_some(filled)
    }
}
