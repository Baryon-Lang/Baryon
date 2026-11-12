//! The operations on array files a [`Builtin::ArrayFile`](crate::Builtin::ArrayFile) call
//! performs: `Array.fromfile` and `a.tofile` (decision 0036, part 4), `Array.load` and
//! `Array.save` of `NumPy`'s `.npy` files (decision 0043), and the `fits` module's
//! `getheader`, `getdata` and `writeto` of FITS files (decision 0044), and the `netCDF4`
//! module's reads of `NetCDF` classic files (decision 0047).

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
    /// library parses. The `D` lines come first, then the file's `A` lines, then each `V` line
    /// followed by its variable's `A` lines. A `D` line is a dimension's name, its length and
    /// `1` when it is the unlimited one or `0`; a `V` line a variable's name, its type as
    /// `netCDF4` names it (`f4`) and its dimensions' names separated by commas; an `A` line an
    /// attribute's variable (empty for the file's own), its kind (`str`, `int` or `float`), its
    /// number of values, its name and its values separated by commas. The call fails with the
    /// kind `INVALID_DATA` when the file is not a classic `NetCDF` file. It reads the header
    /// only.
    NetcdfOpen,
    /// `ds.variables[name][:]` of the `netCDF4` module: two arguments, the path and the
    /// variable's name, each a `str`. The destination is a new `Array[T, rank]`, its `T` a
    /// signed integer or float type, its shape the variable's dimensions in the file's order.
    /// A float `T` reads any number variable, with `scale_factor` and `add_offset` applied as
    /// `netCDF4` applies them; an integer `T` reads an unscaled integer variable it can hold.
    /// Fill values are read as stored. The call fails with the kind `INVALID_DATA` when the
    /// file has no such variable, or one of another rank or that `T` cannot read. It costs one
    /// pass over the variable's values.
    NetcdfRead,
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
        }
    }
}
