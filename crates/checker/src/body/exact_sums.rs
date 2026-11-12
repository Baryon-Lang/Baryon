//! `math.fsum(values)`, the exactly rounded sum of a list or a rank-1 array of `f64`s (roadmap
//! 1.13.16).
//!
//! `fsum` is written in Baryon, in the standard library's `<std>/math.byn` (decision 0038), as
//! two private functions: `_fsum_list` for a list and `_fsum_array` for a rank-1 array. A call is
//! checked here as a call of the one that fits its argument, so it is lowered as any call of a
//! function is. The argument is checked first, since its type picks the function; a list
//! display is read as a `list[f64]`. Python's `fsum` takes any iterable of numbers; this version
//! takes `f64` elements only, and every other argument is an error that says what to pass
//! instead.

use super::Body;
use super::calls::Arguments;
use super::exclusive::Passed;
use syntax::{NodeId, SyntaxKind};
use types::{Callee, Declaration, Float, Item, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// A call of `math.fsum`, or of `fsum` imported from `math`, with `arguments`.
    pub(super) fn fsum(&mut self, node: NodeId, arguments: &Arguments) -> Type<'db> {
        if !arguments.keywords.is_empty() {
            self.untyped(arguments);
            return self.lowerer.error(
                node,
                "`fsum` takes its values by position, as Python's does",
                "a keyword argument",
                Some("pass the values to add by position, such as `fsum(energies)`"),
            );
        }
        let [values] = *arguments.positional.as_slice() else {
            self.untyped(arguments);
            let found = arguments.positional.len();
            let message = format!(
                "`fsum` takes one list or array, found {found} argument{}",
                if found == 1 { "" } else { "s" }
            );
            return self.lowerer.error(
                node,
                &message,
                "wrong number of arguments",
                Some("pass the values to add, such as `fsum(energies)`"),
            );
        };
        // A list display is read as a `list[f64]`, so `fsum([])` has a type and the integer
        // literals of `fsum([1, 2.5])` are floats, as Python's `fsum` converts them.
        let expected = (self.tree.kind(values) == SyntaxKind::List).then(|| {
            let float = Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64)));
            Type::new(self.db, TypeKind::List(float))
        });
        let found = self.expression(values, expected);
        let f64_elements = |element: Type<'db>| {
            matches!(
                element.kind(self.db),
                TypeKind::Primitive(Primitive::Float(Float::F64))
            )
        };
        let (function, hint) = match found.kind(self.db) {
            TypeKind::Unknown => return found,
            &TypeKind::List(element) if f64_elements(element) => ("_fsum_list", ""),
            &TypeKind::Array { element, rank: 1 } if f64_elements(element) => ("_fsum_array", ""),
            TypeKind::Array { rank: 1, .. } | TypeKind::List(_) => (
                "",
                "convert the elements to `f64` first, such as `[f64(n) for n in counts]`",
            ),
            TypeKind::Array { .. } => ("", "add one row at a time, such as `fsum(grid[i, :])`"),
            _ => ("", "pass a `list[f64]` or an `Array[f64, 1]`"),
        };
        if !hint.is_empty() {
            let message = format!(
                "`fsum` adds a list or rank-1 array of `f64`s, found `{}`",
                self.show(found)
            );
            return self
                .lowerer
                .error(values, &message, "cannot be added exactly", Some(hint));
        }
        let Some((file, name)) = self.library_function("math", function) else {
            return self.lowerer.error(
                node,
                "`fsum` is missing from the standard library",
                "cannot be typed",
                Some("the compiler adds the library's files to every package (decision 0038)"),
            );
        };
        let item = Item::new(self.db, self.lowerer.package, file, name);
        let Declaration::Function(signature) = self.db.declaration(item).declaration.clone() else {
            return self.lowerer.unknown();
        };
        let Some(first) = signature.parameters.first() else {
            return self.lowerer.unknown();
        };
        let receiver = Passed {
            node: values,
            mode: first.mode,
            ty: found,
        };
        self.check_call(
            node,
            "fsum",
            &signature,
            Some(receiver),
            Callee::Function(item),
            &Arguments {
                positional: Vec::new(),
                keywords: Vec::new(),
            },
        )
    }
}
