//! A struct's `__init__` in function bodies (decision 0068, roadmap 1.7.17): a call of the
//! class calls it, and its own body sets the struct's fields through `self`.
//!
//! Inside `__init__`, `self` is the struct being built. `self.field = value` sets a field and
//! `self.field` reads one already set; the fields set on each path are tracked as definite
//! assignment tracks variables, each field by the node that declares it, so a branch that
//! skips one is seen where the paths join. Every path that ends or returns must have set every
//! field:
//!
//! ```text
//! error: `__init__` can end without setting `offset`
//! ```
//!
//! `self` is never used as a whole, so the struct is never seen half built: `self.method()`,
//! `f(self)` and `return self` are errors, and so is moving a field back out, which would leave
//! a hole. Calling `__init__` by name is an error too; the class's call is its only caller.

use super::Body;
use super::calls::Arguments;
use crate::outline::{Base, Kind, outline};
use database::{Definition, File};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Callee, Declaration, Item, Nominal, Type};

/// What checking an `__init__` body needs to know about the struct it builds.
pub(super) struct Initializer<'db> {
    /// The `Name` node of the `self` parameter, the binding every use of `self` resolves to.
    pub(super) receiver: NodeId,
    /// Each field: its name, the `Name` node that declares it in the class, and its type.
    pub(super) fields: Vec<(String, NodeId, Type<'db>)>,
    /// The fields already reported as missing at some end, each reported once.
    pub(super) reported: std::collections::BTreeSet<NodeId>,
}

