//! FITS files (decision 0044), built and run: `fits.getdata` and `fits.getheader` read files
//! `AstroPy` wrote, a `Header`'s values are read and set as `AstroPy`'s are, `fits.writeto` writes
//! the bytes `AstroPy` writes, and a file unlike the program raises `OSError` or `KeyError`.
//!
//! The files in `tests/fixtures/fits` were written by `AstroPy`; its `README.md` says how.

use super::{Scratch, baryon_build, package};
use std::fs;
use std::io;
use std::path::Path;
use std::process::{Command, Output};

/// The directory of files `AstroPy` wrote.
fn fixtures() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fits"))
}

/// Builds `program` as the package `name` with `AstroPy`'s files beside it, runs it there, and
/// returns what it printed and the package's directory, which is removed when it is dropped.
///
/// # Errors
///
/// Fails when the package cannot be written or the program cannot be run.
fn run(name: &str, program: &str) -> io::Result<(Output, Scratch)> {
    let directory = package(name, &[("main.byn", program)])?;
    for entry in fs::read_dir(fixtures())? {
        let path = entry?.path();
        if let Some(file) = path
            .file_name()
            .filter(|file| file.as_encoded_bytes().ends_with(b".fits"))
        {
            fs::copy(&path, directory.join(file))?;
        }
    }
    let (output, executable) = baryon_build(&directory)?;
    assert!(output.status.success(), "{output:?}");
    let run = Command::new(&executable).current_dir(&directory).output()?;
    Ok((run, directory))
}

/// A program reads `AstroPy`'s images as the types it names, an unsigned and a scaled one among
/// them, an image in an extension, and a header's values typed by where they go.
#[test]
fn astropy_files_are_read() {
    let program = "\
import fits
from fits import getdata, getheader


def main() -> Raises[None, OSError | KeyError]:
    image: Array[u16, 2] = fits.getdata(\"image.fits\")
    print(image, image.shape)
    floats = getdata(\"image.fits\")
    print(floats[2, 3])
    small: Array[f32, 2] = getdata(filename=\"float32.fits\")
    print(small)
    scaled: Array[f32, 2] = getdata(\"scaled.fits\")
    print(scaled)
    science: Array[i32, 2] = getdata(\"extension.fits\", 1)
    print(science)
    header = getheader(\"image.fits\")
    exposure: float = header[\"EXPTIME\"]
    name: str = header[\"OBJECT\"]
    count: int = header[\"ncombine\"]
    done: bool = header[\"DONE\"]
    print(name, exposure, count, done)
    print(header.get(\"FILTER\", \"none\"), header.get(\"EXPTIME\", 0.0))
    print(\"EXPTIME\" in header, \"FILTER\" in header, \"FILTER\" not in header)
    extension = getheader(\"extension.fits\", ext=1)
    print(extension.get(\"EXTNAME\", \"\"))
";
    let (output, _) = run("fits_read", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "[[    0  1000  2000  3000]\n \
         [ 4000  5000  6000  7000]\n \
         [ 8000  9000 10000 11000]] (3, 4)\n\
         11000.0\n\
         [[ 5.00e-01 -1.25e+00  2.00e+00]\n \
         [ 3.00e+00  1.00e-03 -7.50e+00]]\n\
         [[1.  2. ]\n \
         [3.5 4.5]]\n\
         [[ 1 -2]\n \
         [ 3 -4]]\n\
         M31 30.0 3 True\n\
         none 30.0\n\
         True False True\n\
         SCI\n"
    );
}

/// A program changes a header and writes an image with it, and one with no header, and each
/// file holds the bytes `AstroPy` wrote for the same image and header. Writing over a file needs
/// `overwrite=True`, as in `AstroPy`.
#[test]
fn written_files_match_astropy() {
    let program = "\
import fits


def main() -> Raises[None, OSError | KeyError]:
    image: Array[u16, 2] = fits.getdata(\"image.fits\")
    header = fits.getheader(\"image.fits\")
    header[\"EXPTIME\"] = 31.5
    header[\"FILTER\"] = (\"V\", \"band\")
    fits.writeto(\"out_edited.fits\", image, header)
    fits.writeto(\"out_plain.fits\", fits.getdata(\"image.fits\"), overwrite=True)
    try:
        fits.writeto(\"out_plain.fits\", image)
    except OSError as error:
        print(error.kind)
";
    let (output, directory) = run("fits_written", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "ErrorKind.FILE_EXISTS\n"
    );
    for name in ["edited", "plain"] {
        let written = fs::read(directory.join(format!("out_{name}.fits"))).unwrap();
        let astropy = fs::read(fixtures().join(format!("{name}.fits"))).unwrap();
        assert_eq!(written, astropy, "{name}");
    }
}

/// An image of another type or rank, a missing HDU or a file that is not FITS raises `OSError`
/// with the kind `INVALID_DATA`, and a missing card or one of another type raises `KeyError`
/// with `AstroPy`'s message.
#[test]
fn files_unlike_the_program_raise() {
    let program = "\
import fits


def image(path: str, ext: int) -> None:
    try:
        pixels: Array[i16, 2] = fits.getdata(path, ext)
        print(pixels.shape)
    except OSError as error:
        print(error.kind, error.strerror)


def line(path: str) -> None:
    try:
        pixels: Array[f64, 1] = fits.getdata(path)
        print(pixels.shape)
    except OSError as error:
        print(error.kind, error.strerror)


def value(header: fits.Header, key: str) -> None:
    try:
        number: float = header[key]
        print(number)
    except KeyError as error:
        print(error.message)


def main() -> Raises[None, OSError]:
    with open(\"notes.txt\", \"w\") as notes:
        notes.write(\"1.5 2.5\\n\")
    image(\"image.fits\", 0)
    image(\"extension.fits\", 2)
    image(\"extension.fits\", 0)
    image(\"notes.txt\", 0)
    line(\"image.fits\")
    header = fits.getheader(\"image.fits\")
    value(header, \"OBJECT\")
    value(header, \"NOPE\")
";
    let (output, _) = run("fits_failing", program).unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "ErrorKind.INVALID_DATA the FITS image has BITPIX = 16 with BSCALE = 1 and BZERO = \
         32768, which does not read as `i16`; read it as a float array\n\
         ErrorKind.INVALID_DATA there is no HDU 2; the file has 2\n\
         ErrorKind.INVALID_DATA HDU 0 of the FITS file holds no image\n\
         ErrorKind.INVALID_DATA the file is not a FITS file: it does not start with SIMPLE\n\
         ErrorKind.INVALID_DATA HDU 0 of the FITS file holds an image of rank 2, not rank 1 as \
         the program asks\n\
         Keyword 'OBJECT' does not hold a number.\n\
         Keyword 'NOPE' not found.\n"
    );
}
