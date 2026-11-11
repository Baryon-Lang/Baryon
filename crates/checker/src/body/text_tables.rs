//! Text tables of numbers (decision 0043): `Array.loadtxt(fname)` and `Array.savetxt(fname, X)`,
//! with `NumPy`'s names.
//!
//! Both are written in Baryon, in the standard library's `<std>/arrays.byn` (decision 0038), so
//! each call here is checked as a call of the library function that fits, and lowered as any
//! call of a function is. `loadtxt` picks its function by the rank the context needs: a table,
//! `Array[f64, 2]`, unless the context needs a rank-1 array. `savetxt` picks its function by
//! the rank of the array it writes. `NumPy` has the array second, but each library function takes
//! it first, so the array is checked here and passed as the call's receiver.
//!
//! The first version reads and writes `f64` only, and every keyword `NumPy` has that is not
//! built yet is an error that names it, never silently ignored.

use super::Body;
use super::calls::Arguments;
use super::exclusive::Passed;
use syntax::NodeId;
use types::{Callee, Declaration, Float, Item, Primitive, Type, TypeKind};

/// `NumPy`'s keywords of `loadtxt` that the first version does not take.
const LOADTXT_LATER: [&str; 7] = [
    "converters",
    "usecols",
    "unpack",
    "ndmin",
    "encoding",
    "quotechar",
    "like",
];

/// `NumPy`'s keywords of `savetxt` that the first version does not take.
const SAVETXT_LATER: [&str; 2] = ["fmt", "encoding"];

impl<'db> Body<'db> {
    /// `Array.loadtxt(fname)`, with an optional `dtype` by position or keyword and the rest of
    /// its arguments by keyword. `expected` is the type the context needs, which picks the rank.
    pub(super) fn array_loadtxt(
        &mut self,
        node: NodeId,
        arguments: &Arguments,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        let mut dtype = arguments.positional.get(1).copied();
        let mut keywords = Vec::new();
        for &(name, value) in &arguments.keywords {
            if self.tree.text(name, self.text) == "dtype" && dtype.is_none() {
                dtype = Some(value);
            } else {
                keywords.push((name, value));
            }
        }
        if arguments.positional.len() > 2 {
            return self.table_error(
                node,
                arguments,
                (
                    "`Array.loadtxt` takes `fname` and `dtype` by position",
                    "too many by position",
                ),
                "pass the rest by keyword, such as `delimiter=\",\"`",
            );
        }
        if let Some(dtype) = dtype {
            let element = self.lowerer.lower(dtype);
            if !self.is_f64(element) {
                return self.table_error(
                    dtype,
                    arguments,
                    (
                        "`Array.loadtxt` reads `f64` numbers only in this version",
                        "not built yet",
                    ),
                    "leave out `dtype`, or write `dtype=f64`, and convert the array after",
                );
            }
        }
        if let Some(problem) = self.later_keyword(&keywords, &LOADTXT_LATER, "Array.loadtxt") {
            return self.table_error(
                problem.0,
                arguments,
                (&problem.1, "not built yet"),
                &problem.2,
            );
        }
        let vector = matches!(
            expected.map(|ty| ty.kind(self.db)),
            Some(TypeKind::Array { rank: 1, .. })
        );
        let function = if vector { "loadtxt_vector" } else { "loadtxt" };
        let Some((file, name)) = self.library_function("arrays", function) else {
            return self.table_error(
                node,
                arguments,
                (
                    "`Array.loadtxt` is missing from the standard library",
                    "cannot be typed",
                ),
                "the compiler adds the library's files to every package (decision 0038)",
            );
        };
        let given = Arguments {
            positional: arguments.positional.iter().copied().take(1).collect(),
            keywords,
        };
        self.call_item(node, "Array.loadtxt", file, name, &given)
    }

