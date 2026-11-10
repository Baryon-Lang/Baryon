//! Exhaustiveness: whether the cases of a `match` cover every value of its subject, and if not,
//! an example of a value none of them covers.
//!
//! This is the usefulness algorithm of Maranget, "Warnings for pattern matching" (2007), cut
//! down to the patterns version 0 has. The cases form a matrix with one row per case and one
//! column per value still to match, starting with one column for the subject. The algorithm
//! looks for a value that no row matches, one column at a time:
//!
//! - When the column's type has finitely many shapes (an enum's members, `True` and `False`,
//!   `None` or a value for `T | None`, a union's members, or the one shape of a tuple or a
//!   struct), it tries each shape in turn,
//!   keeping the rows that match it and replacing the column by the shape's parts.
//! - Otherwise (numbers and strings) only a wildcard covers the column, so it keeps the rows
//!   with a wildcard there and drops the column.
//!
//! A case with an `if` guard may not match, so it is left out of the matrix. The cost grows with
//! the product of the shapes of a tuple's parts, which is small for the tuples of enums and
//! `bool`s that real `match` statements take apart.

/// What a column's values can look like, for exhaustiveness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Space {
    /// An enum, with each member's name as a program writes it, such as `Boundary.PERIODIC`.
    Enum(Vec<String>),
    /// `bool`.
    Bool,
    /// `T | None`, with the space of `T`.
    Optional(Box<Space>),
    /// A tuple, with the space of each item.
    Tuple(Vec<Space>),
    /// A union of structs, enums and `None` (decision 0032), with the space of each member in
    /// the union's order.
    Union(Vec<Space>),
    /// A struct, taken apart by a class pattern: its name as a program writes it, and the space
    /// of each field in declaration order. `None` as a union member is a struct space with no
    /// name, since `case None:` covers it whole.
    Struct {
        /// The struct's name, such as `Dirichlet`, or empty for `None`.
        name: String,
        /// The space of each field.
        fields: Vec<Space>,
    },
    /// Numbers, strings and everything else that only a wildcard covers.
    Open,
}

/// A case's pattern, reduced to what exhaustiveness needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Pattern {
    /// `_`, a capture such as `x`, or a pattern with an error already reported.
    Wild,
    /// An enum member, by its index in the enum's members.
    Member(usize),
    /// `True` or `False`.
    Bool(bool),
    /// `None`.
    None,
    /// A tuple pattern, one pattern per item.
    Tuple(Vec<Pattern>),
    /// A pattern that matches one member of a union, by the member's position, and what it
    /// matches of that member.
    Variant(usize, Box<Pattern>),
    /// A class pattern, such as `Dirichlet(value=v)`: one pattern per field in declaration
    /// order, a wildcard for each field it does not name.
    Class(Vec<Pattern>),
    /// A number or string literal, which covers one value of an open space.
    Literal,
    /// `a | b`: each alternative.
    Or(Vec<Pattern>),
}

/// One of the finitely many shapes of a value in a space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// The enum member with this index.
    Member(usize),
    /// `True` or `False`.
    Bool(bool),
    /// `None`, in an optional space.
    None,
    /// A value of the optional's type.
    Some,
    /// A tuple.
    Tuple,
    /// The union member with this position.
    Variant(usize),
    /// A struct.
    Struct,
}

/// An example of a value of `space` that no row of `rows` matches, written as a program
/// writes it, or `None` when the rows cover every value.
pub(crate) fn missing(rows: &[Pattern], space: &Space) -> Option<String> {
    let rows: Vec<Vec<Pattern>> = rows.iter().map(|row| vec![row.clone()]).collect();
    uncovered(&rows, std::slice::from_ref(space)).map(|mut witness| witness.remove(0))
}

