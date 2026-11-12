//! The `fits` module's calls (decision 0044): `fits.getdata(filename, ext)`,
//! `fits.getheader(filename, ext)` and `fits.writeto(filename, data, header, overwrite)`, with
//! `AstroPy`'s names and argument order.
//!
//! Each can fail with `OSError`. `getdata` gives an `Array[T, rank]` whose element type and
//! rank come from the context, as `Array.load`'s do, else `Array[f64, 2]`; the runtime checks
//! the image against them. A FITS image holds integers or floats, so `T` is one of those.
//! `getheader` gives the library's `Header`, whose values the `fits_header` module reads.

use super::Body;
use super::calls::Arguments;
use syntax::{NodeId, SyntaxKind};
use types::{Call, Callee, Float, Integer, Key, Primitive, Type, TypeKind};

/// `AstroPy`'s keywords of `getdata`, `getheader` and `writeto` that do not apply yet: a
/// `(data, header)` pair, scaling options, memory maps and checksums.
const NOT_TAKEN: [&str; 10] = [
    "header",
    "lower",
    "upper",
    "view",
    "memmap",
    "do_not_scale_image_data",
    "uint",
    "output_verify",
    "checksum",
    "extname",
];

impl<'db> Body<'db> {
    /// `fits.member(arguments)`. `expected` is the type the context needs, which gives the
    /// element type and the rank of `getdata`'s array.
    pub(super) fn fits_call(
        &mut self,
        node: NodeId,
        member: &str,
        arguments: &Arguments,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        match member {
            "getdata" => self.fits_getdata(node, arguments, expected),
            "getheader" => self.fits_getheader(node, arguments),
            "writeto" => self.fits_writeto(node, arguments),
            _ => {
                self.untyped(arguments);
                let message = format!("the `fits` module has no function `{member}` yet");
                self.lowerer.error(
                    node,
                    &message,
                    "not built yet",
                    Some(
                        "decision 0044 builds `getdata`, `getheader`, `writeto` and `Header`; \
                         `fits.open` and tables are roadmap 4.2.5's",
                    ),
                )
            }
        }
    }

