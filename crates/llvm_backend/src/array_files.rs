//! Array files: `Array.fromfile` and `a.tofile` (decision 0036, part 4), `Array.load` and
//! `Array.save` of `NumPy`'s `.npy` files (decision 0043), and `fits.getheader`,
//! `fits.getdata` and `fits.writeto` of FITS files (decision 0044), and the `netCDF4`
//! module's reads and writes of `NetCDF` classic files (decision 0047), each a call of the
//! runtime that can fail with `OSError`.
//!
//! Both raise as a file call does (the `files` module): the last parameter is the address of the
//! error edge's local, and the `i1` result is true when the call failed.
//!
//! | Method | Runtime call |
//! | --- | --- |
//! | [`FromFile`](mir::ArrayFile::FromFile) | `baryon_rt_array_from_file(path, length, element, count, offset, out, error)` |
//! | [`ToFile`](mir::ArrayFile::ToFile) | `baryon_rt_array_to_file(path, length, array, rank, element_size, error)` |
//! | [`Load`](mir::ArrayFile::Load) | `baryon_rt_array_load(path, length, element, descr, descr_length, rank, out, error)` |
//! | [`Save`](mir::ArrayFile::Save) | `baryon_rt_array_save(path, length, array, rank, element_size, descr, descr_length, error)` |
//! | [`FitsHeader`](mir::ArrayFile::FitsHeader) | `baryon_rt_fits_cards(path, length, ext, out, error)` |
//! | [`FitsData`](mir::ArrayFile::FitsData) | `baryon_rt_fits_read(path, length, ext, element, rank, out, error)` |
//! | [`FitsWrite`](mir::ArrayFile::FitsWrite) | `baryon_rt_fits_write(path, length, array, rank, element, cards, card_count, header, overwrite, error)` |
//! | [`NetcdfOpen`](mir::ArrayFile::NetcdfOpen) | `baryon_rt_netcdf_open(path, length, out, error)` |
//! | [`NetcdfRead`](mir::ArrayFile::NetcdfRead) | `baryon_rt_netcdf_read(path, length, name, name_length, items, item_count, element, rank, out, error)` |
//! | [`NetcdfDefine`](mir::ArrayFile::NetcdfDefine) | `baryon_rt_netcdf_define(path, length, line, line_length, out, error)` |
//! | [`NetcdfWrite`](mir::ArrayFile::NetcdfWrite) | `baryon_rt_netcdf_write(path, length, name, name_length, items, item_count, array, rank, element, error)` |
//!
//! A read writes the header of the new rank-1 array through `out`, the destination's address.
//! `element` is the address of a stack slot holding the runtime's `ElementLayout`,
//! `{ i64 size, i64 align, i64 bool_count, ptr bools }`: the element's size and alignment, and
//! the offset of every `bool` in it, which the runtime makes 0 or 1 whatever byte the file
//! holds. A write passes the address of the array's header, which holds its pointer, lengths and
//! strides, so a view writes the elements it sees. A `.npy` call also passes the element type
//! in `NumPy`'s spelling, such as `<f8`, a constant this module works out from the type; a load
//! writes the header of a new array of the destination's rank. A FITS call passes the element
//! type as the runtime's `ELEMENT_*` number, and a write passes the program's header cards, the
//! data and length of a `list[str]`, with a flag that says whether there was a header. A
//! `NetCDF` read or write passes the element type as a FITS call does, with the variable's
//! name and the items of its subscript, four `i64`s each in a stack slot, and a `NetCDF`
//! change passes its line of text.

use crate::Unsupported;
use crate::function::{Function, block_label};
use mir::{Argument, ArrayFile, BlockId, ErrorEdge, Place};
use types::{Complex, Declared, Float, Integer, Layout, Primitive, Type, TypeKind};

