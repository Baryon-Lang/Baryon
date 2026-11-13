//! The `h5by` module (decision 0052), with h5py's names: `h5by.File(path)`, `f[name]`,
//! `group[name]`, `dset[...]` and `dset[0, ::2]`, `keys()`, `name in group`, `obj.attrs[name]`
//! and `obj.attrs.keys()`.
//!
//! `h5by.File(path)` is a built-in call that can fail with `OSError`; the runtime reads the
//! file's structure and the library's `File` keeps it as text. A subscript of a dataset is the
//! other built-in call: it reads the values it picks into an `Array[T, rank]` whose element
//! type and rank come from the context, else `Array[f64, 2]`, and the runtime checks the
//! dataset against them. Everything else is a call of a function of `<std>/h5by.byn`, which
//! the checker records where the program wrote it, as a `NetCDF` variable's are (decision
//! 0047): `f["run1/temp"]` is `_dataset(f, "run1/temp")`, which raises `KeyError` for a
//! missing name.
//!
//! In h5py `f[name]` gives a group or a dataset, whichever the file holds; in Baryon the type
//! comes from how the value is used, as Collins chose on 2026-11-13. A lookup indexed again by
//! a name, `f["run1"]["temp"]`, asked for its `keys()`, or on the right of `in`, is a `Group`;
//! a lookup given the type `h5by.Group` is one too; any other lookup is a `Dataset`. A
//! variable that holds a lookup takes its type there, so `g = f["run1"]` then `g["temp"]` is a
//! compile error that asks for `g: h5by.Group = f["run1"]`.

