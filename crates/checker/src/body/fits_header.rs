//! A FITS `Header`'s values (decision 0044): `header[key]`, `header.get(key, default)`,
//! `key in header` and `header[key] = value`, as `AstroPy`'s `Header` reads and sets them.
//!
//! A card holds a string, an integer, a float or a logical value, and a Baryon value has one
//! type, so a read takes its type from the context, as `exposure: float = header["EXPTIME"]`
//! does, and a write from the value. Each is a call of a function of the library's
//! `<std>/fits.byn`, which reads and writes the cards' text: the checker records the call on
//! the subscript, the comparison or the target, and lowering lowers it as any call is lowered.
//! A read raises `KeyError` for a missing card or one of another type.
//!
//! The header of a write, and of `in`, is a variable, whose type is known before the statement
//! is checked; a header reached through a field is checked as any struct is, and refused.

use super::Body;
use super::calls::Arguments;
use super::exclusive::Passed;
use database::Definition;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Callee, Declaration, Float, Integer, Item, Key, Mode, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// Whether `ty` is the library's `fits.Header`.
    pub(super) fn is_header(&self, ty: Type<'db>) -> bool {
        self.library_class_in("fits", "Header") == Some(ty)
    }

    /// Whether `node` names a variable that holds a `Header`, known before the statement that
    /// reads it is checked.
    pub(super) fn names_header(&self, node: NodeId) -> bool {
        if self.tree.kind(node) != SyntaxKind::Name {
            return false;
        }
        let Some(&Definition::Local(binding)) = self.lowerer.definition(node) else {
            return false;
        };
        self.locals
            .get(&binding)
            .is_some_and(|&ty| self.is_header(ty))
    }

    /// `header[key]`, the subscript `node` of the `Header` `value` of type `base`, read as the
    /// type `expected` the context needs.
    pub(super) fn header_item(
        &mut self,
        node: NodeId,
        value: NodeId,
        base: Type<'db>,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        let Some(key) = self.tree.child(node, Role::Index) else {
            return self.lowerer.unknown();
        };
        let Some(kind) = expected.and_then(|ty| self.card_kind(ty)) else {
            self.expression(key, None);
            return self.lowerer.error(
                node,
                "the type of a header value comes from where it goes",
                "type not known",
                Some(
                    "give it a type, such as `exposure: float = header[\"EXPTIME\"]`; a card \
                     holds a `str`, an `int`, a `float` or a `bool` (decision 0044)",
                ),
            );
        };
        let arguments = Arguments {
            positional: vec![key],
            keywords: Vec::new(),
        };
        let ty = self.header_call(node, &format!("_get_{kind}"), (value, base), &arguments);
        self.fallible_at(node, ty, "reading a header value can fail with");
        match ty.kind(self.db) {
            TypeKind::Fallible { value, .. } => *value,
            _ => ty,
        }
    }

    /// `header.get(key, default)`, the call `node` on the `Header` `value` of type `base`: the
    /// value of the type of `default`, or `default` when there is no card `key`.
    pub(super) fn header_get(
        &mut self,
        node: NodeId,
        value: NodeId,
        base: Type<'db>,
        arguments: &Arguments,
    ) -> Type<'db> {
        let [key, default] = arguments.positional[..] else {
            self.untyped(arguments);
            return self.lowerer.error(
                node,
                "`Header.get` takes a keyword and a default",
                "wrong arguments",
                Some("for example `header.get(\"FILTER\", \"none\")`; the default gives the type"),
            );
        };
        if !arguments.keywords.is_empty() {
            self.untyped(arguments);
            return self.lowerer.error(
                node,
                "`Header.get` takes its arguments by position",
                "wrong arguments",
                Some("for example `header.get(\"FILTER\", \"none\")`"),
            );
        }
        let found = self.expression(default, None);
        let Some(kind) = self.card_kind(found) else {
            let message = format!(
                "a header value is a `str`, an `int`, a `float` or a `bool`, found `{}`",
                self.show(found)
            );
            self.expression(key, None);
            return self
                .lowerer
                .error(default, &message, "not a card's type", None);
        };
        let given = Arguments {
            positional: vec![key, default],
            keywords: Vec::new(),
        };
        self.header_call(node, &format!("_get_{kind}_or"), (value, base), &given)
    }

    /// `key in header` or `key not in header`, the comparison `node` whose one comparator is
    /// `operator`'s, when `right` names a `Header` variable: a `bool`. `None` for any other
    /// comparison.
    pub(super) fn header_contains(&mut self, node: NodeId) -> Option<Type<'db>> {
        let left = self.tree.child(node, Role::Left)?;
        let comparators: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        let [comparator] = comparators[..] else {
            return None;
        };
        let right = self.tree.child(comparator, Role::Right)?;
        let function = match self.tree.node(comparator).operator() {
            syntax::Operator::In => "_contains",
            syntax::Operator::NotIn => "_lacks",
            _ => return None,
        };
        if !self.names_header(right) {
            return None;
        }
        let base = self.expression(right, None);
        let arguments = Arguments {
            positional: vec![left],
            keywords: Vec::new(),
        };
        Some(self.header_call(node, function, (right, base), &arguments))
    }

    /// `header[key] = value` or `header[key] = (value, comment)`, the assignment of `value` to
    /// the subscript `target`, when it names a `Header` variable; returns whether it did.
    pub(super) fn header_store(&mut self, target: NodeId, value: NodeId) -> bool {
        let Some(header) = self.tree.child(target, Role::Value) else {
            return false;
        };
        if self.tree.kind(target) != SyntaxKind::Subscript || !self.names_header(header) {
            return false;
        }
        let Some(key) = self.tree.child(target, Role::Index) else {
            return false;
        };
        self.change(header);
        let base = self.expression(header, None);
        let items: Vec<NodeId> = self.tree.children_with(value, Role::Item).collect();
        let (written, comment) = match (self.tree.kind(value), &items[..]) {
            (SyntaxKind::Tuple, &[written, comment]) => (written, Some(comment)),
            _ => (value, None),
        };
        let found = self.expression(written, None);
        let Some(kind) = self.card_kind(found) else {
            let message = format!(
                "a header value is a `str`, an `int`, a `float` or a `bool`, found `{}`",
                self.show(found)
            );
            self.expression(key, None);
            if let Some(comment) = comment {
                self.expression(comment, None);
            }
            self.lowerer
                .error(written, &message, "not a card's type", None);
            return true;
        };
        let (function, positional) = match comment {
            Some(comment) => (
                format!("_set_{kind}_commented"),
                vec![key, written, comment],
            ),
            None => (format!("_set_{kind}"), vec![key, written]),
        };
        let arguments = Arguments {
            positional,
            keywords: Vec::new(),
        };
        self.header_call(target, &function, (header, base), &arguments);
        true
    }

    /// The name the library's functions give a card of type `ty`: `str`, `int`, `float` or
    /// `bool`, or `None` for a type no card holds.
    fn card_kind(&self, ty: Type<'db>) -> Option<&'static str> {
        match ty.kind(self.db) {
            TypeKind::Str => Some("str"),
            TypeKind::Primitive(Primitive::Integer(Integer::I64)) => Some("int"),
            TypeKind::Primitive(Primitive::Float(Float::F64)) => Some("float"),
            TypeKind::Primitive(Primitive::Bool) => Some("bool"),
            _ => None,
        }
    }

    /// The call `node` of the library function `function` of `<std>/fits.byn`, with the
    /// `Header` `header` of type `base` as its receiver, then `arguments`. The checker checks it
    /// as a call of that function and records it at `node`.
    fn header_call(
        &mut self,
        node: NodeId,
        function: &str,
        (header, base): (NodeId, Type<'db>),
        arguments: &Arguments,
    ) -> Type<'db> {
        let signature = self
            .library_function("fits", function)
            .map(|(file, name)| Item::new(self.db, self.lowerer.package, file, name))
            .and_then(|item| match self.db.declaration(item).declaration.clone() {
                Declaration::Function(signature) => Some((item, signature)),
                _ => None,
            });
        let Some((item, signature)) = signature else {
            self.untyped(arguments);
            let message = format!("`fits.{function}` is missing from the standard library");
            return self.lowerer.error(
                node,
                &message,
                "cannot be typed",
                Some("the compiler adds the library's files to every package (decision 0038)"),
            );
        };
        let mode = signature
            .parameters
            .first()
            .map_or(Mode::Borrow, |parameter| parameter.mode);
        let receiver = Passed {
            node: header,
            mode,
            ty: base,
        };
        let ty = self.check_call(
            node,
            "Header",
            &signature,
            Some(receiver),
            Callee::Function(item),
            arguments,
        );
        debug_assert!(self.typed.calls.contains_key(&Key(node)));
        ty
    }
}
