//! The standard `random` module's `Philox` generator (decision 0028): `Philox(seed, stream=k)`
//! makes one, and `generator.uniform()` draws a float in `[0, 1)` from it.
//!
//! The compiler builds the type in until the standard library can define it, so its constructor
//! and method are checked here rather than against a declared signature.

use super::Body;
use super::calls::Arguments;
use syntax::NodeId;
use types::{Float, Integer, Primitive, StandardType, Type, TypeKind};

impl<'db> Body<'db> {
    /// `Philox(seed)` or `Philox(seed, stream)`, the stream positional or by keyword. Both are
    /// `int`s. The seed must come first and by position, so the arguments reach later stages
    /// in the order seed, stream.
    pub(super) fn philox(&mut self, node: NodeId, arguments: &Arguments) -> Type<'db> {
        let int = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        let stream_keyword: Vec<NodeId> = arguments
            .keywords
            .iter()
            .filter(|&&(name, _)| self.tree.text(name, self.text) == "stream")
            .map(|&(_, value)| value)
            .collect();
        let other_keyword = arguments.keywords.len() != stream_keyword.len();
        let given: Vec<NodeId> = arguments
            .positional
            .iter()
            .copied()
            .chain(stream_keyword)
            .collect();
        for &argument in &given {
            let found = self.expression(argument, Some(int));
            self.expect(argument, int, found, "a seed or stream is");
        }
        for &(name, value) in &arguments.keywords {
            if self.tree.text(name, self.text) != "stream" {
                self.expression(value, None);
            }
        }
        if other_keyword || arguments.positional.is_empty() || given.len() > 2 {
            return self.lowerer.error(
                node,
                "`Philox` takes a seed and an optional stream",
                "wrong arguments",
                Some("for example `Philox(2026)` or `Philox(2026, stream=chunk)`"),
            );
        }
        Type::new(self.db, TypeKind::Standard(StandardType::Philox))
    }

    /// `generator.method(arguments)` on a value of the built-in type `standard`, or an error for
    /// a method it does not have; a `File`'s methods are the `files` module's. `uniform()`
    /// changes the generator, as reading or writing changes a file, so the caller records the
    /// receiver as changed, or as moved for a file's `close()`.
    pub(super) fn standard_method(
        &mut self,
        node: NodeId,
        standard: StandardType,
        method: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        match (standard, method) {
            (StandardType::Philox, "uniform")
                if arguments.positional.is_empty() && arguments.keywords.is_empty() =>
            {
                Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64)))
            }
            (StandardType::Philox, "uniform") => {
                self.untyped(arguments);
                self.lowerer.error(
                    node,
                    "`uniform` takes no arguments",
                    "wrong arguments",
                    Some("it draws a float in `[0, 1)`; scale it for another range"),
                )
            }
            (StandardType::Philox, _) => {
                self.untyped(arguments);
                let message = format!("`Philox` has no method `{method}`");
                self.lowerer.error(
                    node,
                    &message,
                    "not defined",
                    Some("a `Philox` has `uniform()` so far"),
                )
            }
            (StandardType::File, _) => self.file_method(node, method, arguments),
        }
    }
}