use super::Body;
use super::calls::Arguments;
use super::netcdf::subscript_errors;
use database::Definition;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Call, Callee, Float, Key, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// `h5by.member(arguments)`: `File(name, mode="r")`, a `File` that can fail with `OSError`,
    /// whose recorded argument is the path. Writing (mode `"w"` and the rest) is roadmap
    /// 3.14.6.2.
    pub(super) fn h5by_call(
        &mut self,
        node: NodeId,
        member: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        if member != "File" {
            self.untyped(arguments);
            let message = format!("the `h5by` module has no `{member}` yet");
            return self.lowerer.error(
                node,
                &message,
                "not built yet",
                Some("decision 0052 builds `File`, for reading HDF5 files"),
            );
        }
        let mut path = arguments.positional.first().copied();
        let mut mode = arguments.positional.get(1).copied();
        let mut wrong = arguments.positional.len() > 2;
        for &(name, value) in &arguments.keywords {
            let slot = match self.tree.text(name, self.text) {
                "name" => &mut path,
                "mode" => &mut mode,
                _ => {
                    wrong = true;
                    continue;
                }
            };
            wrong |= slot.replace(value).is_some();
        }
        let str = Type::new(self.db, TypeKind::Str);
        let Some(path) = path.filter(|_| !wrong) else {
            self.untyped(arguments);
            return self.lowerer.error(
                node,
                "`File` takes `name` and `mode`",
                "wrong arguments",
                Some("for example `h5by.File(\"run.h5\")` or `h5by.File(\"run.h5\", \"r\")`"),
            );
        };
        let found = self.expression(path, Some(str));
        self.expect(path, str, found, "a path is");
        if let Some(mode) = mode {
            let found = self.expression(mode, Some(str));
            self.expect(mode, str, found, "a mode is");
            if self.literal_text(mode) != Some("r") {
                return self.lowerer.error(
                    mode,
                    "an HDF5 file is opened to read, so far",
                    "not built yet",
                    Some("write the mode `\"r\"`; writing HDF5 files is roadmap row 3.14.6.2"),
                );
            }
        }
        let Some(file) = self.library_class_in("h5by", "File") else {
            return self.missing_h5by_class(node, "File");
        };
        let call = Call {
            callee: Callee::Builtin("h5by.File".to_owned()),
            type_arguments: Vec::new(),
            arguments: vec![Some(Key(path))],
        };
        self.typed.calls.insert(Key(node), call);
        self.raises_library_error(node, file, "OSError")
    }

    /// Whether the class `name` of `file` is the library's `h5by.File`, which a program calls
    /// to open a file rather than to build one field by field.
    pub(super) fn is_h5by_file_class(&self, file: database::File, name: NodeId) -> bool {
        self.library_class_in("h5by", "File").is_some_and(|class| {
            matches!(class.kind(self.db), TypeKind::Struct(nominal) if nominal.file == file && nominal.name == name)
        })
    }

    /// Whether `ty` is the library's `h5by` class `name`.
    pub(super) fn is_h5by(&self, ty: Type<'db>, name: &str) -> bool {
        self.library_class_in("h5by", name) == Some(ty)
    }

    /// The `h5by` class a value of type `ty` is: `File`, `Group` or `Dataset`.
    fn h5by_class(&self, ty: Type<'db>) -> Option<&'static str> {
        ["File", "Group", "Dataset"]
            .into_iter()
            .find(|&class| self.is_h5by(ty, class))
    }

    /// The `h5by` class `node` names before it is checked: a variable that holds a `File`, a
    /// `Group` or a `Dataset`, or `"lookup"` for a lookup of a name in a file or a group, whose
    /// class comes from how it is used.
    fn names_h5by(&self, node: NodeId) -> Option<&'static str> {
        match self.tree.kind(node) {
            SyntaxKind::Name => {
                let Some(&Definition::Local(binding)) = self.lowerer.definition(node) else {
                    return None;
                };
                let &ty = self.locals.get(&binding)?;
                self.h5by_class(ty)
            }
            SyntaxKind::Subscript => {
                let value = self.tree.child(node, Role::Value)?;
                let index = self.tree.child(node, Role::Index)?;
                let container = matches!(self.names_h5by(value), Some("File" | "Group" | "lookup"));
                (container && self.is_text(index)).then_some("lookup")
            }
            _ => None,
        }
    }

    /// Whether `node`, an index, is text before it is checked: a string literal, an f-string,
    /// or a variable that holds a `str`.
    fn is_text(&self, node: NodeId) -> bool {
        match self.tree.kind(node) {
            SyntaxKind::StringLiteral | SyntaxKind::FormattedString => true,
            SyntaxKind::Name => match self.lowerer.definition(node) {
                Some(&Definition::Local(binding)) => self
                    .locals
                    .get(&binding)
                    .is_some_and(|ty| matches!(ty.kind(self.db), TypeKind::Str)),
                _ => false,
            },
            _ => false,
        }
    }

    /// Checks `node`, a value of an `h5by` class, as `class` when it is a lookup, whose class
    /// comes from its use: its type.
    fn h5by_value(&mut self, node: NodeId, class: &str) -> Type<'db> {
        let expected = if self.names_h5by(node) == Some("lookup") {
            self.library_class_in("h5by", class)
        } else {
            None
        };
        self.expression(node, expected)
    }

    /// The subscript `node` when it reads an HDF5 file: `f[name]` or `group[name]`, a lookup
    /// read as the type `expected` the context needs; `dset[...]` or `dset[0, ::2]`, the values
    /// it picks; or `obj.attrs[name]`, an attribute typed by `expected`. `None` for any other
    /// subscript.
    pub(super) fn h5by_item(
        &mut self,
        node: NodeId,
        expected: Option<Type<'db>>,
    ) -> Option<Type<'db>> {
        let value = self.tree.child(node, Role::Value)?;
        let index = self.tree.child(node, Role::Index)?;
        if let Some(owner) = self.attrs_owner(value) {
            return Some(self.h5by_attribute_read(node, owner, index, expected));
        }
        let class = self.names_h5by(value)?;
        if self.is_text(index) {
            let base = self.h5by_value(value, "Group");
            let wants_group = expected.is_some_and(|ty| self.is_h5by(ty, "Group"));
            let function = match (self.h5by_class(base)?, wants_group) {
                ("File", false) => "_dataset",
                ("File", true) => "_group",
                ("Group", false) => "_group_dataset",
                ("Group", true) => "_group_group",
                (class, _) => return Some(self.not_a_group(node, value, class)),
            };
            let class = self.h5by_class(base)?;
            let arguments = Arguments {
                positional: vec![index],
                keywords: Vec::new(),
            };
            let ty =
                self.library_method(node, ("h5by", class, function), (value, base), &arguments);
            return Some(self.raised_value(node, ty, "reading an HDF5 file can fail with"));
        }
        if class != "Dataset" && class != "lookup" {
            self.expression(value, None);
            return Some(self.not_a_group(node, value, class));
        }
        self.h5by_value(value, "Dataset");
        Some(self.h5by_read(node, value, index, expected))
    }

    /// The error for `value[index]`, the subscript `node`, where `value` is a `class` that is
    /// not a group: a dataset indexed by a name, or a file or group indexed by numbers.
    fn not_a_group(&mut self, node: NodeId, value: NodeId, class: &str) -> Type<'db> {
        let name = self.tree.text(value, self.text).to_owned();
        let (message, label, help) = if class == "Dataset" {
            (
                format!("`{name}` is an HDF5 dataset, which is not read by name"),
                "a dataset",
                format!(
                    "if it is a group, give it the type, as `{name}: h5by.Group = ...` \
                     (decision 0052)"
                ),
            )
        } else {
            (
                format!("an HDF5 {} is read by name", class.to_lowercase()),
                "not a name",
                format!("for example `{name}[\"temp\"]`, then a subscript of the dataset"),
            )
        };
        self.lowerer.error(node, &message, label, Some(&help))
    }

    /// `dset[...]` or `dset[0, ::2]`, the subscript `node` of the `Dataset` `value` with the
    /// index `index`: the values it picks, as an array of the type `expected` the context needs.
    /// The runtime checks the array's rank against the axes the subscript keeps.
    fn h5by_read(
        &mut self,
        node: NodeId,
        value: NodeId,
        index: NodeId,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        let items = if self.tree.kind(index) == SyntaxKind::Ellipsis {
            Some(Vec::new())
        } else {
            self.subscript_items(
                index,
                "an HDF5 dataset",
                "for example `dset[...]` or `dset[0, :, ::2]`; other indexes come later under \
                 decision 0052",
            )
        };
        let Some(items) = items else {
            return self.lowerer.unknown();
        };
        let array = match expected.map(|ty| (ty, ty.kind(self.db))) {
            Some((ty, TypeKind::Array { element, .. }))
                if matches!(
                    element.kind(self.db),
                    TypeKind::Primitive(Primitive::Integer(_) | Primitive::Float(_))
                ) =>
            {
                ty
            }
            Some((ty, TypeKind::Array { .. })) => {
                let message = format!("an HDF5 dataset cannot be read into an `{}`", self.show(ty));
                return self.lowerer.error(
                    node,
                    &message,
                    "not a type an HDF5 dataset holds",
                    Some(
                        "datasets of integers and floats are read so far, into an array such \
                         as `Array[f32, 3]` (decision 0052)",
                    ),
                );
            }
            _ => {
                let element = Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64)));
                Type::new(self.db, TypeKind::Array { element, rank: 2 })
            }
        };
        let call = Call {
            callee: Callee::Builtin("h5by.read".to_owned()),
            type_arguments: Vec::new(),
            arguments: std::iter::once(value)
                .chain(items.iter().copied())
                .map(|node| Some(Key(node)))
                .collect(),
        };
        self.typed.calls.insert(Key(node), call);
        let ty = self.raises_library_errors(node, array, subscript_errors(&items));
        self.raised_value(node, ty, "reading an HDF5 dataset can fail with")
    }

    /// The object whose attributes `node` is, when `node` is `obj.attrs` for a `File`, a
    /// `Group` or a `Dataset` `obj`: `obj`. `None` for anything else.
    fn attrs_owner(&self, node: NodeId) -> Option<NodeId> {
        if self.tree.kind(node) != SyntaxKind::Attribute {
            return None;
        }
        let attribute = self.tree.child(node, Role::Attribute)?;
        let owner = self.tree.child(node, Role::Value)?;
        (self.tree.text(attribute, self.text) == "attrs" && self.names_h5by(owner).is_some())
            .then_some(owner)
    }

    /// The prefix of the library's functions for an object of `class`: none for a `File`.
    fn prefix(class: &str) -> &'static str {
        match class {
            "Group" => "_group",
            "Dataset" => "_dataset",
            _ => "",
        }
    }

    /// `obj.attrs[name]`, the subscript `node` with the index `index`, of the object `owner`:
    /// the attribute, read as the type `expected` the context needs.
    fn h5by_attribute_read(
        &mut self,
        node: NodeId,
        owner: NodeId,
        index: NodeId,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        let base = self.h5by_value(owner, "Dataset");
        let Some(class) = self.h5by_class(base) else {
            return self.lowerer.unknown();
        };
        let kind = expected.and_then(|ty| match ty.kind(self.db) {
            TypeKind::Array { element, rank: 1 }
                if matches!(
                    element.kind(self.db),
                    TypeKind::Primitive(Primitive::Float(Float::F64))
                ) =>
            {
                Some("array")
            }
            _ => self.attribute_kind(ty),
        });
        let Some(kind) = kind else {
            self.expression(index, None);
            return self.lowerer.error(
                node,
                "the type of an HDF5 attribute comes from where it goes",
                "type not known",
                Some(
                    "give it a type, such as `units: str = dset.attrs[\"units\"]`; an \
                     attribute is read as a `str`, an `int`, a `float` or an `Array[f64, 1]` \
                     (decision 0052)",
                ),
            );
        };
        let function = format!("{}_get_{kind}", Self::prefix(class));
        let arguments = Arguments {
            positional: vec![index],
            keywords: Vec::new(),
        };
        let ty = self.library_method(node, ("h5by", class, &function), (owner, base), &arguments);
        self.raised_value(node, ty, "reading an HDF5 attribute can fail with")
    }

    /// The type a method's receiver `value` is checked against when the method is `method`:
    /// a `Group` for a lookup whose `keys()` the program asks for. `None` otherwise.
    pub(super) fn h5by_receiver(&self, value: NodeId, method: &str) -> Option<Type<'db>> {
        (method == "keys" && self.names_h5by(value) == Some("lookup"))
            .then(|| self.library_class_in("h5by", "Group"))
            .flatten()
    }

    /// The call `node` of `method` on `value`, when `value` is `obj.attrs` of an `h5by` object
    /// and the method `keys`: the names of its attributes. `None` for any other call.
    pub(super) fn h5by_attrs_method(
        &mut self,
        node: NodeId,
        value: NodeId,
        method: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let owner = self.attrs_owner(value)?;
        let base = self.h5by_value(owner, "Dataset");
        let class = self.h5by_class(base)?;
        if method != "keys" {
            self.untyped(arguments);
            let message = format!("the attributes of an HDF5 object have no `{method}` yet");
            return Some(self.lowerer.error(
                node,
                &message,
                "not built yet",
                Some("read one as `obj.attrs[name]`, or list them with `obj.attrs.keys()`"),
            ));
        }
        let function = format!("{}_attrs_keys", Self::prefix(class));
        Some(self.library_method(node, ("h5by", class, &function), (owner, base), arguments))
    }

    /// The call `node` of `method` on the value `value` of type `base`, when `base` is a `File`
    /// or a `Group` and `method` one of theirs: `keys()`, and `close()` of a `File`. `None` for
    /// any other method.
    pub(super) fn h5by_method(
        &mut self,
        node: NodeId,
        (value, base): (NodeId, Type<'db>),
        method: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let class = self.h5by_class(base)?;
        let function = match (class, method) {
            ("File", "keys") => "_keys",
            ("Group", "keys") => "_group_keys",
            ("File", "close") => "_close",
            _ => return None,
        };
        Some(self.library_method(node, ("h5by", class, function), (value, base), arguments))
    }

    /// `name in f`, `name in group` or `name in obj.attrs`, the comparison `node` whose one
    /// comparator is `in` or `not in`: a `bool`. `None` for any other comparison.
    pub(super) fn h5by_contains(&mut self, node: NodeId) -> Option<Type<'db>> {
        let left = self.tree.child(node, Role::Left)?;
        let comparators: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        let [comparator] = comparators[..] else {
            return None;
        };
        let right = self.tree.child(comparator, Role::Right)?;
        let negated = match self.tree.node(comparator).operator() {
            syntax::Operator::In => false,
            syntax::Operator::NotIn => true,
            _ => return None,
        };
        let (owner, attributes) = match self.attrs_owner(right) {
            Some(owner) => (owner, true),
            None => (right, false),
        };
        self.names_h5by(owner)?;
        let base = self.h5by_value(owner, if attributes { "Dataset" } else { "Group" });
        let class = self.h5by_class(base)?;
        let function = match (class, attributes) {
            ("File", false) => "_contains".to_owned(),
            ("Group", false) => "_group_contains".to_owned(),
            (class, true) => format!("{}_attrs_contains", Self::prefix(class)),
            (class, false) => return Some(self.not_a_group(node, owner, class)),
        };
        let function = if negated {
            function.replacen("contains", "lacks", 1)
        } else {
            function
        };
        let arguments = Arguments {
            positional: vec![left],
            keywords: Vec::new(),
        };
        Some(self.library_method(node, ("h5by", class, &function), (owner, base), &arguments))
    }

    /// Reports that the library's `h5by` class `name` is missing at `node`, which happens only
    /// when the library's files were left out of the package.
    fn missing_h5by_class(&mut self, node: NodeId, name: &str) -> Type<'db> {
        let message = format!("`h5by.{name}` is missing from the standard library");
        self.lowerer.error(
            node,
            &message,
            "cannot be typed",
            Some("the compiler adds the library's files to every package (decision 0038)"),
        )
    }
}
