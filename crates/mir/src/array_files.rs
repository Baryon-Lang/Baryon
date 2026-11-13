//! The operations on array files a [`Builtin::ArrayFile`](crate::Builtin::ArrayFile) call
//! performs: `Array.fromfile` and `a.tofile` (decision 0036, part 4), `Array.load` and
//! `Array.save` of `NumPy`'s `.npy` files (decision 0043), and the `fits` module's
//! `getheader`, `getdata` and `writeto` of FITS files (decision 0044), the `netCDF4`
//! module's reads of `NetCDF` classic files (decision 0047), and the `h5by` module's reads of
//! HDF5 files (decision 0052).

/// An operation on an array file, called by [`Builtin::ArrayFile`](crate::Builtin::ArrayFile).
///
/// An array file holds an array's elements and nothing else: each element's bytes as they lie in
/// memory, in row-major order, with no header, as `numpy.fromfile` reads and
/// `numpy.ndarray.tofile` writes. Both operations can fail, so the call has an error edge whose
/// local is a `Raises[None, OSError]`, as a [`FileMethod`](crate::FileMethod) call has. A `str`
/// argument is passed `Borrow`, or `Owned` for a literal. Each costs one pass over the array's
/// bytes, with no work per element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub enum ArrayFile {
    /// `Array.fromfile(path, dtype, count, offset)`: three arguments, the path, a `str`, then
    /// the count and the offset, each an `i64` passed `Owned`. The destination is a new
    /// `Array[T, 1]`, and its `T` is the element type read: a number, a `bool`, a `Fixed` array
    /// or a `Copy` struct of them. The call skips `offset` bytes, then reads `count` elements,
    /// or every element to the end of the file when `count` is negative. It fails with the kind
    /// `INVALID_DATA` when the bytes after the offset are not a whole number of elements, or
    /// are fewer than `count` elements. A byte read into a `bool` becomes `True` unless it is 0.
    FromFile,
    /// `a.tofile(path)`: two arguments, the array, passed `Borrow`, and the path, a `str`; the
    /// destination has type `None`. The call makes the file, or empties the one there, and
    /// writes every element the array sees, so a view writes only its own elements, in
    /// row-major order.
    ToFile,
    /// `Array.load(path)`: one argument, the path, a `str`. The destination is a new
    /// `Array[T, rank]` of any rank, and its `T` is a number or a `bool`. The call reads a
    /// `.npy` file, `NumPy`'s format versions 1.0 to 3.0, and fails with the kind
    /// `INVALID_DATA` when the file is not one, or when its header names another element type
    /// or rank than the destination's, Fortran order or big-endian bytes. Its elements are read
    /// straight into the array, as `FromFile` reads them.
    Load,
    /// `Array.save(path, a)`: two arguments, the path, a `str`, and the array, passed `Borrow`;
    /// the destination has type `None`. The call makes the file, or empties the one there, and
    /// writes a `.npy` file in `NumPy`'s format version 1.0: the header that names the element
    /// type, as `NumPy` spells it (`'<f8'`), and the shape, then every element the array sees
    /// in row-major order, as `ToFile` writes them.
    Save,
    /// `fits.getheader(path, ext)`: two arguments, the path, a `str`, and the number of the
    /// HDU, an `i64` passed `Owned`, the primary HDU 0. The destination is a new `list[str]`:
    /// the HDU's header cards, each 80 characters, `END` left out. The call fails with the kind
    /// `INVALID_DATA` when the file is not FITS or has no such HDU. It reads the headers up to
    /// the HDU and skips the data before it.
    FitsHeader,
    /// `fits.getdata(path, ext)`: the arguments of `FitsHeader`. The destination is a new
    /// `Array[T, rank]`, its `T` a number type other than a complex one, its shape the HDU's
    /// axes reversed, as `AstroPy` gives them. An integer `T` reads an image stored as `T`, or
    /// as its other signedness shifted by `BZERO` as `AstroPy` writes it; a float `T` reads
    /// any image, with `BSCALE` and `BZERO` applied in `T`'s precision. The call fails with
    /// the kind `INVALID_DATA` when the HDU holds no image, one of another rank, or one `T`
    /// cannot read. It costs one pass over the pixels.
    FitsData,
    /// `fits.writeto(path, data, header, overwrite)`: three or four arguments, the path, a
    /// `str`, the array, passed `Borrow`, whose element type is one `FitsData` reads, the
    /// `overwrite` flag, a `bool` passed `Owned`, then, when the program gave a header, its
    /// cards, a `list[str]` passed `Borrow`; the destination has type `None`. The call writes
    /// a FITS file of one image as `AstroPy`'s `writeto` does, the program's cards after the
    /// mandatory ones, and fails with the kind `FILE_EXISTS` when the file exists and
    /// `overwrite` is false. It costs one pass over the array's elements.
    FitsWrite,
    /// `Dataset(path)` of the `netCDF4` module: one argument, the path, a `str`. The
    /// destination is a new `list[str]`: lines that describe the header of a `NetCDF` classic
    /// file (CDF-1 or CDF-2), each a tag and fields separated by tabs, which the standard
    /// library parses. A `P` line, the path, comes first, so the library can read variables
    /// later; then the `D` lines, then the file's `A` lines, then each `V` line followed by its
    /// variable's `A` lines. A `D` line is a dimension's name, its length and
    /// `1` when it is the unlimited one or `0`; a `V` line a variable's name, its type as
    /// `netCDF4` names it (`f4`) and its dimensions' names separated by commas; an `A` line an
    /// attribute's variable (empty for the file's own), its kind (`str`, `int` or `float`), its
    /// number of values, its name and its values separated by commas. The call fails with the
    /// kind `INVALID_DATA` when the file is not a classic `NetCDF` file. It reads the header
    /// only.
    NetcdfOpen,
    /// `ds.variables[name][items]` of the `netCDF4` module: the path and the variable's name,
    /// each a `str`, then four `int`s for each item of the subscript, which picks the values
    /// read: `0` and an index, or `1` and a slice's start, stop and step, each `i64::MIN` when
    /// the program left it out. No items read the whole variable, as `var[:]` does. The
    /// destination is a new `Array[T, rank]`, its `T` a signed integer or float type, its shape
    /// the lengths picked along the axes a slice keeps, in the file's order. A float `T` reads
    /// any number variable, with `scale_factor` and `add_offset` applied as `netCDF4` applies
    /// them; an integer `T` reads an unscaled integer variable it can hold. Fill values are read
    /// as stored. The call fails with the kind `INVALID_DATA` when the file has no such
    /// variable, the items do not fit its dimensions, or the values picked have another rank or
    /// `T` cannot read them. It costs one pass over the values picked.
    NetcdfRead,
    /// One change to a `NetCDF` classic file opened for writing, which the `netCDF4` module's
    /// `Dataset(path, "w")`, `createDimension`, `createVariable` and `setncattr` make: two
    /// arguments, the path and the change, each a `str`. The change is a line of a tag and
    /// fields separated by tabs: `C` and the format's version, `1` or `2`, for a new empty file;
    /// `D`, a dimension's name and its length, `0` for the unlimited one; `V`, a variable's
    /// name, its type as `netCDF4` names it (`f4`) and its dimensions' names separated by
    /// commas; `A`, an attribute's variable (empty for the file's own), its kind (`str`, `int`
    /// or `float`), its name and its value. The destination is a new `list[str]`, the lines
    /// that describe the file's header after the change, as [`Self::NetcdfOpen`] writes them,
    /// so the library's `Dataset` stays in step with its file. The file is the only state: each
    /// change reads it, writes it again and reads its header. The call fails with the kind
    /// `INVALID_DATA` when the format refuses the change.
    NetcdfDefine,
    /// `ds.variables[name][items] = array` of the `netCDF4` module: the path and the
    /// variable's name, each a `str`, the array, an `Array[T, rank]` borrowed, its `T` a number
    /// type, then four `int`s for each item of the subscript, as [`Self::NetcdfRead`] takes
    /// them; no items write the whole variable. The destination is `None`. The array's values
    /// become the values picked, converted to the variable's type and packed by `scale_factor`
    /// and `add_offset` as `netCDF4` packs them. Along the unlimited dimension the items may
    /// pass the last record, and a slice with no end takes as many records as the array has
    /// rows. The call fails with the kind `INVALID_DATA` when the file has no such variable, the
    /// items do not fit its dimensions, or the array's shape differs from the shape picked. It
    /// costs one pass over the values and one read and write of the file.
    NetcdfWrite,
    /// `h5by.File(path)`: one argument, the path, a `str`. The destination is a new
    /// `list[str]`: lines that describe the HDF5 file, each a tag and fields separated by tabs,
    /// which the standard library parses. A `P` line, the path, comes first, so the library can
    /// read datasets later; then, in h5py's order, a `G` line for each group, its path; a `D`
    /// line for each dataset, its path, its type as `NumPy` names it (`<f8`) and its shape, the
    /// lengths separated by commas; and after each object its `A` lines, an attribute's owner,
    /// its kind (`str`, `int` or `float`), its shape, its name and its values separated by
    /// tabs. The call fails with the kind `INVALID_DATA` when the file is not HDF5 or is
    /// damaged, or uses a feature not read yet. It reads the file's structure only.
    Hdf5Open,
    /// The deflated chunks a read of an HDF5 dataset needs inflated: the path and the
    /// dataset's path in the file, each a `str`, then four `int`s for each item of the
    /// subscript, as [`Self::Hdf5Read`] takes them. The destination is a new `list[str]`, one
    /// line for each deflated chunk that holds a value picked: its address and its size in
    /// bytes, separated by a tab, in the order [`Self::Hdf5Read`] takes them inflated. The
    /// standard library's `zlib` inflates them (decision 0049), so Baryon has one deflate
    /// decoder. The call fails as [`Self::Hdf5Read`] does. It reads the dataset's chunk index.
    Hdf5Chunks,
    /// `f[name][items]` of the `h5by` module: the path and the dataset's path in the file, each
    /// a `str`, the inflated chunks, an `Array[u8, 1]` borrowed, and their sizes, an
    /// `Array[i64, 1]` borrowed, then four `int`s for each item of the subscript, which picks
    /// the values read: `0` and an index, or `1` and a slice's start, stop and step, each
    /// `i64::MIN` when the program left it out. No items read the whole dataset, as
    /// `f[name][...]` does. The destination is a new `Array[T, rank]`, its `T` a number type,
    /// its shape the lengths picked along the axes a slice keeps. A float `T` reads any number
    /// dataset; an integer `T` reads an integer dataset it can hold. Elements never written
    /// read as the fill value. The call fails with the kind `INVALID_DATA` when the file has no
    /// such dataset, the values picked have another rank or `T` cannot read them, or a chunk is
    /// damaged; with items, it raises h5py's `IndexError` for an index past its axis and
    /// `ValueError` for more items than axes or a step that is not positive, so its error is a
    /// `Raises[None, OSError | IndexError | ValueError]`. It costs one pass over the stored
    /// values picked, reading only the chunks that hold one.
    Hdf5Read,
}

