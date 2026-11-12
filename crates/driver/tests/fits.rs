//! Checking FITS files (decision 0044), end to end: programs that misuse the `fits` module and
//! a `Header` go through the parser, the syntax check, the resolver and the checker, and the
//! rendered diagnostics are compared in full.

use driver::Session;

/// Checks a package of one file, `main.byn`, and returns every diagnostic, rendered and joined
/// by blank lines. The program's first newline, which only lets a test start it on its own
/// line, is dropped.
fn check_one(text: &str) -> String {
    let mut session = Session::new();
    if let Err(error) = session.add_file("main.byn", text.strip_prefix('\n').unwrap_or(text)) {
        return error.to_string();
    }
    session
        .check()
        .iter()
        .map(|diagnostic| diagnostics::render(diagnostic, &session).to_string())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// A program that uses the module and a header well has no diagnostics.
#[test]
fn images_and_headers_check() {
    let program = r#"
import fits

def main() -> Raises[None, OSError | KeyError]:
    image: Array[f32, 2] = fits.getdata("in.fits", ext=1)
    header = fits.getheader("in.fits")
    exposure: float = header["EXPTIME"]
    header["EXPTIME"] = (exposure * 2.0, "seconds")
    header["OBJECT"] = "M31"
    if "FILTER" not in header:
        header["FILTER"] = header.get("BAND", "V")
    fits.writeto("out.fits", image, header, overwrite=True)
"#;
    assert_eq!(check_one(program), "");
}

/// An image type FITS cannot hold, an untyped header value, a value of a type no card holds,
/// `AstroPy`'s keywords that are not built, a function that is not built, and an unhandled
/// `KeyError`.
#[test]
fn what_cannot_be_fits_is_explained() {
    let program = r#"
import fits

def main() -> Raises[None, OSError]:
    flags: Array[bool, 2] = fits.getdata("in.fits")
    mapped = fits.getheader("in.fits", memmap=True)
    header = fits.getheader("in.fits")
    print(header["OBJECT"])
    header["AXES"] = [1, 2]
    fits.writeto("out.fits", [1.0, 2.0])
    fits.open("in.fits")

def careless(header: fits.Header) -> None:
    name: str = header["OBJECT"]
"#;
    assert_eq!(
        check_one(program),
        r#"error: `fits.getdata` cannot read a FITS image into an `Array[bool, 2]`
 --> main.byn:4:29
  |
4 |     flags: Array[bool, 2] = fits.getdata("in.fits")
  |                             ^^^^^^^^^^^^^^^^^^^^^^^ not a type a FITS image holds
  = note: a FITS image holds integers or floats, such as `Array[f32, 2]`

error: `fits.getheader` does not take `memmap` yet
 --> main.byn:5:40
  |
5 |     mapped = fits.getheader("in.fits", memmap=True)
  |                                        ^^^^^^ not built yet
  = note: decision 0044 builds images and headers; `(data, header)` pairs, memory maps, scaling options and checksums come later

error: the type of a header value comes from where it goes
 --> main.byn:7:11
  |
7 |     print(header["OBJECT"])
  |           ^^^^^^^^^^^^^^^^ type not known
  = note: give it a type, such as `exposure: float = header["EXPTIME"]`; a card holds a `str`, an `int`, a `float` or a `bool` (decision 0044)

error: a header value is a `str`, an `int`, a `float` or a `bool`, found `list[i64]`
 --> main.byn:8:22
  |
8 |     header["AXES"] = [1, 2]
  |                      ^^^^^^ not a card's type

error: `fits.writeto` writes an `Array` of integers or floats, found `list[f64]`
 --> main.byn:9:30
  |
9 |     fits.writeto("out.fits", [1.0, 2.0])
  |                              ^^^^^^^^^^ cannot be written as a FITS image
  = note: a FITS image holds integers or floats, such as `Array[f32, 2]`

error: the `fits` module has no function `open` yet
  --> main.byn:10:5
   |
10 |     fits.open("in.fits")
   |     ^^^^^^^^^^^^^^^^^^^^ not built yet
   = note: decision 0044 builds `getdata`, `getheader`, `writeto` and `Header`; `fits.open` and tables are roadmap 4.2.5's

error: reading a header value can fail with `KeyError`, which is not handled here
  --> main.byn:13:17
   |
13 |     name: str = header["OBJECT"]
   |                 ^^^^^^^^^^^^^^^^ can fail with `KeyError`
   = note: catch it with `try:` and `except KeyError:`, or let it pass on by declaring `-> Raises[None, KeyError]`"#
    );
}