/// The core of [`missing`]: an example, one entry per column, of values in `spaces` that no row
/// of `rows` matches.
fn uncovered(rows: &[Vec<Pattern>], spaces: &[Space]) -> Option<Vec<String>> {
    let Some((space, rest)) = spaces.split_first() else {
        return rows.is_empty().then(Vec::new);
    };
    let rows = expand_alternatives(rows);
    let Some(shapes) = shapes(space) else {
        let defaults: Vec<Vec<Pattern>> = rows
            .iter()
            .filter(|row| row[0] == Pattern::Wild)
            .map(|row| row[1..].to_vec())
            .collect();
        let mut witness = uncovered(&defaults, rest)?;
        witness.insert(0, "_".to_owned());
        return Some(witness);
    };
    for shape in shapes {
        let parts = parts(space, shape);
        let specialized: Vec<Vec<Pattern>> = rows
            .iter()
            .filter_map(|row| specialize(row, shape, parts.len()))
            .collect();
        let columns: Vec<Space> = parts.iter().chain(rest).cloned().collect();
        // A struct no case takes apart is missing whole: `Robin()`, not `Robin(True)`.
        let witness = if specialized.is_empty() && shape == Shape::Struct {
            uncovered(&[], rest).map(|tail| {
                let mut witness = vec!["_".to_owned(); parts.len()];
                witness.extend(tail);
                witness
            })
        } else {
            uncovered(&specialized, &columns)
        };
        if let Some(mut witness) = witness {
            let tail = witness.split_off(parts.len());
            let mut result = vec![render(space, shape, witness)];
            result.extend(tail);
            return Some(result);
        }
    }
    None
}

/// `rows` with every row whose first pattern is `a | b` replaced by one row per alternative.
fn expand_alternatives(rows: &[Vec<Pattern>]) -> Vec<Vec<Pattern>> {
    let mut expanded = Vec::with_capacity(rows.len());
    for row in rows {
        match &row[0] {
            Pattern::Or(alternatives) => {
                let alternatives: Vec<Vec<Pattern>> = alternatives
                    .iter()
                    .map(|alternative| {
                        let mut row = row.clone();
                        row[0] = alternative.clone();
                        row
                    })
                    .collect();
                expanded.extend(expand_alternatives(&alternatives));
            }
            _ => expanded.push(row.clone()),
        }
    }
    expanded
}

/// The shapes a value of `space` can take, or `None` when they are too many to list.
fn shapes(space: &Space) -> Option<Vec<Shape>> {
    match space {
        Space::Enum(members) => Some((0..members.len()).map(Shape::Member).collect()),
        Space::Bool => Some(vec![Shape::Bool(true), Shape::Bool(false)]),
        Space::Optional(_) => Some(vec![Shape::None, Shape::Some]),
        Space::Tuple(_) => Some(vec![Shape::Tuple]),
        Space::Union(members) => Some((0..members.len()).map(Shape::Variant).collect()),
        Space::Struct { .. } => Some(vec![Shape::Struct]),
        Space::Open => None,
    }
}

/// The spaces of the parts of a value of `space` with the shape `shape`.
fn parts(space: &Space, shape: Shape) -> Vec<Space> {
    match (space, shape) {
        (Space::Optional(value), Shape::Some) => vec![(**value).clone()],
        (Space::Tuple(items), Shape::Tuple) => items.clone(),
        (Space::Union(members), Shape::Variant(index)) => vec![members[index].clone()],
        (Space::Struct { fields, .. }, Shape::Struct) => fields.clone(),
        _ => Vec::new(),
    }
}

/// The row that `row` becomes when its first value has the shape `shape` with `arity` parts,
/// or `None` when the row's first pattern cannot match that shape.
fn specialize(row: &[Pattern], shape: Shape, arity: usize) -> Option<Vec<Pattern>> {
    let mut first = match (&row[0], shape) {
        (Pattern::Wild, _) => vec![Pattern::Wild; arity],
        (Pattern::Member(a), Shape::Member(b)) if *a == b => Vec::new(),
        (Pattern::Bool(a), Shape::Bool(b)) if *a == b => Vec::new(),
        (Pattern::None, Shape::None) => Vec::new(),
        (Pattern::Tuple(items), Shape::Tuple) if items.len() == arity => items.clone(),
        (Pattern::Variant(a, inner), Shape::Variant(b)) if *a == b => vec![(**inner).clone()],
        (Pattern::Class(fields), Shape::Struct) if fields.len() == arity => fields.clone(),
        // Any pattern but `None` on a `T | None` matches a value of `T`.
        (Pattern::None, Shape::Some) => return None,
        (pattern, Shape::Some) => vec![pattern.clone()],
        _ => return None,
    };
    first.extend_from_slice(&row[1..]);
    Some(first)
}

