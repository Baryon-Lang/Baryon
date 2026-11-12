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

use super::Body;
use super::calls::Arguments;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Call, Callee, Float, Integer, Key, Primitive, Type, TypeKind};

impl<'db> Body<'db> {
    /// `netCDF4.member(arguments)`: `Dataset(filename, mode="r")`, a `Dataset` that can fail
    /// with `OSError`. The recorded argument is the path.
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
        let mut wrong = arguments.positional.len() > 2;
        for &(name, value) in &arguments.keywords {
            let slot = match self.tree.text(name, self.text) {
                "filename" => &mut path,
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
                "`Dataset` takes `filename` and `mode`",
                "wrong arguments",
                Some("for example `Dataset(\"air.nc\")` or `Dataset(\"air.nc\", \"r\")`"),
            );
        };
        let found = self.expression(path, Some(str));
        self.expect(path, str, found, "a path is");
        if let Some(mode) = mode {
            let found = self.expression(mode, Some(str));
            self.expect(mode, str, found, "a mode is");
            let text = (self.tree.kind(mode) == SyntaxKind::StringLiteral)
                .then(|| self.tree.text(mode, self.text));
            if !matches!(text, Some("\"r\"" | "'r'")) {
                return self.lowerer.error(
                    mode,
                    "`Dataset` opens a file for reading only, so far",
                    "not built yet",
                    Some(
                        "write `\"r\"` or leave the mode out; writing NetCDF files comes next \
                         under decision 0047",
                    ),
                );
            }
        }
        let Some(dataset) = self.library_class_in("netCDF4", "Dataset") else {
            return self.missing_netcdf_class(node, "Dataset");
        };
        let call = Call {
            callee: Callee::Builtin("netCDF4.Dataset".to_owned()),
            type_arguments: Vec::new(),
            arguments: vec![Some(Key(path))],
        };
        self.typed.calls.insert(Key(node), call);
        self.raises_library_error(node, dataset, "OSError")
    }

    /// Whether the class `name` of `file` is the library's `netCDF4.Dataset`, which a program
    /// calls to open a file rather than to build one field by field.
    pub(super) fn is_dataset_class(&self, file: database::File, name: NodeId) -> bool {
        self.library_class_in("netCDF4", "Dataset").is_some_and(|dataset| {
            matches!(dataset.kind(self.db), TypeKind::Struct(nominal) if nominal.file == file && nominal.name == name)
        })
    }

    /// Whether `ty` is the library's `netCDF4` class `name`.
    fn is_netcdf(&self, ty: Type<'db>, name: &str) -> bool {
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
    fn names_netcdf(&self, node: NodeId, class: &str) -> bool {
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

    /// `var[:]`, the subscript `node` of the `Variable` `value` with the index `index`: every
    /// value of the variable, as an array of the type `expected` the context needs.
    fn netcdf_read(
        &mut self,
        node: NodeId,
        value: NodeId,
        index: NodeId,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        let whole = self.tree.kind(index) == SyntaxKind::Slice
            && self.tree.child(index, Role::Lower).is_none()
            && self.tree.child(index, Role::Upper).is_none()
            && self.tree.child(index, Role::Step).is_none();
        if !whole {
            return self.lowerer.error(
                index,
                "a NetCDF variable is read whole, as `var[:]`, so far",
                "not built yet",
                Some("slices of a variable come later under decision 0047; read it whole, then slice the array"),
            );
        }
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
            arguments: vec![Some(Key(value))],
        };
        self.typed.calls.insert(Key(node), call);
        let ty = self.raises_library_error(node, array, "OSError");
        self.raised_value(node, ty, "reading a NetCDF variable can fail with")
    }

    /// Whether a classic `NetCDF` file's values can be read as elements of type `ty`: a signed
    /// integer or a float.
    fn is_netcdf_element(&self, ty: Type<'db>) -> bool {
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
        let (class, prefix) = if self.is_netcdf(base, "Dataset") {
            ("Dataset", "")
        } else if self.is_netcdf(base, "Variable") {
            ("Variable", "_variable")
        } else if self.is_netcdf(base, "Dimension") {
            ("Dimension", "")
        } else {
            return None;
        };
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
    fn attribute_kind(&self, ty: Type<'db>) -> Option<&'static str> {
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
    fn raised_value(&mut self, node: NodeId, ty: Type<'db>, what: &str) -> Type<'db> {
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
