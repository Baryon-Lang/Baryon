//! `NumPy`'s `.npy` files (decision 0043): `Array.load(path)` and `Array.save(path, a)`, with
//! `NumPy`'s names and argument order.
//!
//! Both can fail with `OSError`, so each has the type `Raises[T, OSError]`. `load` gives an
//! `Array[T, rank]` whose element type and rank come from the context, as an element type does
//! for `Array.zeros`, else `Array[f64, 1]`; the runtime checks the file's header against them.
//! A `.npy` file names its element type, so the elements are the types `NumPy` names the same
//! way: numbers, `bool` and complex numbers. Structs need `NumPy`'s structured types, which are
//! roadmap 3.25's.

use super::Body;
use super::calls::Arguments;
use syntax::NodeId;
use types::{Call, Callee, Float, Key, Primitive, Type, TypeKind};

/// `NumPy`'s keywords of `load` and `save` that do not apply: memory maps, pickled objects and
/// Python 2's files.
const NOT_TAKEN: [&str; 5] = [
    "mmap_mode",
    "allow_pickle",
    "fix_imports",
    "encoding",
    "max_header_size",
];

impl<'db> Body<'db> {
    /// `Array.load(path)`. `expected` is the type the context needs, which gives the element
    /// type and the rank.
    pub(super) fn array_load(
        &mut self,
        node: NodeId,
        arguments: &Arguments,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        let Some([path]) = self.npy_arguments(node, arguments, &["file"], "Array.load") else {
            return self.lowerer.unknown();
        };
        let str = Type::new(self.db, TypeKind::Str);
        let found = self.expression(path, Some(str));
        self.expect(path, str, found, "a path is");
        let array = match expected.map(|ty| (ty, ty.kind(self.db))) {
            Some((ty, TypeKind::Array { element, .. })) if self.is_npy_element(*element) => ty,
            Some((ty, TypeKind::Array { .. })) => {
                let message = format!(
                    "`Array.load` cannot read a `.npy` file into an `{}`",
                    self.show(ty)
                );
                return self.lowerer.error(
                    node,
                    &message,
                    "not a number type",
                    Some(
                        "a `.npy` file holds numbers, `bool`s or complex numbers; records are \
                         roadmap 3.25's",
                    ),
                );
            }
            _ => {
                let element = Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64)));
                Type::new(self.db, TypeKind::Array { element, rank: 1 })
            }
        };
        self.record_npy(node, "Array.load", vec![path]);
        self.raises_library_error(node, array, "OSError")
    }

    /// `Array.save(path, a)`: any `Array` of numbers, `bool`s or complex numbers, a view
    /// included.
    pub(super) fn array_save(&mut self, node: NodeId, arguments: &Arguments) -> Type<'db> {
        let Some([path, array]) =
            self.npy_arguments(node, arguments, &["file", "arr"], "Array.save")
        else {
            return self.lowerer.unknown();
        };
        let str = Type::new(self.db, TypeKind::Str);
        let found = self.expression(path, Some(str));
        self.expect(path, str, found, "a path is");
        let found = self.expression(array, None);
        match found.kind(self.db) {
            TypeKind::Array { element, .. } if self.is_npy_element(*element) => {}
            TypeKind::Unknown => return self.lowerer.unknown(),
            kind => {
                let hint = if matches!(kind, TypeKind::Array { .. }) {
                    "a `.npy` file names its element type as `NumPy` does; records are roadmap \
                     3.25's"
                } else {
                    "make an array first, such as `Array(values)` of a list"
                };
                let message = format!(
                    "`Array.save` writes an `Array` of numbers, `bool`s or complex numbers, \
                     found `{}`",
                    self.show(found)
                );
                return self
                    .lowerer
                    .error(array, &message, "cannot be saved", Some(hint));
            }
        }
        self.record_npy(node, "Array.save", vec![path, array]);
        let nothing = Type::new(self.db, TypeKind::None);
        self.raises_library_error(node, nothing, "OSError")
    }

    /// The arguments of `callee`, named `names` as `NumPy` names them, each by position or by
    /// keyword; `None`, with the error reported, when one is missing, given twice or unknown.
    fn npy_arguments<const N: usize>(
        &mut self,
        node: NodeId,
        arguments: &Arguments,
        names: &[&str; N],
        callee: &str,
    ) -> Option<[NodeId; N]> {
        let mut slots: [Option<NodeId>; N] = [None; N];
        let mut wrong = arguments.positional.len() > N;
        for (slot, &argument) in slots.iter_mut().zip(&arguments.positional) {
            *slot = Some(argument);
        }
        let mut not_taken = None;
        for &(name, value) in &arguments.keywords {
            let written = self.tree.text(name, self.text);
            if let Some(index) = names.iter().position(|&known| known == written) {
                wrong |= slots[index].replace(value).is_some();
            } else {
                if NOT_TAKEN.contains(&written) {
                    not_taken = Some((name, written.to_owned()));
                }
                wrong = true;
            }
        }
        let given = slots.iter().copied().collect::<Option<Vec<NodeId>>>();
        if let (Some(given), false) = (&given, wrong) {
            return given.clone().try_into().ok();
        }
        self.untyped(arguments);
        if let Some((name, written)) = not_taken {
            let message = format!("`{callee}` does not take `{written}`");
            self.lowerer.error(
                name,
                &message,
                "not taken",
                Some("a `.npy` file holds numbers, read straight into the array"),
            );
        } else {
            let message = format!("`{callee}` takes {}", list(names));
            let hint = if N == 1 {
                "for example `Array.load(\"field.npy\")`"
            } else {
                "for example `Array.save(\"field.npy\", field)`"
            };
            self.lowerer
                .error(node, &message, "wrong arguments", Some(hint));
        }
        None
    }

    /// Whether a `.npy` file can hold elements of type `ty`: a number, a `bool` or a complex
    /// number, each of which `NumPy` names with one type code.
    fn is_npy_element(&self, ty: Type<'db>) -> bool {
        matches!(ty.kind(self.db), TypeKind::Primitive(_))
    }

    /// Records the call `node` of the built-in `name` with `arguments`.
    fn record_npy(&mut self, node: NodeId, name: &str, arguments: Vec<NodeId>) {
        let call = Call {
            callee: Callee::Builtin(name.to_owned()),
            type_arguments: Vec::new(),
            arguments: arguments
                .into_iter()
                .map(|argument| Some(Key(argument)))
                .collect(),
        };
        self.typed.calls.insert(Key(node), call);
    }
}

/// `names` as a message lists them: "`file`" or "`file` and `arr`".
fn list(names: &[&str]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    match quoted.as_slice() {
        [last] => last.clone(),
        [first @ .., last] => format!("{} and {last}", first.join(", ")),
        [] => String::new(),
    }
}
