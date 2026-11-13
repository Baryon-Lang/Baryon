//! The `netCDF4` module (decision 0047), with Unidata's names: `Dataset(path)`,
//! `ds.variables[name]`, `ds.dimensions[name]`, `var[:]`, and `getncattr` and `ncattrs` of a
//! dataset or a variable.
//!
//! `Dataset(path)` is a built-in call that can fail with `OSError`; the runtime reads the
//! header and the library's `Dataset` keeps it as text. Everything else a program asks of a
//! `Dataset`, a `Variable` or a `Dimension` is a call of a function of `<std>/netCDF4.byn`, which
//! the checker records where the program wrote it, as a FITS `Header`'s reads are recorded
//! (decision 0044): `ds.variables["temp"]` is `_variable(ds, "temp")`, which raises `KeyError`
//! for a missing name. `var[:]` is the other built-in call: it reads the variable's values into
//! an `Array[T, rank]` whose element type and rank come from the context, as
//! `fits.getdata`'s do, else `Array[f64, 2]`, and the runtime checks the variable against them.
//! An attribute is typed by the context too: `units: str = var.getncattr("units")`.
//!
//! `Dataset(path, "w", format=...)` makes a new file; the methods that define and fill it are
//! the `netcdf_writing` module's.

use super::Body;
use super::calls::Arguments;
use syntax::{Flags, NodeId, Role, SyntaxKind};
use types::{Call, Callee, Float, Integer, Key, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// `netCDF4.member(arguments)`: `Dataset(filename, mode="r", format=...)`, a `Dataset`
    /// that can fail with `OSError`. The mode `"r"`, `"a"` or `"r+"` opens a file, and the
    /// recorded argument is the path; the mode `"w"` makes a new one, in the format `format`
    /// names, and the recorded arguments are the path and the format.
    pub(super) fn netcdf_call(
        &mut self,
        node: NodeId,
        member: &str,
        arguments: &Arguments,
    ) -> Type<'db> {
        if member != "Dataset" {
            self.untyped(arguments);
            let message = format!("the `netCDF4` module has no `{member}` yet");
            return self.lowerer.error(
                node,
                &message,
                "not built yet",
                Some("decision 0047 builds `Dataset` for NetCDF classic files"),
            );
        }
        let mut path = arguments.positional.first().copied();
        let mut mode = arguments.positional.get(1).copied();
        let mut format = None;
        let mut wrong = arguments.positional.len() > 2;
        for &(name, value) in &arguments.keywords {
            let slot = match self.tree.text(name, self.text) {
                "filename" => &mut path,
                "mode" => &mut mode,
                "format" => &mut format,
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
                "`Dataset` takes `filename`, `mode` and `format`",
                "wrong arguments",
                Some("for example `Dataset(\"air.nc\")` or `Dataset(\"air.nc\", \"r\")`"),
            );
        };
        let found = self.expression(path, Some(str));
        self.expect(path, str, found, "a path is");
        let mut written = vec![Some(Key(path))];
        if let Some(mode) = mode {
            let found = self.expression(mode, Some(str));
            self.expect(mode, str, found, "a mode is");
            match self.literal_text(mode) {
                Some("r" | "a" | "r+") => {}
                Some("w") => match self.netcdf_format(node, format) {
                    Some(format) => written.push(Some(Key(format))),
                    None => return self.lowerer.unknown(),
                },
                _ => {
                    return self.lowerer.error(
                        mode,
                        "`Dataset` takes the mode `\"r\"`, `\"a\"`, `\"r+\"` or `\"w\"`",
                        "not a mode",
                        Some("write the mode as text in the call, such as `\"w\"`"),
                    );
                }
            }
        }
        let Some(dataset) = self.library_class_in("netCDF4", "Dataset") else {
            return self.missing_netcdf_class(node, "Dataset");
        };
        let name = if written.len() == 2 {
            "netCDF4.create"
        } else {
            "netCDF4.Dataset"
        };
        let call = Call {
            callee: Callee::Builtin(name.to_owned()),
            type_arguments: Vec::new(),
            arguments: written,
        };
        self.typed.calls.insert(Key(node), call);
        self.raises_library_error(node, dataset, "OSError")
    }

    /// The text of `node` when it is a string literal, without its quotes.
    pub(super) fn literal_text(&self, node: NodeId) -> Option<&str> {
        (self.tree.kind(node) == SyntaxKind::StringLiteral)
            .then(|| self.tree.text(node, self.text))
            .and_then(|text| text.get(1..text.len().checked_sub(1)?))
    }

    /// The `format` of `Dataset(path, "w", format=...)` at `node`, after checking that it names
    /// a classic format: `NETCDF3_CLASSIC`, or `NETCDF3_64BIT_OFFSET` or its other name
    /// `NETCDF3_64BIT`. `None`, after reporting it, for anything else.
    fn netcdf_format(&mut self, node: NodeId, format: Option<NodeId>) -> Option<NodeId> {
        let Some(format) = format else {
            self.lowerer.error(
                node,
                "a new NetCDF file needs its `format`",
                "no format",
                Some(
                    "`netCDF4` writes NetCDF-4 files when the format is left out, which are not \
                     built yet; write `format=\"NETCDF3_CLASSIC\"` or \
                     `format=\"NETCDF3_64BIT_OFFSET\"`",
                ),
            );
            return None;
        };
        let str = Type::new(self.db, TypeKind::Str);
        let found = self.expression(format, Some(str));
        self.expect(format, str, found, "a format is");
        let (label, help) = match self.literal_text(format) {
            Some("NETCDF3_CLASSIC" | "NETCDF3_64BIT_OFFSET" | "NETCDF3_64BIT") => {
                return Some(format);
            }
            Some("NETCDF4" | "NETCDF4_CLASSIC") => (
                "not built yet",
                "NetCDF-4 files ride on HDF5, which comes later (row 3.14.6); write \
                 `\"NETCDF3_CLASSIC\"` or `\"NETCDF3_64BIT_OFFSET\"`",
            ),
            _ => (
                "not a format",
                "write `\"NETCDF3_CLASSIC\"` or `\"NETCDF3_64BIT_OFFSET\"` as text in the call",
            ),
        };
        self.lowerer.error(
            format,
            "a NetCDF file is written as a classic file, so far",
            label,
            Some(help),
        );
        None
    }

    /// Whether the class `name` of `file` is the library's `netCDF4.Dataset`, which a program
    /// calls to open a file rather than to build one field by field.
    pub(super) fn is_dataset_class(&self, file: database::File, name: NodeId) -> bool {
        self.library_class_in("netCDF4", "Dataset").is_some_and(|dataset| {
            matches!(dataset.kind(self.db), TypeKind::Struct(nominal) if nominal.file == file && nominal.name == name)
        })
    }

    /// Whether `ty` is the library's `netCDF4` class `name`.
    pub(super) fn is_netcdf(&self, ty: Type<'db>, name: &str) -> bool {
        self.library_class_in("netCDF4", name) == Some(ty)
    }

    /// The subscript `node` when it reads a `NetCDF` file: `ds.variables[name]`,
    /// `ds.dimensions[name]` or `var[:]`, read as the type `expected` the context needs. `None`
    /// for any other subscript.
    pub(super) fn netcdf_item(
        &mut self,
        node: NodeId,
        expected: Option<Type<'db>>,
    ) -> Option<Type<'db>> {
        let value = self.tree.child(node, Role::Value)?;
        let index = self.tree.child(node, Role::Index)?;
        if self.tree.kind(value) == SyntaxKind::Attribute {
            let dataset = self.tree.child(value, Role::Value)?;
            let attribute = self.tree.child(value, Role::Attribute)?;
            let function = match self.tree.text(attribute, self.text) {
                "variables" => "_variable",
                "dimensions" => "_dimension",
                _ => return None,
            };
            if !self.names_netcdf(dataset, "Dataset") {
                return None;
            }
            let base = self.expression(dataset, None);
            let arguments = Arguments {
                positional: vec![index],
                keywords: Vec::new(),
            };
            let ty = self.library_method(
                node,
                ("netCDF4", "Dataset", function),
                (dataset, base),
                &arguments,
            );
            return Some(self.raised_value(node, ty, "reading a NetCDF file can fail with"));
        }
        if !self.names_netcdf(value, "Variable") {
            return None;
        }
        self.expression(value, None);
        Some(self.netcdf_read(node, value, index, expected))
    }

    /// Whether `node`, before it is checked, is a value of the library's `netCDF4` class
    /// `class`: a variable that holds one, or for a `Variable` also `ds.variables[name]`.
    pub(super) fn names_netcdf(&self, node: NodeId, class: &str) -> bool {
        match self.tree.kind(node) {
            SyntaxKind::Name => {
                let Some(&database::Definition::Local(binding)) = self.lowerer.definition(node)
                else {
                    return false;
                };
                self.locals
                    .get(&binding)
                    .is_some_and(|&ty| self.is_netcdf(ty, class))
            }
            SyntaxKind::Subscript if class == "Variable" => self
                .tree
                .child(node, Role::Value)
                .filter(|&value| self.tree.kind(value) == SyntaxKind::Attribute)
                .and_then(|value| {
                    let attribute = self.tree.child(value, Role::Attribute)?;
                    let dataset = self.tree.child(value, Role::Value)?;
                    Some(
                        self.tree.text(attribute, self.text) == "variables"
                            && self.names_netcdf(dataset, "Dataset"),
                    )
                })
                .unwrap_or(false),
            _ => false,
        }
    }

    /// `var[:]` or `var[0, :, ::2]`, the subscript `node` of the `Variable` `value` with the
    /// index `index`: the values it picks, as an array of the type `expected` the context needs.
    /// The runtime checks the array's rank against the axes the subscript keeps.
    fn netcdf_read(
        &mut self,
        node: NodeId,
        value: NodeId,
        index: NodeId,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        let Some(items) = self.netcdf_items(index) else {
            return self.lowerer.unknown();
        };
        let array = match expected.map(|ty| (ty, ty.kind(self.db))) {
            Some((ty, TypeKind::Array { element, .. })) if self.is_netcdf_element(*element) => ty,
            Some((ty, TypeKind::Array { .. })) => {
                let message = format!(
                    "a NetCDF variable cannot be read into an `{}`",
                    self.show(ty)
                );
                return self.lowerer.error(
                    node,
                    &message,
                    "not a type a NetCDF variable holds",
                    Some(
                        "a classic NetCDF file holds signed integers and floats, such as \
                         `Array[f32, 3]`",
                    ),
                );
            }
            _ => {
                let element = Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64)));
                Type::new(self.db, TypeKind::Array { element, rank: 2 })
            }
        };
        let call = Call {
            callee: Callee::Builtin("netCDF4.read".to_owned()),
            type_arguments: Vec::new(),
            arguments: std::iter::once(value)
                .chain(items.iter().copied())
                .map(|node| Some(Key(node)))
                .collect(),
        };
        self.typed.calls.insert(Key(node), call);
        let ty = self.raises_library_errors(node, array, subscript_errors(&items));
        self.raised_value(node, ty, "reading a NetCDF variable can fail with")
    }

    /// The items of the subscript's `index` that pick part of a variable, `var[0, :, ::2]`,
    /// each an integer or a slice of integers, as `netCDF4` takes them: none for `:`, the whole
    /// variable, which the runtime reads or writes without a selection. `None`, after reporting
    /// it, when an item is anything else, such as an array of indices `netCDF4` would also take.
    pub(super) fn netcdf_items(&mut self, index: NodeId) -> Option<Vec<NodeId>> {
        self.subscript_items(
            index,
            "a NetCDF variable",
            "for example `var[0, :, ::2]`; other indexes come later under decision 0047",
        )
    }

    /// The items of the subscript's `index` that pick part of `what`, a variable or a
    /// dataset, each an integer or a slice of integers: none for `:`, which picks the whole.
    /// `None`, after reporting it with the `help` that shows what is taken, when an item is
    /// anything else.
    pub(super) fn subscript_items(
        &mut self,
        index: NodeId,
        what: &str,
        help: &str,
    ) -> Option<Vec<NodeId>> {
        let whole = self.tree.kind(index) == SyntaxKind::Slice
            && self.tree.children(index).next().is_none();
        if whole {
            return Some(Vec::new());
        }
        let items: Vec<NodeId> = if self.tree.kind(index) == SyntaxKind::Tuple
            && !self.tree.node(index).flags().contains(Flags::PARENTHESIZED)
        {
            self.tree.children_with(index, Role::Item).collect()
        } else {
            vec![index]
        };
        let int = Type::new(
            self.db,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        let mut integers = true;
        for &item in &items {
            let parts: Vec<NodeId> = if self.tree.kind(item) == SyntaxKind::Slice {
                self.tree.children(item).collect()
            } else {
                vec![item]
            };
            for part in parts {
                let ty = self.expression(part, Some(int));
                if matches!(
                    ty.kind(self.db),
                    TypeKind::Primitive(Primitive::Integer(_)) | TypeKind::Unknown
                ) {
                    continue;
                }
                integers = false;
                let message = format!(
                    "{what} is indexed by integers and slices, found `{}`",
                    self.show(ty)
                );
                self.lowerer
                    .error(part, &message, "not an integer", Some(help));
            }
        }
        integers.then_some(items)
    }

    /// Whether a classic `NetCDF` file's values can be read as elements of type `ty`: a signed
    /// integer or a float.
    pub(super) fn is_netcdf_element(&self, ty: Type<'db>) -> bool {
        matches!(
            ty.kind(self.db),
            TypeKind::Primitive(
                Primitive::Integer(Integer::I8 | Integer::I16 | Integer::I32 | Integer::I64)
                    | Primitive::Float(_)
            )
        )
    }

    /// The call `node` of the method `method` of the value `value` of type `base`, when `base` is
    /// a `Dataset`, a `Variable` or a `Dimension` and `method` one of theirs; `expected` types an
    /// attribute. `None` for any other method.
    pub(super) fn netcdf_method(
        &mut self,
        node: NodeId,
        (value, base): (NodeId, Type<'db>),
        method: &str,
        arguments: &Arguments,
        expected: Option<Type<'db>>,
    ) -> Option<Type<'db>> {
        if method == "_apply" {
            return self.netcdf_apply(node, (value, base), arguments);
        }
        let (class, prefix) = if self.is_netcdf(base, "Dataset") {
            ("Dataset", "")
        } else if self.is_netcdf(base, "Variable") {
            ("Variable", "_variable")
        } else if self.is_netcdf(base, "Dimension") {
            ("Dimension", "")
        } else {
            return None;
        };
        if let Some(ty) =
            self.netcdf_definition(node, (value, base), (class, prefix), method, arguments)
        {
            return Some(ty);
        }
        let function = match (class, method) {
            ("Dataset" | "Variable", "getncattr") => {
                let Some(kind) = expected.and_then(|ty| self.attribute_kind(ty)) else {
                    self.untyped(arguments);
                    return Some(self.lowerer.error(
                        node,
                        "the type of a NetCDF attribute comes from where it goes",
                        "type not known",
                        Some(
                            "give it a type, such as `units: str = var.getncattr(\"units\")`; \
                             an attribute is read as a `str`, an `int` or a `float` \
                             (decision 0047)",
                        ),
                    ));
                };
                format!("{prefix}_get_{kind}")
            }
            ("Dataset" | "Variable", "ncattrs") => format!("{prefix}_ncattrs"),
            ("Dataset", "close") => "_close".to_owned(),
            ("Dimension", "isunlimited") => "_isunlimited".to_owned(),
            _ => return None,
        };
        Some(self.library_method(
            node,
            ("netCDF4", class, &function),
            (value, base),
            arguments,
        ))
    }

    /// The name the library's functions give an attribute read as `ty`: `str`, `int` or
    /// `float`, or `None` for a type an attribute is not read as.
    pub(super) fn attribute_kind(&self, ty: Type<'db>) -> Option<&'static str> {
        match ty.kind(self.db) {
            TypeKind::Str => Some("str"),
            TypeKind::Primitive(Primitive::Integer(Integer::I64)) => Some("int"),
            TypeKind::Primitive(Primitive::Float(Float::F64)) => Some("float"),
            _ => None,
        }
    }

    /// `ds.variables` or `ds.dimensions` read as a value, the attribute `node` of the
    /// `Dataset` of type `base`: an error that says how to reach one by name. `None` for any
    /// other attribute.
    pub(super) fn netcdf_attribute(
        &mut self,
        node: NodeId,
        base: Type<'db>,
        name: &str,
    ) -> Option<Type<'db>> {
        if !self.is_netcdf(base, "Dataset") || !matches!(name, "variables" | "dimensions") {
            return None;
        }
        let message = format!("`ds.{name}` is read by name, as `ds.{name}[\"time\"]`, so far");
        Some(self.lowerer.error(
            node,
            &message,
            "not built yet",
            Some("listing a file's variables and dimensions comes later under decision 0047"),
        ))
    }

    /// The value type of `ty`, a `Raises[value, E]` that the subscript `node` gives, after
    /// recording that `node` can fail with `E`, which `what` describes.
    pub(super) fn raised_value(&mut self, node: NodeId, ty: Type<'db>, what: &str) -> Type<'db> {
        self.fallible_at(node, ty, what);
        match ty.kind(self.db) {
            TypeKind::Fallible { value, .. } => *value,
            _ => ty,
        }
    }

    /// Reports that the library's `netCDF4` class `name` is missing at `node`, which happens
    /// only when the library's files were left out of the package.
    fn missing_netcdf_class(&mut self, node: NodeId, name: &str) -> Type<'db> {
        let message = format!("`netCDF4.{name}` is missing from the standard library");
        self.lowerer.error(
            node,
            &message,
            "cannot be typed",
            Some("the compiler adds the library's files to every package (decision 0038)"),
        )
    }
}

/// The error classes a read or write of a variable through `items` can fail with, in the order
/// the runtime numbers them: `OSError` alone for `var[:]`, which picks every value, and also
/// `IndexError` for an index out of bounds and `ValueError` for a zero step, as `netCDF4`
/// raises them, when the subscript has items.
pub(super) fn subscript_errors(items: &[NodeId]) -> &'static [&'static str] {
    if items.is_empty() {
        &["OSError"]
    } else {
        &["OSError", "IndexError", "ValueError"]
    }
}