impl<'db> Function<'_, '_, 'db> {
    /// Writes a call of the array file operation `method`, storing its result at `destination`
    /// and going to `target`, or to the error edge's block with the `OSError` in its local.
    pub(crate) fn array_file_call(
        &mut self,
        method: ArrayFile,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
        error: Option<&ErrorEdge>,
    ) -> Result<(), Unsupported> {
        let name = method.name();
        let edge = error.ok_or_else(|| Unsupported::new(format!("`{name}` with no error edge")))?;
        let (symbol, mut parameters) = match (method, arguments) {
            (ArrayFile::FromFile, [path, Argument::Owned(count), Argument::Owned(offset)]) => {
                let (bytes, length) = self.text_argument(path)?;
                let (count, _) = self.value(count)?;
                let (offset, _) = self.value(offset)?;
                let (out, ty) = self.place(destination)?;
                let (element, rank) = self.array(ty)?;
                if rank != 1 {
                    return Err(Unsupported::new(format!(
                        "`{name}` of an array of rank {rank}"
                    )));
                }
                let element = self.element_layout(element)?;
                let parameters = vec![
                    format!("ptr {bytes}"),
                    format!("i64 {length}"),
                    format!("ptr {element}"),
                    format!("i64 {count}"),
                    format!("i64 {offset}"),
                    format!("ptr {out}"),
                ];
                ("baryon_rt_array_from_file", parameters)
            }
            (ArrayFile::ToFile, [Argument::Borrow(array), path]) => {
                let (address, ty) = self.place(array)?;
                let (element, rank) = self.array(ty)?;
                let size = self.layout(element)?.size();
                let (bytes, length) = self.text_argument(path)?;
                let parameters = vec![
                    format!("ptr {bytes}"),
                    format!("i64 {length}"),
                    format!("ptr {address}"),
                    format!("i64 {rank}"),
                    format!("i64 {size}"),
                ];
                ("baryon_rt_array_to_file", parameters)
            }
            (ArrayFile::Load | ArrayFile::Save, _) => {
                self.npy_parameters(method, arguments, destination)?
            }
            (ArrayFile::FitsHeader | ArrayFile::FitsData | ArrayFile::FitsWrite, _) => {
                self.fits_parameters(method, arguments, destination)?
            }
            (
                ArrayFile::NetcdfOpen
                | ArrayFile::NetcdfRead
                | ArrayFile::NetcdfDefine
                | ArrayFile::NetcdfWrite,
                _,
            ) => self.netcdf_parameters(method, arguments, destination)?,
            _ => return Err(Unsupported::new(format!("`{name}` with these arguments"))),
        };
        let (address, _) = self.place(&edge.error.into())?;
        parameters.push(format!("ptr {address}"));
        let types: Vec<&str> = parameters
            .iter()
            .map(|parameter| parameter.rsplit_once(' ').map_or("", |(ty, _)| ty))
            .collect();
        self.declare(
            symbol,
            &format!(
                "declare zeroext i1 @{symbol}({}) nounwind",
                types.join(", ")
            ),
        );
        let failed = self.emit(&format!(
            "call zeroext i1 @{symbol}({})",
            parameters.join(", ")
        ));
        self.line(&format!(
            "br i1 {failed}, label %{}, label %{}",
            block_label(edge.target),
            block_label(target)
        ));
        Ok(())
    }

    /// The layout of a value of type `ty`.
    fn layout(&self, ty: Type<'db>) -> Result<Layout, Unsupported> {
        types::layout(self.types.db, ty, self.types.declared)
            .map_err(|_| self.types.unsupported(ty))
    }

    /// A stack slot holding the runtime's `ElementLayout` of the element type `element`, and
    /// its address: the size, the alignment, and the offsets of its `bool`s in a slot of their
    /// own.
    fn element_layout(&mut self, element: Type<'db>) -> Result<String, Unsupported> {
        let layout = self.layout(element)?;
        let mut bools = Vec::new();
        self.bool_offsets(element, 0, &mut bools)?;
        let table = if bools.is_empty() {
            "null".to_owned()
        } else {
            let llvm = format!("[{} x i64]", bools.len());
            let items: Vec<String> = bools.iter().map(|at| format!("i64 {at}")).collect();
            let slot = self.slot(&llvm);
            self.line(&format!("store {llvm} [{}], ptr {slot}", items.join(", ")));
            slot
        };
        let llvm = "{ i64, i64, i64, ptr }";
        let value = self.emit(&format!(
            "insertvalue {llvm} {{ i64 {}, i64 {}, i64 {}, ptr null }}, ptr {table}, 3",
            layout.size(),
            layout.align(),
            bools.len()
        ));
        let slot = self.slot(llvm);
        self.line(&format!("store {llvm} {value}, ptr {slot}"));
        Ok(slot)
    }

