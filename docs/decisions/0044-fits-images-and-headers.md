# 0044. FITS images and headers are read and written with AstroPy's names, in a `fits` module

Status: Accepted
Date: 2026-11-11
Roadmap: 3.14.3

## Context

FITS, the Flexible Image Transport System, is the file format of astronomy: every telescope
image, most spectra and many catalogues arrive as FITS. Roadmap 3.14.3 reads and writes its core:
images and their headers, as the FITS standard 4.0 defines them. Binary tables, tile compression
and memory mapping are roadmap 4.2.5's.

A FITS file is a sequence of HDUs (header and data units). Each starts with a header: 80-byte
text cards such as `NAXIS1  =                 1024 / length of axis 1`, in blocks of 2880 bytes,
ending with `END`. An image HDU's data follows: `NAXIS` axes of big-endian numbers whose type
`BITPIX` names (8 for `u8`, 16 for `i16`, 32 for `i32`, 64 for `i64`, -32 for `f32`, -64 for
`f64`), the first axis varying fastest, padded to 2880 bytes. `BSCALE` and `BZERO` turn stored
integers into physical values, `physical = BZERO + BSCALE * stored`, which is how an unsigned
16-bit image is stored (`BZERO = 32768`).

[Decision 0039](0039-astrobaryon-keeps-astropy-names.md) settles that astronomy code keeps
AstroPy's names where behavior matches. AstroPy reads FITS with `astropy.io.fits`:
`fits.getdata(path)`, `fits.getheader(path)`, `fits.writeto(path, data, header)`, and
`fits.open(path)` for a list of HDUs. Three questions remain that AstroPy settles at run time and
a compiled language must settle when compiling:

1. **The image's element type and rank.** AstroPy returns whatever the file holds.
2. **The type of a header value.** A card holds a `bool`, an integer, a float, a string or a
   complex number, and a Baryon union holds structs only
   ([decision 0032](0032-sum-types-are-unions-of-structs.md)).
3. **What a failure raises.** AstroPy raises `OSError` for a file that is not FITS and `KeyError`
   for a missing keyword.

## Decision

1. **A `fits` module with AstroPy's names.** `from fits import getdata, getheader, writeto,
   Header`, or `import fits` and `fits.getdata(path)`: the names and arguments of
   `astropy.io.fits`, written in Baryon in the standard library
   ([decision 0038](0038-standard-library-in-baryon.md)) where Baryon can say it, and in the
   runtime where bytes are needed. When AstroBaryon (roadmap 4.2) has its own package, the same
   module is its `io.fits`.
2. **The image's type and rank come from the context**, as `Array.load`'s do
   ([decision 0043](0043-array-data-files.md)): `image: Array[f32, 2] = fits.getdata(path)`,
   else `Array[f64, 2]`. The axes are reversed, as AstroPy and NumPy reverse them, so
   `image[y, x]` reads the pixel at FITS axis 1 = x + 1, axis 2 = y + 1. The pixels are converted
   to the element type asked for, with `BSCALE` and `BZERO` applied, as AstroPy's scaled data
   is: any image reads as a float array, and an integer array reads an integer image with no
   scaling, or an unsigned image stored with AstroPy's `BZERO`. A rank other than the file's,
   or an integer type that cannot hold the image's values, raises `OSError` with the kind
   `INVALID_DATA`.
3. **Header values are typed by the context.** `fits.getheader(path)` gives a `Header`, whose
   cards keep their order and comments. `exposure: f64 = header["EXPTIME"]` reads a number,
   `name: str = header["OBJECT"]` a string, and `header.get("FILTER", "none")` takes its type
   from the default. A card whose value is not of that type, or a missing keyword, raises
   `KeyError` with AstroPy's message. Writing is `header["EXPTIME"] = 30.0` and
   `header["EXPTIME"] = (30.0, "seconds")` for a value with a comment, as in AstroPy.
4. **Bad files raise `OSError` with the kind `INVALID_DATA`**, saying what was wrong, as
   AstroPy raises `OSError`. A missing keyword raises `KeyError`, a new error type in the
   standard library with AstroPy's message, `Keyword 'EXPTIME' not found.`.
5. **Written files are what AstroPy writes.** `fits.writeto(path, data, header, overwrite=False)`
   writes a primary HDU whose `BITPIX` and axes come from the array, the header's own cards
   after the mandatory ones, padded to 2880 bytes, so `astropy.io.fits` and `fitsverify` read it
   and the bytes match AstroPy's for the same array and header. An existing file raises
   `OSError` with the kind `FILE_EXISTS` unless `overwrite=True`, as in AstroPy.

## Consequences

- An AstroPy user reads `fits.getdata`, `fits.getheader` and `header["EXPTIME"]` as they do
  today; the transition guide (roadmap 4.2.11) lists the two differences: the array's type comes
  from the program, and a header value's type does too.
- A header value used with the wrong type is found when the program runs, as in AstroPy, but as
  a `KeyError` the program can catch rather than a value of the wrong type later.
- `fits.open` and its list of HDUs, binary tables, compressed images and memory-mapped images
  larger than memory are not in this record; 4.2.5 builds them on the same module.
- Reading converts big-endian pixels once, in one pass, into a new array: an image costs its
  size in memory and one pass over its bytes.
