//! Writing `NetCDF` classic files with the `netCDF4` module (decision 0047), with Unidata's
//! names: `ds.createDimension(dimname, size)`, `ds.createVariable(varname, datatype,
//! dimensions)`, `setncattr(name, value)` of a dataset or a variable, and `var[:] = values`.
//!
//! The methods are calls of functions of `<std>/netCDF4.byn`, recorded where the program wrote
//! them, as the reads are. Where `netCDF4` takes a value of more than one type, the checker
//! picks the function by the argument: `createDimension("time", None)` is
//! `_create_unlimited(ds, "time")`, a tuple of two dimensions is `_create_variable_2`, and an
//! attribute's value picks `_set_str`, `_set_int` or `_set_float`. Each changes the file at once
//! through `path._apply(line)`, a built-in call that only the library's own functions can make.
//! `var[:] = values` is the other built-in call: it writes an array of numbers as all the
//! variable's values.

use super::Body;
use super::calls::Arguments;
use syntax::{NodeId, Role, SyntaxKind};
use types::{Call, Callee, Key, Type, TypeKind};

/// The most dimensions `createVariable` takes, one library function for each count.
const MOST_DIMENSIONS: usize = 4;

impl<'db> Body<'db> {
    /// `ds.createDimension`, `ds.createVariable` or `setncattr`, the call `node` of `method` of
    /// the value `value` of type `base`, the library's `netCDF4` class `class`; `prefix` starts
    /// the names of a `Variable`'s functions. `None` for any other method.
    pub(super) fn netcdf_definition(
        &mut self,
        node: NodeId,
        (value, base): (NodeId, Type<'db>),
        (class, prefix): (&str, &str),
        method: &str,
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let (function, arguments) = match (class, method) {
            ("Dataset", "createDimension") => self.dimension_function(arguments),
            ("Dataset", "createVariable") => match self.variable_function(node, arguments) {
                Some(function) => function,
                None => return Some(self.lowerer.unknown()),
            },
            ("Dataset" | "Variable", "setncattr") => match self.attribute_function(node, arguments)
            {
                Some((kind, arguments)) => (format!("{prefix}_set_{kind}"), arguments),
                None => return Some(self.lowerer.unknown()),
            },
            _ => return None,
        };
        Some(self.library_method(
            node,
            ("netCDF4", class, &function),
            (value, base),
            &arguments,
        ))
    }

    /// The library function for `createDimension` with `arguments`, and the arguments it takes:
    /// `_create_unlimited` without the size when it is `None` or left out, as `netCDF4`'s
    /// default makes the unlimited dimension, else `_create_dimension`.
    fn dimension_function(&self, arguments: &Arguments) -> (String, Arguments) {
        let mut arguments = Arguments {
            positional: arguments.positional.clone(),
            keywords: arguments.keywords.clone(),
        };
        let unlimited = |body: &Self, node: NodeId| body.tree.kind(node) == SyntaxKind::None;
        let size = arguments.positional.get(1).copied().or_else(|| {
            arguments
                .keywords
                .iter()
                .find(|&&(name, _)| self.tree.text(name, self.text) == "size")
                .map(|&(_, value)| value)
        });
        match size {
            Some(size) if !unlimited(self, size) => ("_create_dimension".to_owned(), arguments),
            _ => {
                arguments.positional.retain(|&node| Some(node) != size);
                arguments.keywords.retain(|&(_, node)| Some(node) != size);
                ("_create_unlimited".to_owned(), arguments)
            }
        }
    }

    /// The library function for `createVariable` with `arguments`, and the arguments it takes:
    /// `_create_variable_n` for `n` dimensions, each its own argument. A tuple of names is
    /// spread out, and a single name stands for a tuple of one. `None`, after reporting it, for
    /// more dimensions than the library takes or an argument `netCDF4` has that is not built.
    fn variable_function(
        &mut self,
        node: NodeId,
        arguments: &Arguments,
    ) -> Option<(String, Arguments)> {
        let mut positional: Vec<NodeId> = arguments.positional.iter().copied().take(2).collect();
        let mut dimensions = arguments.positional.get(2).copied();
        let mut wrong = arguments.positional.len() > 3;
        for &(name, value) in &arguments.keywords {
            match self.tree.text(name, self.text) {
                "varname" | "datatype" => positional.push(value),
                "dimensions" => wrong |= dimensions.replace(value).is_some(),
                _ => wrong = true,
            }
        }
        let names: Vec<NodeId> = match dimensions {
            Some(tuple) if self.tree.kind(tuple) == SyntaxKind::Tuple => {
                self.tree.children_with(tuple, Role::Item).collect()
            }
            Some(name) => vec![name],
            None => Vec::new(),
        };
        if wrong || positional.len() != 2 || names.len() > MOST_DIMENSIONS {
            self.untyped(arguments);
            let message = if names.len() > MOST_DIMENSIONS {
                format!("a NetCDF variable has at most {MOST_DIMENSIONS} dimensions, so far")
            } else {
                "`createVariable` takes `varname`, `datatype` and `dimensions`, so far".to_owned()
            };
            self.lowerer.error(
                node,
                &message,
                "not built yet",
                Some(
                    "for example `ds.createVariable(\"temp\", \"f4\", (\"time\", \"lat\"))`; \
                     compression and fill values come later under decision 0047",
                ),
            );
            return None;
        }
        let function = format!("_create_variable_{}", names.len());
        positional.extend(names);
        let arguments = Arguments {
            positional,
            keywords: Vec::new(),
        };
        Some((function, arguments))
    }

    /// The kind of the value `setncattr` with `arguments` sets, `str`, `int` or `float`, which
    /// names its library function, and the arguments it takes. `None`, after reporting it, for
    /// a value of another type.
    fn attribute_function(
        &mut self,
        node: NodeId,
        arguments: &Arguments,
    ) -> Option<(&'static str, Arguments)> {
        let value = arguments.positional.get(1).copied().or_else(|| {
            arguments
                .keywords
                .iter()
                .find(|&&(name, _)| self.tree.text(name, self.text) == "value")
                .map(|&(_, value)| value)
        });
        let Some(value) = value else {
            self.untyped(arguments);
            self.lowerer.error(
                node,
                "`setncattr` takes `name` and `value`",
                "wrong arguments",
                Some("for example `var.setncattr(\"units\", \"K\")`"),
            );
            return None;
        };
        let found = self.expression(value, None);
        let Some(kind) = self.attribute_kind(found) else {
            let message = format!(
                "a NetCDF attribute is a `str`, an `int` or a `float`, found `{}`",
                self.show(found)
            );
            self.untyped(arguments);
            self.lowerer
                .error(value, &message, "not an attribute's type", None);
            return None;
        };
        let arguments = Arguments {
            positional: arguments.positional.clone(),
            keywords: arguments.keywords.clone(),
        };
        Some((kind, arguments))
    }

    /// `path._apply(line)`, the call `node` of the `str` `path` with `arguments`, inside
    /// `<std>/netCDF4.byn` only: applies the change `line` to the file at `path` and gives the
    /// lines that describe its new header, a `list[str]`, or fails with `OSError`. `None`
    /// anywhere else, where `_apply` is no method.
    pub(super) fn netcdf_apply(
        &mut self,
        node: NodeId,
        (path, base): (NodeId, Type<'db>),
        arguments: &Arguments,
    ) -> Option<Type<'db>> {
        let library = database::modules(self.db, self.lowerer.package).library("netCDF4");
        let str = Type::new(self.db, TypeKind::Str);
        if library != Some(self.lowerer.file) || base != str {
            return None;
        }
        let &[line] = &arguments.positional[..] else {
            return Some(self.untyped(arguments));
        };
        let found = self.expression(line, Some(str));
        self.expect(line, str, found, "a change is");
        let call = Call {
            callee: Callee::Builtin("netCDF4.apply".to_owned()),
            type_arguments: Vec::new(),
            arguments: vec![Some(Key(path)), Some(Key(line))],
        };
        self.typed.calls.insert(Key(node), call);
        let lines = Type::new(self.db, TypeKind::List(str));
        Some(self.raises_library_error(node, lines, "OSError"))
    }

    /// `var[:] = values`, the assignment of `value` to the subscript `target`, when it names a
    /// `Variable`; returns whether it did. `values` is an array of numbers, whose rank and
    /// lengths the runtime checks against the variable's.
    pub(super) fn netcdf_store(&mut self, target: NodeId, value: NodeId) -> bool {
        let (Some(variable), Some(index)) = (
            self.tree.child(target, Role::Value),
            self.tree.child(target, Role::Index),
        ) else {
            return false;
        };
        if self.tree.kind(target) != SyntaxKind::Subscript
            || !self.names_netcdf(variable, "Variable")
        {
            return false;
        }
        self.expression(variable, None);
        if !self.is_whole(index) {
            self.expression(value, None);
            self.lowerer.error(
                index,
                "a NetCDF variable is written whole, as `var[:] = values`, so far",
                "not built yet",
                Some("slices of a variable come later under decision 0047"),
            );
            return true;
        }
        let found = self.expression(value, None);
        let element = match found.kind(self.db) {
            TypeKind::Array { element, .. } => Some(*element),
            // Already reported where the value went wrong.
            TypeKind::Unknown => return true,
            _ => None,
        };
        if !element.is_some_and(|element| self.is_netcdf_element(element)) {
            let message = format!(
                "a NetCDF variable is written from an array of numbers, found `{}`",
                self.show(found)
            );
            self.lowerer.error(
                value,
                &message,
                "not an array a NetCDF variable takes",
                Some("write an array of signed integers or floats, such as `Array[f32, 3]`"),
            );
            return true;
        }
        let call = Call {
            callee: Callee::Builtin("netCDF4.write".to_owned()),
            type_arguments: Vec::new(),
            arguments: vec![Some(Key(variable)), Some(Key(value))],
        };
        self.typed.calls.insert(Key(target), call);
        let nothing = Type::new(self.db, TypeKind::None);
        let ty = self.raises_library_error(target, nothing, "OSError");
        self.fallible_at(target, ty, "writing a NetCDF variable can fail with");
        true
    }
}