impl ArrayFile {
    /// The name the source calls it by.
    ///
    /// ```
    /// assert_eq!(mir::ArrayFile::FromFile.name(), "Array.fromfile");
    /// assert_eq!(mir::ArrayFile::ToFile.name(), "Array.tofile");
    /// assert_eq!(mir::ArrayFile::Load.name(), "Array.load");
    /// assert_eq!(mir::ArrayFile::Save.name(), "Array.save");
    /// assert_eq!(mir::ArrayFile::FitsHeader.name(), "fits.getheader");
    /// assert_eq!(mir::ArrayFile::FitsData.name(), "fits.getdata");
    /// assert_eq!(mir::ArrayFile::FitsWrite.name(), "fits.writeto");
    /// assert_eq!(mir::ArrayFile::NetcdfOpen.name(), "netCDF4.Dataset");
    /// assert_eq!(mir::ArrayFile::NetcdfRead.name(), "netCDF4.Variable[:]");
    /// assert_eq!(mir::ArrayFile::NetcdfDefine.name(), "netCDF4.define");
    /// assert_eq!(mir::ArrayFile::NetcdfWrite.name(), "netCDF4.Variable[:] =");
    /// assert_eq!(mir::ArrayFile::Hdf5Open.name(), "h5by.File");
    /// assert_eq!(mir::ArrayFile::Hdf5Chunks.name(), "h5by.chunks");
    /// assert_eq!(mir::ArrayFile::Hdf5Read.name(), "h5by.Dataset[...]");
    /// ```
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::FromFile => "Array.fromfile",
            Self::ToFile => "Array.tofile",
            Self::Load => "Array.load",
            Self::Save => "Array.save",
            Self::FitsHeader => "fits.getheader",
            Self::FitsData => "fits.getdata",
            Self::FitsWrite => "fits.writeto",
            Self::NetcdfOpen => "netCDF4.Dataset",
            Self::NetcdfRead => "netCDF4.Variable[:]",
            Self::NetcdfDefine => "netCDF4.define",
            Self::NetcdfWrite => "netCDF4.Variable[:] =",
            Self::Hdf5Open => "h5by.File",
            Self::Hdf5Chunks => "h5by.chunks",
            Self::Hdf5Read => "h5by.Dataset[...]",
        }
    }
}