    /// `fits.getdata(filename, ext=0)`.
    fn fits_getdata(
        &mut self,
        node: NodeId,
        arguments: &Arguments,
        expected: Option<Type<'db>>,
    ) -> Type<'db> {
        let names = ["filename", "ext"];
        let Some(given) = self.fits_arguments(node, arguments, &names, 1, "fits.getdata") else {
            return self.lowerer.unknown();
        };
        self.fits_path_and_ext(&given);
        let array = match expected.map(|ty| (ty, ty.kind(self.db))) {
            Some((ty, TypeKind::Array { element, .. })) if self.is_fits_element(*element) => ty,
            Some((ty, TypeKind::Array { .. })) => {
                let message = format!(
                    "`fits.getdata` cannot read a FITS image into an `{}`",
                    self.show(ty)
                );
                return self.lowerer.error(
                    node,
                    &message,
                    "not a type a FITS image holds",
                    Some("a FITS image holds integers or floats, such as `Array[f32, 2]`"),
                );
            }
            _ => {
                let element = Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64)));
                Type::new(self.db, TypeKind::Array { element, rank: 2 })
            }
        };
        self.record_fits(node, "fits.getdata", given);
        self.raises_library_error(node, array, "OSError")
    }

    /// `fits.getheader(filename, ext=0)`.
    fn fits_getheader(&mut self, node: NodeId, arguments: &Arguments) -> Type<'db> {
        let names = ["filename", "ext"];
        let Some(given) = self.fits_arguments(node, arguments, &names, 1, "fits.getheader") else {
            return self.lowerer.unknown();
        };
        self.fits_path_and_ext(&given);
        let Some(header) = self.library_class_in("fits", "Header") else {
            return self.lowerer.error(
                node,
                "`fits.Header` is missing from the standard library",
                "cannot be typed",
                Some("the compiler adds the library's files to every package (decision 0038)"),
            );
        };
        self.record_fits(node, "fits.getheader", given);
        self.raises_library_error(node, header, "OSError")
    }

    /// `fits.writeto(filename, data, header=None, overwrite=False)`. The recorded arguments are
    /// the path, the array, `overwrite` and the header, in that order, each `None` when left
    /// out; a `header=None` written out is left out.
    fn fits_writeto(&mut self, node: NodeId, arguments: &Arguments) -> Type<'db> {
        let names = ["filename", "data", "header", "overwrite"];
        let Some(given) = self.fits_arguments(node, arguments, &names, 2, "fits.writeto") else {
            return self.lowerer.unknown();
        };
        let [path, data, header, overwrite] = given[..] else {
            return self.lowerer.unknown();
        };
        let str = Type::new(self.db, TypeKind::Str);
        if let Some(path) = path {
            let found = self.expression(path, Some(str));
            self.expect(path, str, found, "a path is");
        }
        let found = data.map(|data| (data, self.expression(data, None)));
        let header = header.filter(|&header| self.tree.kind(header) != SyntaxKind::None);
        if let Some(header) = header {
            let found = self.expression(header, None);
            if let Some(expected) = self.library_class_in("fits", "Header") {
                self.expect(header, expected, found, "a header is");
            }
        }
        if let Some(overwrite) = overwrite {
            let bool = Type::new(self.db, TypeKind::Primitive(Primitive::Bool));
            let found = self.expression(overwrite, Some(bool));
            self.expect(overwrite, bool, found, "`overwrite` is");
        }
        if let Some((data, found)) = found {
            match found.kind(self.db) {
                TypeKind::Array { element, .. } if self.is_fits_element(*element) => {}
                TypeKind::Unknown => return self.lowerer.unknown(),
                _ => {
                    let message = format!(
                        "`fits.writeto` writes an `Array` of integers or floats, found `{}`",
                        self.show(found)
                    );
                    return self.lowerer.error(
                        data,
                        &message,
                        "cannot be written as a FITS image",
                        Some("a FITS image holds integers or floats, such as `Array[f32, 2]`"),
                    );
                }
            }
        }
        self.record_fits(node, "fits.writeto", vec![path, data, overwrite, header]);
        let nothing = Type::new(self.db, TypeKind::None);
        self.raises_library_error(node, nothing, "OSError")
    }

    /// Types the path and the HDU number of `getdata` or `getheader`, given in that order.
    fn fits_path_and_ext(&mut self, given: &[Option<NodeId>]) {
        let str = Type::new(self.db, TypeKind::Str);
        if let Some(&Some(path)) = given.first() {
            let found = self.expression(path, Some(str));
            self.expect(path, str, found, "a path is");
        }
        if let Some(&Some(ext)) = given.get(1) {
            let int = Type::new(
                self.db,
                TypeKind::Primitive(Primitive::Integer(Integer::I64)),
            );
            let found = self.expression(ext, Some(int));
            self.expect(ext, int, found, "an HDU number is");
        }
    }

    /// The arguments of `callee`, named `names` as `AstroPy` names them, each by position or by
    /// keyword, the first `required` of them needed: each given argument in `names`'s order,
    /// `None` for one left out. `None`, with the error reported, when one is missing, given
    /// twice or unknown.
    fn fits_arguments(
        &mut self,
        node: NodeId,
        arguments: &Arguments,
        names: &[&str],
        required: usize,
        callee: &str,
    ) -> Option<Vec<Option<NodeId>>> {
        let mut slots: Vec<Option<NodeId>> = vec![None; names.len()];
        let mut wrong = arguments.positional.len() > names.len();
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
        wrong |= slots[..required].iter().any(Option::is_none);
        if !wrong {
            return Some(slots);
        }
        self.untyped(arguments);
        if let Some((name, written)) = not_taken {
            let message = format!("`{callee}` does not take `{written}` yet");
            self.lowerer.error(
                name,
                &message,
                "not built yet",
                Some(
                    "decision 0044 builds images and headers; `(data, header)` pairs, memory \
                     maps, scaling options and checksums come later",
                ),
            );
        } else {
            let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
            let message = format!("`{callee}` takes {}", quoted.join(", "));
            let hint = if names.len() == 2 {
                format!("for example `{callee}(\"image.fits\")`")
            } else {
                "for example `fits.writeto(\"image.fits\", image, header, overwrite=True)`"
                    .to_owned()
            };
            self.lowerer
                .error(node, &message, "wrong arguments", Some(&hint));
        }
        None
    }

    /// Whether a FITS image can hold elements of type `ty`: an integer or a float.
    fn is_fits_element(&self, ty: Type<'db>) -> bool {
        matches!(
            ty.kind(self.db),
            TypeKind::Primitive(Primitive::Integer(_) | Primitive::Float(_))
        )
    }

    /// Records the call `node` of the built-in `name` with `arguments`, `None` for one left
    /// out.
    fn record_fits(&mut self, node: NodeId, name: &str, arguments: Vec<Option<NodeId>>) {
        let call = Call {
            callee: Callee::Builtin(name.to_owned()),
            type_arguments: Vec::new(),
            arguments: arguments
                .into_iter()
                .map(|argument| argument.map(Key))
                .collect(),
        };
        self.typed.calls.insert(Key(node), call);
    }
}