    /// `Array.savetxt(fname, X)`, with `X` by position or keyword and the rest of its
    /// arguments by keyword. `X` is checked first, since its rank picks the function.
    pub(super) fn array_savetxt(&mut self, node: NodeId, arguments: &Arguments) -> Type<'db> {
        let mut array = arguments.positional.get(1).copied();
        let mut keywords = Vec::new();
        for &(name, value) in &arguments.keywords {
            if self.tree.text(name, self.text) == "X" && array.is_none() {
                array = Some(value);
            } else {
                keywords.push((name, value));
            }
        }
        if arguments.positional.len() > 2 {
            return self.table_error(
                node,
                arguments,
                (
                    "`Array.savetxt` takes `fname` and `X` by position",
                    "too many by position",
                ),
                "pass the rest by keyword, such as `delimiter=\",\"`",
            );
        }
        let Some(array) = array else {
            return self.table_error(
                node,
                arguments,
                ("`Array.savetxt` needs the array to write", "no array"),
                "for example `Array.savetxt(\"out.txt\", table)`",
            );
        };
        if let Some(problem) = self.later_keyword(&keywords, &SAVETXT_LATER, "Array.savetxt") {
            return self.table_error(
                problem.0,
                arguments,
                (&problem.1, "not built yet"),
                &problem.2,
            );
        }
        let found = self.expression(array, None);
        let (function, hint) = match found.kind(self.db) {
            TypeKind::Unknown => return self.untyped_rest(arguments, array),
            TypeKind::Array { element, rank } if self.is_f64(*element) => match rank {
                1 => ("savetxt_vector", ""),
                2 => ("savetxt", ""),
                _ => (
                    "",
                    "a text table has rows and columns; write one slice at a time",
                ),
            },
            _ => (
                "",
                "convert the elements to `f64` first, such as `f64(n)` of each one",
            ),
        };
        if !hint.is_empty() {
            self.untyped_rest(arguments, array);
            let message = format!(
                "`Array.savetxt` writes an `Array[f64, 1]` or `Array[f64, 2]`, found `{}`",
                self.show(found)
            );
            return self
                .lowerer
                .error(array, &message, "cannot be written as a table", Some(hint));
        }
        let Some((file, name)) = self.library_function("arrays", function) else {
            self.untyped_rest(arguments, array);
            return self.lowerer.error(
                node,
                "`Array.savetxt` is missing from the standard library",
                "cannot be typed",
                Some("the compiler adds the library's files to every package (decision 0038)"),
            );
        };
        let item = Item::new(self.db, self.lowerer.package, file, name);
        let Declaration::Function(signature) = self.db.declaration(item).declaration.clone() else {
            return self.untyped_rest(arguments, array);
        };
        let Some(first) = signature.parameters.first() else {
            return self.untyped_rest(arguments, array);
        };
        let receiver = Passed {
            node: array,
            mode: first.mode,
            ty: found,
        };
        let given = Arguments {
            positional: arguments.positional.iter().copied().take(1).collect(),
            keywords,
        };
        self.check_call(
            node,
            "Array.savetxt",
            &signature,
            Some(receiver),
            Callee::Function(item),
            &given,
        )
    }

    /// Whether `ty` is `f64`, the one element type text tables read and write so far.
    fn is_f64(&self, ty: Type<'db>) -> bool {
        matches!(
            ty.kind(self.db),
            TypeKind::Primitive(Primitive::Float(Float::F64))
        )
    }

    /// The first of `keywords` that `NumPy`'s function `callee` has but this version does not
    /// take: its name node, the message and the hint.
    fn later_keyword(
        &self,
        keywords: &[(NodeId, NodeId)],
        later: &[&str],
        callee: &str,
    ) -> Option<(NodeId, String, String)> {
        keywords.iter().find_map(|&(name, _)| {
            let written = self.tree.text(name, self.text);
            later.contains(&written).then(|| {
                (
                    name,
                    format!("`{callee}` does not take `{written}` yet"),
                    "decision 0043 lists what the first version takes; until then, `open`, \
                     `str` methods and f-strings do it by hand"
                        .to_owned(),
                )
            })
        })
    }

    /// Types every argument with no expectation and reports `message` at `node`, with its
    /// `label` and `hint`.
    fn table_error(
        &mut self,
        node: NodeId,
        arguments: &Arguments,
        (message, label): (&str, &str),
        hint: &str,
    ) -> Type<'db> {
        self.untyped(arguments);
        self.lowerer.error(node, message, label, Some(hint))
    }

    /// Types every argument but `done`, which is typed already, with no expectation, and gives
    /// the call an unknown type.
    fn untyped_rest(&mut self, arguments: &Arguments, done: NodeId) -> Type<'db> {
        let rest = Arguments {
            positional: arguments
                .positional
                .iter()
                .copied()
                .filter(|&argument| argument != done)
                .collect(),
            keywords: arguments
                .keywords
                .iter()
                .copied()
                .filter(|&(_, value)| value != done)
                .collect(),
        };
        self.untyped(&rest)
    }
}
