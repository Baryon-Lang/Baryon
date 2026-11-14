//! The methods of `str` (roadmap 1.13.12): `split`, `strip`, `lstrip`, `rstrip`, `join`,
//! `startswith`, `endswith`, `find`, `replace`, `lower`, `upper`, `copy` and `encode`
//! (decision 0064), each checked against its signature like any other call and recorded as the
//! built-in `str.<name>`.
//!
//! Each method reads its text and returns a new value, so the receiver is borrowed. `split` and
//! the three strips take their one argument or none, as in Python, where leaving it out means
//! "whitespace"; the call records the form it was written in, so no default is passed.

use super::Body;
use super::calls::Arguments;
use super::exclusive::Passed;
use super::methods::copy_signature;
use syntax::NodeId;
use types::{Callee, Integer, Mode, Parameter, Primitive, Signature, Type, TypeKind};

/// The methods of `str` a program can call, for the hint of an unknown one.
const METHODS: &str = "`split`, `strip`, `lstrip`, `rstrip`, `join`, `startswith`, `endswith`, \
                       `find`, `replace`, `lower`, `upper`, `encode` and `copy`";

impl<'db> Body<'db> {
    /// The call `text.name(arguments)` on a `str`, `function` being `text.name` and `receiver`
    /// the text. An unknown name is an error that lists the methods there are.
    pub(super) fn text_method(
        &mut self,
        node: NodeId,
        function: NodeId,
        receiver: Passed<'db>,
        name: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        let text = receiver.ty;
        let given = arguments.positional.len() + arguments.keywords.len();
        let int = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        let bool = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
        let words = Type::new(self.db, TypeKind::List(text));
        let (parameters, returns): (&[(&str, Type<'db>)], Type<'db>) = match name {
            "copy" => {
                let signature = copy_signature(text);
                let builtin = Callee::Builtin("str.copy".to_owned());
                return self.check_call(node, name, &signature, Some(receiver), builtin, arguments);
            }
            "encode" => return self.encode(node, receiver, arguments),
            "split" if given == 0 => (&[], words),
            "split" => (&[("sep", text)], words),
            "strip" | "lstrip" | "rstrip" if given == 0 => (&[], text),
            "strip" | "lstrip" | "rstrip" => (&[("chars", text)], text),
            "join" => (&[("iterable", words)], text),
            "startswith" => (&[("prefix", text)], bool),
            "endswith" => (&[("suffix", text)], bool),
            "find" => (&[("sub", text)], int),
            "replace" => (&[("old", text), ("new", text)], text),
            "lower" | "upper" => (&[], text),
            _ => {
                self.untyped(arguments);
                let message = format!("`str` has no method `{name}`");
                let hint = format!("a `str` has the methods {METHODS}");
                return self
                    .lowerer
                    .error(function, &message, "not defined on `str`", Some(&hint));
            }
        };
        let signature = Signature {
            type_parameters: Vec::new(),
            parameters: std::iter::once(("self", text))
                .chain(parameters.iter().copied())
                .map(|(name, ty)| Parameter {
                    name: name.to_owned(),
                    ty,
                    mode: Mode::Borrow,
                    default: false,
                })
                .collect(),
            returns,
        };
        let builtin = Callee::Builtin(format!("str.{name}"));
        self.check_call(node, name, &signature, Some(receiver), builtin, arguments)
    }

    /// `text.encode()` (decision 0064): an `Array[u8, 1]` of the text's UTF-8 bytes. The one
    /// argument it may take, `encoding`, by position or keyword, must be `"utf-8"`, and is not
    /// passed on: the call is recorded with the text alone.
    fn encode(&mut self, node: NodeId, receiver: Passed<'db>, arguments: &Arguments) -> Type<'db> {
        let encoding = match (
            arguments.positional.as_slice(),
            arguments.keywords.as_slice(),
        ) {
            ([], []) => None,
            ([encoding], []) => Some(*encoding),
            ([], [(name, encoding)]) if self.tree.text(*name, self.text) == "encoding" => {
                Some(*encoding)
            }
            _ => {
                self.untyped(arguments);
                return self.lowerer.error(
                    node,
                    "`encode` takes only `encoding=\"utf-8\"`",
                    "wrong arguments",
                    Some("text is encoded as UTF-8: write `text.encode()` (decision 0064)"),
                );
            }
        };
        if let Some(encoding) = encoding {
            self.encoding(encoding);
        }
        let byte = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::U8)),
        );
        let bytes = Type::new(
            self.db,
            TypeKind::Array {
                element: byte,
                rank: 1,
            },
        );
        let signature = Signature {
            type_parameters: Vec::new(),
            parameters: vec![Parameter {
                name: "self".to_owned(),
                ty: receiver.ty,
                mode: Mode::Borrow,
                default: false,
            }],
            returns: bytes,
        };
        let builtin = Callee::Builtin("str.encode".to_owned());
        let only_text = Arguments {
            positional: Vec::new(),
            keywords: Vec::new(),
        };
        self.check_call(
            node,
            "encode",
            &signature,
            Some(receiver),
            builtin,
            &only_text,
        )
    }

    /// `data.decode()` (decision 0064) on an array: a `str` of the bytes of an `Array[u8, 1]`,
    /// which raises the library's `ValueError` when they are not UTF-8. Its one optional
    /// argument, `encoding`, must be `"utf-8"`.
    pub(super) fn decode(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        arguments: &Arguments,
    ) -> Type<'db> {
        let encoding = match (
            arguments.positional.as_slice(),
            arguments.keywords.as_slice(),
        ) {
            ([], []) => None,
            ([encoding], []) => Some(*encoding),
            ([], [(name, encoding)]) if self.tree.text(*name, self.text) == "encoding" => {
                Some(*encoding)
            }
            _ => {
                self.untyped(arguments);
                return self.lowerer.error(
                    node,
                    "`decode` takes only `encoding=\"utf-8\"`",
                    "wrong arguments",
                    Some("bytes are decoded as UTF-8: write `data.decode()` (decision 0064)"),
                );
            }
        };
        if let Some(encoding) = encoding {
            self.encoding(encoding);
        }
        let bytes = matches!(base.kind(self.db), TypeKind::Array { element, rank: 1 }
            if matches!(element.kind(self.db), TypeKind::Primitive(Primitive::Integer(Integer::U8))));
        if !bytes {
            let message = format!(
                "`decode` needs an `Array[u8, 1]`, found `{}`",
                self.show(base)
            );
            return self.lowerer.error(
                node,
                &message,
                "not bytes",
                Some("`decode` reads UTF-8 bytes, as `gzip.decompress` gives them"),
            );
        }
        let text = Type::new(self.db, TypeKind::Str);
        self.raises_library_error(node, text, "ValueError")
    }
}