    /// Adds to `bools` the offset in bytes of every `bool` in a value of type `ty` that starts
    /// `offset` bytes into an element: the type itself, each element of a `Fixed` array, and
    /// each field of a struct. Fails for a type a file cannot be read into, such as an enum,
    /// whose bytes could name no member; the checker reports those first.
    fn bool_offsets(
        &self,
        ty: Type<'db>,
        offset: u64,
        bools: &mut Vec<u64>,
    ) -> Result<(), Unsupported> {
        match ty.kind(self.types.db) {
            TypeKind::Primitive(Primitive::Bool) => bools.push(offset),
            TypeKind::Primitive(_) => {}
            TypeKind::Fixed { element, shape } => {
                let size = self.layout(*element)?.size();
                for index in 0..shape.elements() {
                    self.bool_offsets(*element, offset + index * size, bools)?;
                }
            }
            TypeKind::Struct(nominal) => {
                let Declared::Struct(fields) = (self.types.declared)(nominal) else {
                    return Err(self.reading(ty));
                };
                let layouts = fields
                    .iter()
                    .map(|&field| self.layout(field))
                    .collect::<Result<Vec<_>, _>>()?;
                let record = Layout::record(layouts).ok_or_else(|| self.types.unsupported(ty))?;
                for (&field, &at) in fields.iter().zip(record.offsets()) {
                    self.bool_offsets(field, offset + at, bools)?;
                }
            }
            _ => return Err(self.reading(ty)),
        }
        Ok(())
    }