impl<'db> Body<'db> {
    /// The `__init__` a call of the class `name`, of kind `kind` in `file`, calls: `None` for
    /// anything but a non-generic struct that defines one, which is built from its fields.
    pub(super) fn initializer(&self, kind: &Kind, file: File, name: NodeId) -> Option<Item<'db>> {
        let Kind::Class(class) = kind else {
            return None;
        };
        if class.base != Base::Struct || !class.type_parameters.is_empty() {
            return None;
        }
        let nominal = Nominal {
            file,
            name,
            arguments: Vec::new(),
        };
        crate::initializers::initializer(self.db, self.lowerer.package, &nominal)
    }

    /// The call `node` of a class, written `callee`, that calls its `__init__`, `initializer`:
    /// checked against `__init__`'s parameters after `self`, and giving the struct.
    pub(super) fn initialized(
        &mut self,
        node: NodeId,
        callee: &str,
        initializer: Item<'db>,
        arguments: &Arguments,
    ) -> Type<'db> {
        let Declaration::Function(signature) = &self.db.declaration(initializer).declaration else {
            return self.untyped(arguments);
        };
        let signature = signature.clone();
        self.check_call(
            node,
            callee,
            &signature,
            None,
            Callee::Method(initializer),
            arguments,
        )
    }

    /// Starts checking the body of `__init__`, the statement `function` of the class `class`:
    /// binds `self`, its first parameter, as the struct being built, with no field set yet.
    pub(super) fn start_initializer(&mut self, function: NodeId, class: NodeId) {
        let receiver = self
            .tree
            .child(function, Role::Parameters)
            .into_iter()
            .flat_map(|parameters| self.tree.children(parameters))
            .find(|&parameter| self.tree.kind(parameter) == SyntaxKind::Parameter)
            // A first parameter with a type is not `self`, which the declaration reports.
            .filter(|&parameter| self.tree.child(parameter, Role::Annotation).is_none())
            .and_then(|parameter| self.tree.child(parameter, Role::Name));
        let Some(receiver) = receiver else {
            return;
        };
        let item = Item::new(self.db, self.lowerer.package, self.lowerer.file, class);
        let Declaration::Struct { fields, .. } = &self.db.declaration(item).declaration else {
            return;
        };
        let Some(statement) = outline(self.db, self.lowerer.file)
            .get(class)
            .map(|entry| entry.statement)
        else {
            return;
        };
        let declared = self
            .tree
            .children_with(statement, Role::Body)
            .filter(|&statement| self.tree.kind(statement) == SyntaxKind::AnnotatedAssign)
            .filter_map(|statement| self.tree.child(statement, Role::Target));
        let fields: Vec<(String, NodeId, Type<'db>)> = fields
            .iter()
            .zip(declared)
            .map(|(field, node)| (field.name.clone(), node, field.ty))
            .collect();
        // Each field is a variable of its own while the body runs, which lowering reads.
        for &(_, declared, ty) in &fields {
            self.locals.insert(declared, ty);
        }
        // Its fields change in place, `self.c[i] = v`, as an `InOut` parameter's do.
        self.modes.insert(receiver, types::Mode::InOut);
        self.state.assigned.insert(receiver);
        self.initializer = Some(Initializer {
            receiver,
            fields,
            reported: std::collections::BTreeSet::new(),
        });
    }

    /// The field of `self` that `node` names inside `__init__`, `self.offset`: its name,
    /// declaring node and type. `None` for anything else.
    fn initialized_field(&self, node: NodeId) -> Option<(String, NodeId, Type<'db>)> {
        let initializer = self.initializer.as_ref()?;
        if self.tree.kind(node) != SyntaxKind::Attribute {
            return None;
        }
        let value = self.tree.child(node, Role::Value)?;
        if self.lowerer.definition(value) != Some(&Definition::Local(initializer.receiver)) {
            return None;
        }
        let attribute = self.tree.child(node, Role::Attribute)?;
        let wanted = self.tree.text(attribute, self.text);
        initializer
            .fields
            .iter()
            .find(|(name, _, _)| name == wanted)
            .cloned()
    }

    /// Whether `node` is `self.something` inside `__init__`, a field or not.
    fn on_receiver(&self, node: NodeId) -> bool {
        let Some(initializer) = &self.initializer else {
            return false;
        };
        self.tree.kind(node) == SyntaxKind::Attribute
            && self.tree.child(node, Role::Value).is_some_and(|value| {
                self.lowerer.definition(value) == Some(&Definition::Local(initializer.receiver))
            })
    }

    /// `self.field = value` inside `__init__`, the target `target`: checks the value against
    /// the field's type and marks the field set on this path. `false` when `target` is not on
    /// `self` there, for the usual assignment.
    pub(super) fn initializer_store(&mut self, target: NodeId, value: NodeId) -> bool {
        if !self.on_receiver(target) {
            return false;
        }
        let Some((_, declared, ty)) = self.initialized_field(target) else {
            self.expression(value, None);
            self.missing_field(target);
            return true;
        };
        let found = self.expression(value, Some(ty));
        self.consume(value, found);
        self.expect(value, ty, found, "the field holds");
        self.typed.expressions.insert(types::Key(target), ty);
        self.state.assigned.insert(declared);
        true
    }

    /// `self.field` read inside `__init__`, at `node`: its type, after checking that every
    /// path to here has set it. `None` when `node` is not on `self` there.
    pub(super) fn initializer_read(&mut self, node: NodeId) -> Option<Type<'db>> {
        if !self.on_receiver(node) {
            return None;
        }
        let Some((name, declared, ty)) = self.initialized_field(node) else {
            return Some(self.missing_field(node));
        };
        if !self.state.assigned.contains(&declared) {
            let message = format!("`self.{name}` is read before `__init__` sets it");
            return Some(self.lowerer.error(
                node,
                &message,
                "not set on every path to here",
                Some("set the field first; until every field is set, `self` is not a whole struct"),
            ));
        }
        Some(ty)
    }

    /// `self.name` inside `__init__` where the struct has no field `name`.
    fn missing_field(&mut self, node: NodeId) -> Type<'db> {
        let attribute = self.tree.child(node, Role::Attribute).unwrap_or(node);
        let name = self.tree.text(attribute, self.text);
        let class = self.show(self.signature.returns);
        let message = format!("`{class}` has no field `{name}`");
        self.lowerer.error(
            attribute,
            &message,
            "not a field",
            Some(
                "`__init__` sets the fields the class declares, and uses `self` only through them",
            ),
        )
    }

    /// The name `node` read as a value: an error for `self` inside `__init__`, otherwise the
    /// usual read (the `expressions` module).
    pub(super) fn receiver_or_name(
        &mut self,
        node: NodeId,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        match self.initializer_receiver(node) {
            Some(ty) => ty,
            None => self.name(node, expected),
        }
    }

    /// `self` used as a whole inside `__init__`, at `node`, where the struct is not built yet.
    /// `None` when `node` is not `self` there.
    fn initializer_receiver(&mut self, node: NodeId) -> Option<Type<'db>> {
        let initializer = self.initializer.as_ref()?;
        if self.lowerer.definition(node) != Some(&Definition::Local(initializer.receiver)) {
            return None;
        }
        Some(self.lowerer.error(
            node,
            "`self` is used only through its fields in `__init__`",
            "the struct is not built yet",
            Some(
                "compute what a method would give from the arguments first, then set the \
                 field (decision 0068)",
            ),
        ))
    }

    /// Moving `self.field` out inside `__init__`, the read `node`, which would leave the
    /// struct with a hole. `false` when `node` is not a field of `self` there.
    pub(super) fn initializer_field_moved(&mut self, node: NodeId) -> bool {
        if !self.on_receiver(node) {
            return false;
        }
        self.lowerer.error(
            node,
            "a field of `self` cannot be moved out in `__init__`",
            "moved here",
            Some("keep a copy instead: `self.field.copy()`"),
        );
        true
    }

    /// Reports each field that a path reaching `node`, a `return` or the end of `__init__`,
    /// has not set. A field is reported at the first such end only.
    pub(super) fn initializer_end(&mut self, node: NodeId) {
        let Some(initializer) = &mut self.initializer else {
            return;
        };
        let mut missing = Vec::new();
        for (name, declared, _) in &initializer.fields {
            if !self.state.assigned.contains(declared) && initializer.reported.insert(*declared) {
                missing.push(name.clone());
            }
        }
        for name in missing {
            let message = format!("`__init__` can end without setting `{name}`");
            self.lowerer.error(
                node,
                &message,
                "a path ends here",
                Some("set every field on every path through `__init__`"),
            );
        }
    }

    /// `x.__init__(...)`, the call `function`: an error, since calling the class is the only
    /// way to run it.
    pub(super) fn called_initializer(
        &mut self,
        function: NodeId,
        arguments: &Arguments,
    ) -> Type<'db> {
        self.untyped(arguments);
        self.lowerer.error(
            function,
            "`__init__` runs when the class is called, never by name",
            "called here",
            Some("call the class instead: `CubicSpline(x, y)` (decision 0068)"),
        )
    }
}