/// A value of `space` with the shape `shape` and the parts `parts`, as a program writes it.
fn render(space: &Space, shape: Shape, mut parts: Vec<String>) -> String {
    match (space, shape) {
        (Space::Enum(members), Shape::Member(index)) => members[index].clone(),
        (_, Shape::Bool(value)) => if value { "True" } else { "False" }.to_owned(),
        (_, Shape::None) => "None".to_owned(),
        (_, Shape::Some) => parts.pop().unwrap_or_else(|| "_".to_owned()),
        (_, Shape::Variant(_)) => parts.pop().unwrap_or_else(|| "_".to_owned()),
        (Space::Struct { name, .. }, Shape::Struct) if name.is_empty() => "None".to_owned(),
        (Space::Struct { name, .. }, Shape::Struct) => {
            if parts.iter().all(|part| part == "_") {
                format!("{name}()")
            } else {
                format!("{name}({})", parts.join(", "))
            }
        }
        (_, Shape::Tuple) if parts.len() == 1 => format!("({},)", parts[0]),
        _ => format!("({})", parts.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The space of an enum with three members.
    fn boundary() -> Space {
        Space::Enum(vec![
            "Boundary.PERIODIC".to_owned(),
            "Boundary.REFLECTING".to_owned(),
            "Boundary.ABSORBING".to_owned(),
        ])
    }

    #[test]
    fn enums_need_every_member_or_a_wildcard() {
        let all = [Pattern::Member(0), Pattern::Member(1), Pattern::Member(2)];
        assert_eq!(missing(&all, &boundary()), None);
        let two = [Pattern::Member(0), Pattern::Member(2)];
        assert_eq!(
            missing(&two, &boundary()).as_deref(),
            Some("Boundary.REFLECTING")
        );
        let either = [
            Pattern::Or(vec![Pattern::Member(0), Pattern::Member(1)]),
            Pattern::Wild,
        ];
        assert_eq!(missing(&either, &boundary()), None);
    }

    #[test]
    fn numbers_need_a_wildcard() {
        assert_eq!(
            missing(&[Pattern::Literal], &Space::Open).as_deref(),
            Some("_")
        );
        assert_eq!(
            missing(&[Pattern::Literal, Pattern::Wild], &Space::Open),
            None
        );
    }

    #[test]
    fn unions_need_every_member() {
        let neumann = Space::Struct {
            name: "Neumann".to_owned(),
            fields: vec![Space::Open],
        };
        let mode = Space::Struct {
            name: "Robin".to_owned(),
            fields: vec![Space::Bool],
        };
        let union = Space::Union(vec![neumann, mode]);
        let class = |index, field| Pattern::Variant(index, Box::new(Pattern::Class(vec![field])));
        let rows = [class(0, Pattern::Wild)];
        assert_eq!(missing(&rows, &union).as_deref(), Some("Robin()"));
        let rows = [class(0, Pattern::Wild), class(1, Pattern::Bool(true))];
        assert_eq!(missing(&rows, &union).as_deref(), Some("Robin(False)"));
        let rows = [class(1, Pattern::Wild), Pattern::Wild];
        assert_eq!(missing(&rows, &union), None);
    }

    #[test]
    fn optionals_and_tuples_are_taken_apart() {
        let optional = Space::Optional(Box::new(Space::Bool));
        let rows = [Pattern::None, Pattern::Bool(true)];
        assert_eq!(missing(&rows, &optional).as_deref(), Some("False"));
        let pair = Space::Tuple(vec![Space::Bool, boundary()]);
        let rows = [
            Pattern::Tuple(vec![Pattern::Bool(true), Pattern::Wild]),
            Pattern::Tuple(vec![Pattern::Bool(false), Pattern::Member(0)]),
        ];
        assert_eq!(
            missing(&rows, &pair).as_deref(),
            Some("(False, Boundary.REFLECTING)")
        );
        assert_eq!(
            missing(&[], &Space::Tuple(vec![Space::Open])).as_deref(),
            Some("(_,)")
        );
    }
}