    /// The element type `element` as `NumPy` spells it in a `.npy` header (its `dtype.str`):
    /// the byte order, `<` for little-endian or `|` for one byte, the kind, and the size in
    /// bytes. Every platform Baryon supports is little-endian. `name` is the call, for the
    /// report of a type `.npy` files cannot hold yet, such as a struct, which needs `NumPy`'s
    /// structured types (roadmap 3.25).
    fn descr(&self, element: Type<'db>, name: &str) -> Result<&'static str, Unsupported> {
        let descr = match element.kind(self.types.db) {
            TypeKind::Primitive(primitive) => match primitive {
                Primitive::Bool => "|b1",
                Primitive::Integer(integer) => match integer {
                    Integer::I8 => "|i1",
                    Integer::I16 => "<i2",
                    Integer::I32 => "<i4",
                    Integer::I64 => "<i8",
                    Integer::U8 => "|u1",
                    Integer::U16 => "<u2",
                    Integer::U32 => "<u4",
                    Integer::U64 => "<u8",
                },
                Primitive::Float(Float::F32) => "<f4",
                Primitive::Float(Float::F64) => "<f8",
                Primitive::Complex(Complex::C64) => "<c8",
                Primitive::Complex(Complex::C128) => "<c16",
            },
            _ => {
                return Err(Unsupported::new(format!(
                    "`{name}` of elements of type `{}`",
                    element.display(self.types.db)
                )));
            }
        };
        Ok(descr)
    }

    /// The runtime function a `.npy` call of `method` calls, and its parameters before the
    /// error's address, for `arguments` and the result's place `destination`.
    fn npy_parameters(
        &mut self,
        method: ArrayFile,
        arguments: &[Argument<'db>],
        destination: &Place,
    ) -> Result<(&'static str, Vec<String>), Unsupported> {
        let name = method.name();
        let call = match (method, arguments) {
            (ArrayFile::Load, [path]) => {
                let (bytes, length) = self.text_argument(path)?;
                let (out, ty) = self.place(destination)?;
                let (element, rank) = self.array(ty)?;
                let descr = self.descr(element, name)?;
                let layout = self.element_layout(element)?;
                let constant = self.constants.bytes(descr.as_bytes());
                let parameters = vec![
                    format!("ptr {bytes}"),
                    format!("i64 {length}"),
                    format!("ptr {layout}"),
                    format!("ptr {constant}"),
                    format!("i64 {}", descr.len()),
                    format!("i64 {rank}"),
                    format!("ptr {out}"),
                ];
                ("baryon_rt_array_load", parameters)
            }
            (ArrayFile::Save, [path, Argument::Borrow(array)]) => {
                let (address, ty) = self.place(array)?;
                let (element, rank) = self.array(ty)?;
                let size = self.layout(element)?.size();
                let descr = self.descr(element, name)?;
                let constant = self.constants.bytes(descr.as_bytes());
                let (bytes, length) = self.text_argument(path)?;
                let parameters = vec![
                    format!("ptr {bytes}"),
                    format!("i64 {length}"),
                    format!("ptr {address}"),
                    format!("i64 {rank}"),
                    format!("i64 {size}"),
                    format!("ptr {constant}"),
                    format!("i64 {}", descr.len()),
                ];
                ("baryon_rt_array_save", parameters)
            }
            _ => return Err(Unsupported::new(format!("`{name}` with these arguments"))),
        };
        Ok(call)
    }

    /// The runtime function a `NetCDF` call of `method` calls, and its parameters before the
    /// error's address, for `arguments` and the result's place `destination`.
    fn netcdf_parameters(
        &mut self,
        method: ArrayFile,
        arguments: &[Argument<'db>],
        destination: &Place,
    ) -> Result<(&'static str, Vec<String>), Unsupported> {
        let name = method.name();
        let call = match (method, arguments) {
            (ArrayFile::NetcdfOpen, [path]) => {
                let (bytes, length) = self.text_argument(path)?;
                let (out, _) = self.place(destination)?;
                let parameters = vec![
                    format!("ptr {bytes}"),
                    format!("i64 {length}"),
                    format!("ptr {out}"),
                ];
                ("baryon_rt_netcdf_open", parameters)
            }
            (ArrayFile::NetcdfRead, [path, variable, items @ ..]) => {
                let (bytes, length) = self.text_argument(path)?;
                let (variable, variable_length) = self.text_argument(variable)?;
                let (out, ty) = self.place(destination)?;
                let (element, rank) = self.array(ty)?;
                let code = self.fits_code(element, name)?;
                let (items, item_count) = self.subscript_items(items, name)?;
                let parameters = vec![
                    format!("ptr {bytes}"),
                    format!("i64 {length}"),
                    format!("ptr {variable}"),
                    format!("i64 {variable_length}"),
                    format!("ptr {items}"),
                    format!("i64 {item_count}"),
                    format!("i32 {code}"),
                    format!("i64 {rank}"),
                    format!("ptr {out}"),
                ];
                ("baryon_rt_netcdf_read", parameters)
            }
            (ArrayFile::NetcdfDefine, [path, line]) => {
                let (bytes, length) = self.text_argument(path)?;
                let (line, line_length) = self.text_argument(line)?;
                let (out, _) = self.place(destination)?;
                let parameters = vec![
                    format!("ptr {bytes}"),
                    format!("i64 {length}"),
                    format!("ptr {line}"),
                    format!("i64 {line_length}"),
                    format!("ptr {out}"),
                ];
                ("baryon_rt_netcdf_define", parameters)
            }
            (ArrayFile::NetcdfWrite, [path, variable, Argument::Borrow(array), items @ ..]) => {
                let (bytes, length) = self.text_argument(path)?;
                let (variable, variable_length) = self.text_argument(variable)?;
                let (address, ty) = self.place(array)?;
                let (element, rank) = self.array(ty)?;
                let code = self.fits_code(element, name)?;
                let (items, item_count) = self.subscript_items(items, name)?;
                let parameters = vec![
                    format!("ptr {bytes}"),
                    format!("i64 {length}"),
                    format!("ptr {variable}"),
                    format!("i64 {variable_length}"),
                    format!("ptr {items}"),
                    format!("i64 {item_count}"),
                    format!("ptr {address}"),
                    format!("i64 {rank}"),
                    format!("i32 {code}"),
                ];
                ("baryon_rt_netcdf_write", parameters)
            }
            _ => return Err(Unsupported::new(format!("`{name}` with these arguments"))),
        };
        Ok(call)
    }

    /// The items of a `NetCDF` subscript, `words` four `int`s each, stored in a stack slot of
    /// `i64`s: its address and the number of items, or `null` and 0 for none, as `var[:]` has.
    /// `name` is the call's, for the error when the words are not whole items.
    fn subscript_items(
        &mut self,
        words: &[Argument<'db>],
        name: &str,
    ) -> Result<(String, usize), Unsupported> {
        if words.is_empty() {
            return Ok(("null".to_owned(), 0));
        }
        if !words.len().is_multiple_of(4) {
            return Err(Unsupported::new(format!(
                "`{name}` with {} subscript words",
                words.len()
            )));
        }
        let llvm = format!("[{} x i64]", words.len());
        let slot = self.slot(&llvm);
        for (at, word) in words.iter().enumerate() {
            let Argument::Owned(word) = word else {
                return Err(Unsupported::new(format!("`{name}` with a borrowed index")));
            };
            let (value, _) = self.value(word)?;
            let address = self.emit(&format!(
                "getelementptr inbounds {llvm}, ptr {slot}, i64 0, i64 {at}"
            ));
            self.line(&format!("store i64 {value}, ptr {address}"));
        }
        Ok((slot, words.len() / 4))
    }

    /// The runtime function a FITS call of `method` calls, and its parameters before the
    /// error's address, for `arguments` and the result's place `destination`.
    fn fits_parameters(
        &mut self,
        method: ArrayFile,
        arguments: &[Argument<'db>],
        destination: &Place,
    ) -> Result<(&'static str, Vec<String>), Unsupported> {
        let name = method.name();
        let call = match (method, arguments) {
            (ArrayFile::FitsHeader, [path, Argument::Owned(ext)]) => {
                let (bytes, length) = self.text_argument(path)?;
                let (ext, _) = self.value(ext)?;
                let (out, _) = self.place(destination)?;
                let parameters = vec![
                    format!("ptr {bytes}"),
                    format!("i64 {length}"),
                    format!("i64 {ext}"),
                    format!("ptr {out}"),
                ];
                ("baryon_rt_fits_cards", parameters)
            }
            (ArrayFile::FitsData, [path, Argument::Owned(ext)]) => {
                let (bytes, length) = self.text_argument(path)?;
                let (ext, _) = self.value(ext)?;
                let (out, ty) = self.place(destination)?;
                let (element, rank) = self.array(ty)?;
                let code = self.fits_code(element, name)?;
                let parameters = vec![
                    format!("ptr {bytes}"),
                    format!("i64 {length}"),
                    format!("i64 {ext}"),
                    format!("i32 {code}"),
                    format!("i64 {rank}"),
                    format!("ptr {out}"),
                ];
                ("baryon_rt_fits_read", parameters)
            }
            (
                ArrayFile::FitsWrite,
                [
                    path,
                    Argument::Borrow(array),
                    Argument::Owned(overwrite),
                    cards @ ..,
                ],
            ) => {
                let (address, ty) = self.place(array)?;
                let (element, rank) = self.array(ty)?;
                let code = self.fits_code(element, name)?;
                let (bytes, length) = self.text_argument(path)?;
                let (overwrite, _) = self.value(overwrite)?;
                let (data, count, header) = match cards {
                    [] => ("null".to_owned(), "0".to_owned(), "false"),
                    [Argument::Borrow(cards)] => {
                        let (list, _) = self.place(cards)?;
                        let data = self.emit(&format!("load ptr, ptr {list}"));
                        (data, self.list_length(&list), "true")
                    }
                    _ => return Err(Unsupported::new(format!("`{name}` with these cards"))),
                };
                let parameters = vec![
                    format!("ptr {bytes}"),
                    format!("i64 {length}"),
                    format!("ptr {address}"),
                    format!("i64 {rank}"),
                    format!("i32 {code}"),
                    format!("ptr {data}"),
                    format!("i64 {count}"),
                    format!("i1 zeroext {header}"),
                    format!("i1 zeroext {overwrite}"),
                ];
                ("baryon_rt_fits_write", parameters)
            }
            _ => return Err(Unsupported::new(format!("`{name}` with these arguments"))),
        };
        Ok(call)
    }

    /// The runtime's `ELEMENT_*` number for a FITS image or a `NetCDF` variable of `element`s:
    /// a number type other than a complex one, which the checker allows. `name` is the call,
    /// for the report of another type.
    fn fits_code(&self, element: Type<'db>, name: &str) -> Result<u32, Unsupported> {
        match (element.kind(self.types.db), self.element_code(element)) {
            (TypeKind::Primitive(Primitive::Integer(_) | Primitive::Float(_)), Some(code)) => {
                Ok(code)
            }
            _ => Err(Unsupported::new(format!(
                "`{name}` of elements of type `{}`",
                element.display(self.types.db)
            ))),
        }
    }

    /// What is reported for an element type `Array.fromfile` cannot read.
    fn reading(&self, ty: Type<'db>) -> Unsupported {
        Unsupported::new(format!(
            "`Array.fromfile` of elements of type `{}`",
            ty.display(self.types.db)
        ))
    }
}
