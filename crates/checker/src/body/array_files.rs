//! Array files (decision 0036, part 4): `Array.fromfile(path, dtype, count, offset)` and
//! `a.tofile(path)`, binary data as whole arrays with `NumPy`'s names and `NumPy`'s bytes.
//!
//! Both can fail with `OSError`, so each has the type `Raises[T, OSError]` and its error is
//! handled as any call's is. `fromfile` gives an `Array[T, 1]`, where `T` is the `dtype`
//! argument, else the element type the context needs, else `f64`, as `NumPy`'s default is. A
//! file's bytes become the elements as they are, so `T` must be a type every pattern of bytes
//! is a value of: a number, a `bool`, a `Fixed` array of them or a `Copy` struct of them. An
//! enum is not, since a byte could name no member. `tofile` takes any array, since writing
//! needs no such promise.

use super::Body;
use super::calls::Arguments;
use crate::embed;
use crate::forms::is_copy;
use syntax::NodeId;
use types::{Call, Callee, Float, Integer, Key, Primitive, Type, TypeKind};

/// What `Array.fromfile` takes, for its messages.
const FROMFILE: &str = "for example `Array.fromfile(\"field.bin\", f64)` or \
                        `Array.fromfile(path, f32, count=1024, offset=8192)`";

impl<'db> Body<'db> {
    /// `Array.fromfile(path)`, with an optional `dtype`, `count` and `offset`, each positional
    /// in that order or by keyword. `expected` is the type the context needs. Records the call
    /// with the path, the count and the offset, the last two `None` where left out.
    pub(super) fn array_fromfile(
        &mut self,
        node: NodeId,
        arguments: &Arguments,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        let [path, mut dtype, mut count, mut offset] = {
            let mut positional = arguments.positional.iter().copied();
            [(); 4].map(|()| positional.next())
        };
        let mut wrong = path.is_none() || arguments.positional.len() > 4;
        let mut separator = false;
        for &(name, value) in &arguments.keywords {
            let slot = match self.tree.text(name, self.text) {
                "dtype" => &mut dtype,
                "count" => &mut count,
                "offset" => &mut offset,
                keyword => {
                    separator |= keyword == "sep";
                    wrong = true;
                    continue;
                }
            };
            wrong |= slot.replace(value).is_some();
        }
        let Some(path) = path.filter(|_| !wrong) else {
            self.untyped(arguments);
            let hint = if separator {
                "an array file is binary; read text with `open` and the `str` methods"
            } else {
                FROMFILE
            };
            return self.lowerer.error(
                node,
                "`Array.fromfile` takes a path and an optional `dtype`, `count` and `offset`",
                "wrong arguments",
                Some(hint),
            );
        };
        let str = Type::new(self.db, TypeKind::Str);
        let found = self.expression(path, Some(str));
        self.expect(path, str, found, "a path is");
        let int = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        for (argument, what) in [(count, "a count is"), (offset, "an offset is")] {
            if let Some(argument) = argument {
                let found = self.expression(argument, Some(int));
                self.expect(argument, int, found, what);
            }
        }
        let context = match expected.map(|ty| ty.kind(self.db)) {
            Some(TypeKind::Array { element, rank: 1 }) => Some(*element),
            _ => None,
        };
        let element = dtype
            .map(|dtype| self.lowerer.lower(dtype))
            .or(context)
            .unwrap_or_else(|| {
                Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64)))
            });
        if matches!(element.kind(self.db), TypeKind::Unknown) {
            return self.lowerer.unknown();
        }
        if !self.is_plain_data(element) {
            let message = format!(
                "`Array.fromfile` cannot read elements of type `{}`",
                self.show(element)
            );
            return self.lowerer.error(
                dtype.unwrap_or(node),
                &message,
                "not plain data",
                Some(
                    "a file's bytes become the elements as they are, so the element type is a \
                     number, a `bool`, a `Fixed` array of them or a `Copy` struct of them, \
                     written out",
                ),
            );
        }
        let call = Call {
            callee: Callee::Builtin("Array.fromfile".to_owned()),
            type_arguments: Vec::new(),
            arguments: [Some(path), count, offset]
                .into_iter()
                .map(|argument| argument.map(Key))
                .collect(),
        };
        self.typed.calls.insert(Key(node), call);
        let array = Type::new(self.db, TypeKind::Array { element, rank: 1 });
        self.raises_library_error(node, array, "OSError")
    }

    /// Whether every pattern of bytes is a value of `ty`, so a file can be read into it: a
    /// number, a `bool` (the runtime makes each byte 0 or 1), a `Fixed` array of such a type, or
    /// a `Copy` struct whose fields all are. An enum and a union are not, and a type parameter
    /// is not, since the type it stands for may not be.
    fn is_plain_data(&self, ty: Type<'db>) -> bool {
        match ty.kind(self.db) {
            TypeKind::Primitive(_) => true,
            TypeKind::Fixed { element, .. } => self.is_plain_data(*element),
            TypeKind::Struct(nominal) => {
                is_copy(self.db, ty)
                    && embed::fields(self.db, self.lowerer.package, nominal)
                        .into_iter()
                        .all(|(_, field)| self.is_plain_data(field))
            }
            _ => false,
        }
    }

    /// `a.tofile(path)` on an `Array`: one positional argument, the path.
    pub(super) fn array_tofile(&mut self, node: NodeId, arguments: &Arguments) -> Type<'db> {
        let ([path], []) = (
            arguments.positional.as_slice(),
            arguments.keywords.as_slice(),
        ) else {
            self.untyped(arguments);
            return self.lowerer.error(
                node,
                "`tofile` takes one path",
                "wrong arguments",
                Some(
                    "it writes the array's elements as binary, `a.tofile(\"field.bin\")`; write \
                     text with `open` and f-strings",
                ),
            );
        };
        let str = Type::new(self.db, TypeKind::Str);
        let found = self.expression(*path, Some(str));
        self.expect(*path, str, found, "a path is");
        let nothing = Type::new(self.db, TypeKind::None);
        self.raises_library_error(node, nothing, "OSError")
    }
}
