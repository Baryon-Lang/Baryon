//! The `h5by` module's reads of HDF5 files (decision 0052): the file's description, the
//! deflated chunks a read needs inflated, and a dataset's values read; and the `netCDF4`
//! module's reads of a variable, which take the same arguments, since a NetCDF-4 file is HDF5
//! and its deflated chunks are inflated the same way.
//!
//! | Method | Runtime call |
//! | --- | --- |
//! | [`Hdf5Open`](mir::ArrayFile::Hdf5Open) | `baryon_rt_hdf5_open(path, length, out, error)` |
//! | [`Hdf5Chunks`](mir::ArrayFile::Hdf5Chunks) | `baryon_rt_hdf5_chunks(path, length, name, name_length, items, item_count, out, error)` |
//! | [`Hdf5Read`](mir::ArrayFile::Hdf5Read) | `baryon_rt_hdf5_read(path, length, name, name_length, items, item_count, inflated, sizes, element, rank, out, error)` |
//! | [`NetcdfChunks`](mir::ArrayFile::NetcdfChunks) | `baryon_rt_netcdf_chunks(path, length, name, name_length, items, item_count, out, error)` |
//! | [`NetcdfRead`](mir::ArrayFile::NetcdfRead) | `baryon_rt_netcdf_read(path, length, name, name_length, items, item_count, inflated, sizes, element, rank, out, error)` |
//!
//! The items of a subscript are passed as a `NetCDF` call passes them, and the element type as
//! a FITS call does. The inflated chunks and their sizes are passed as the addresses of their
//! arrays' headers, as a write passes its array, so the runtime reads the elements they see.

use crate::Unsupported;
use crate::function::Function;
use mir::{Argument, ArrayFile, Place};

impl<'db> Function<'_, '_, 'db> {
    /// The runtime function an HDF5 call, or a `NetCDF` read, of `method` calls, and its
    /// parameters before the error's address, for `arguments` and the result's place
    /// `destination`.
    pub(super) fn hdf5_parameters(
        &mut self,
        method: ArrayFile,
        arguments: &[Argument<'db>],
        destination: &Place,
    ) -> Result<(&'static str, Vec<String>), Unsupported> {
        let name = method.name();
        let call = match (method, arguments) {
            (ArrayFile::Hdf5Open, [path]) => {
                let (bytes, length) = self.text_argument(path)?;
                let (out, _) = self.place(destination)?;
                let parameters = vec![
                    format!("ptr {bytes}"),
                    format!("i64 {length}"),
                    format!("ptr {out}"),
                ];
                ("baryon_rt_hdf5_open", parameters)
            }
            (ArrayFile::Hdf5Chunks | ArrayFile::NetcdfChunks, [path, dataset, items @ ..]) => {
                let (bytes, length) = self.text_argument(path)?;
                let (dataset, dataset_length) = self.text_argument(dataset)?;
                let (out, _) = self.place(destination)?;
                let (items, item_count) = self.subscript_items(items, name)?;
                let parameters = vec![
                    format!("ptr {bytes}"),
                    format!("i64 {length}"),
                    format!("ptr {dataset}"),
                    format!("i64 {dataset_length}"),
                    format!("ptr {items}"),
                    format!("i64 {item_count}"),
                    format!("ptr {out}"),
                ];
                let symbol = if method == ArrayFile::NetcdfChunks {
                    "baryon_rt_netcdf_chunks"
                } else {
                    "baryon_rt_hdf5_chunks"
                };
                (symbol, parameters)
            }
            (
                ArrayFile::Hdf5Read | ArrayFile::NetcdfRead,
                [
                    path,
                    dataset,
                    Argument::Borrow(inflated),
                    Argument::Borrow(sizes),
                    items @ ..,
                ],
            ) => {
                let (bytes, length) = self.text_argument(path)?;
                let (dataset, dataset_length) = self.text_argument(dataset)?;
                let (inflated, _) = self.place(inflated)?;
                let (sizes, _) = self.place(sizes)?;
                let (out, ty) = self.place(destination)?;
                let (element, rank) = self.array(ty)?;
                let code = self.fits_code(element, name)?;
                let (items, item_count) = self.subscript_items(items, name)?;
                let parameters = vec![
                    format!("ptr {bytes}"),
                    format!("i64 {length}"),
                    format!("ptr {dataset}"),
                    format!("i64 {dataset_length}"),
                    format!("ptr {items}"),
                    format!("i64 {item_count}"),
                    format!("ptr {inflated}"),
                    format!("ptr {sizes}"),
                    format!("i32 {code}"),
                    format!("i64 {rank}"),
                    format!("ptr {out}"),
                ];
                let symbol = if method == ArrayFile::NetcdfRead {
                    "baryon_rt_netcdf_read"
                } else {
                    "baryon_rt_hdf5_read"
                };
                (symbol, parameters)
            }
            _ => return Err(Unsupported::new(format!("`{name}` with these arguments"))),
        };
        Ok(call)
    }
}
