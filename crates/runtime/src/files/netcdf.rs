//! `NetCDF` classic files (decision 0047): the header of a CDF-1 or CDF-2 file and the values of
//! one of its variables, read as Unidata's "`NetCDF` Classic Format Specification" defines them,
//! in safe code the exported functions in `abi::netcdf` call. What the dimensions, variables and
//! attributes mean to a program is parsed in Baryon, in the standard library's `netCDF4`
//! module, from the description lines [`describe`] writes; this module reads only what it needs
//! to find and convert a variable's values.
//!
//! A classic file is the magic bytes `CDF` and a version byte, 1 for 32-bit offsets or 2 for
//! 64-bit ones, the number of records, then three lists: the dimensions (a name and a length,
//! 0 for the one unlimited dimension), the global attributes (a name, a type and values), and
//! the variables (a name, the dimensions it spans, its attributes, its type, its size in bytes
//! and where its data begins). Every number is big-endian and every name and value list is
//! padded to a multiple of 4 bytes. A variable whose first dimension is the unlimited one is a
//! record variable: its values for record `r` begin at `begin + r * record_size`, where the
//! record size is the sum of the sizes of every record variable.
//!
//! Cost: a read is the header, then one pass over the variable's values, converted from
//! big-endian as they are copied into the array.

use super::fits::Pixel;
use super::{BUFFER, ErrorKind, OsFailure};
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};

mod encoding;
mod header;
mod storage;
mod writing;

pub(crate) use header::read_header;
pub(crate) use writing::{define, write};

/// The type of a `NetCDF` value, numbered as the format numbers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// `NC_BYTE`, a signed 8-bit integer.
    Byte,
    /// `NC_CHAR`, a byte of text.
    Char,
    /// `NC_SHORT`, a signed 16-bit integer.
    Short,
    /// `NC_INT`, a signed 32-bit integer.
    Int,
    /// `NC_FLOAT`, a 32-bit float.
    Float,
    /// `NC_DOUBLE`, a 64-bit float.
    Double,
}

impl Kind {
    /// The type numbered `number`, or `None` for a number the classic formats do not define.
    fn from_number(number: u32) -> Option<Self> {
        Some(match number {
            1 => Self::Byte,
            2 => Self::Char,
            3 => Self::Short,
            4 => Self::Int,
            5 => Self::Float,
            6 => Self::Double,
            _ => return None,
        })
    }

    /// The size of one value in bytes.
    pub(crate) fn size(self) -> usize {
        match self {
            Self::Byte | Self::Char => 1,
            Self::Short => 2,
            Self::Int | Self::Float => 4,
            Self::Double => 8,
        }
    }

    /// The type as `netCDF4` names it in a variable's `dtype`: `i1`, `S1`, `i2`, `i4`, `f4` or
    /// `f8`.
    fn name(self) -> &'static str {
        match self {
            Self::Byte => "i1",
            Self::Char => "S1",
            Self::Short => "i2",
            Self::Int => "i4",
            Self::Float => "f4",
            Self::Double => "f8",
        }
    }

    /// Whether an array of `pixel` can hold every value of this type: an integer type at least
    /// as wide and signed, or any float type for any number.
    fn fits(self, pixel: Pixel) -> bool {
        match pixel {
            Pixel::F32 | Pixel::F64 => self != Self::Char,
            Pixel::I8 => self == Self::Byte,
            Pixel::I16 => matches!(self, Self::Byte | Self::Short),
            Pixel::I32 | Pixel::I64 => matches!(self, Self::Byte | Self::Short | Self::Int),
            Pixel::U8 | Pixel::U16 | Pixel::U32 | Pixel::U64 => false,
        }
    }
}

/// An attribute: a name and its values.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Attribute {
    /// The attribute's name.
    pub(crate) name: String,
    /// The values' type.
    pub(crate) kind: Kind,
    /// The values: text for `NC_CHAR`, else each number as an `f64`, which holds every value
    /// of the classic types exactly.
    pub(crate) values: Values,
}

/// The values of an attribute.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Values {
    /// The text of an `NC_CHAR` attribute, without the zero bytes that may end it.
    Text(String),
    /// The numbers of any other attribute.
    Numbers(Vec<f64>),
}

/// A variable: what the header says about it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Variable {
    /// The variable's name.
    pub(crate) name: String,
    /// The dimensions it spans, as indexes into [`Header::dimensions`], slowest first.
    pub(crate) dimensions: Vec<usize>,
    /// Its attributes.
    pub(crate) attributes: Vec<Attribute>,
    /// The type of its values.
    pub(crate) kind: Kind,
    /// Where its data begins, in bytes from the start of the file.
    pub(crate) begin: u64,
    /// Its size in bytes as the header gives it: one record's for a record variable.
    pub(crate) size: u64,
}

/// What a classic file's header says.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Header {
    /// Whether the file is CDF-2, whose variables begin at 64-bit offsets, or else CDF-1.
    pub(crate) wide: bool,
    /// The number of records written.
    pub(crate) records: u64,
    /// Each dimension's name and length, 0 for the unlimited one.
    pub(crate) dimensions: Vec<(String, u64)>,
    /// The file's own attributes.
    pub(crate) attributes: Vec<Attribute>,
    /// The variables, in the file's order.
    pub(crate) variables: Vec<Variable>,
}

impl Header {
    /// Whether `variable` is a record variable: its first dimension is the unlimited one.
    fn is_record(&self, variable: &Variable) -> bool {
        variable
            .dimensions
            .first()
            .is_some_and(|&dimension| self.dimensions[dimension].1 == 0)
    }

    /// The length of dimension `index`: the number of records for the unlimited one.
    pub(crate) fn length(&self, index: usize) -> u64 {
        match self.dimensions[index].1 {
            0 => self.records,
            length => length,
        }
    }

    /// The bytes between one record's values of a variable and the next: the sum of the record
    /// variables' sizes, or the one record variable's unpadded size when there is only one, as
    /// the specification says.
    fn record_size(&self) -> u64 {
        let records: Vec<&Variable> = self
            .variables
            .iter()
            .filter(|variable| self.is_record(variable))
            .collect();
        match records[..] {
            [only] => self.values(only).saturating_mul(only.kind.size() as u64),
            _ => records
                .iter()
                .fold(0_u64, |sum, variable| sum.saturating_add(variable.size)),
        }
    }

    /// The number of values in one record of `variable`, or in all of it when it is not a
    /// record variable.
    fn values(&self, variable: &Variable) -> u64 {
        let skip = usize::from(self.is_record(variable));
        variable.dimensions[skip..]
            .iter()
            .fold(1_u64, |count, &dimension| {
                count.saturating_mul(self.length(dimension))
            })
    }
}

/// The big-endian value `bytes` of `kind`, a number type, as an `f64`, which holds every value
/// of the classic number types exactly.
fn number(bytes: &[u8], kind: Kind) -> f64 {
    match kind {
        Kind::Byte | Kind::Char => f64::from(i8::from_be_bytes([bytes[0]])),
        Kind::Short => f64::from(i16::from_be_bytes([bytes[0], bytes[1]])),
        Kind::Int => f64::from(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])),
        Kind::Float => f64::from(f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])),
        Kind::Double => {
            let mut word = [0_u8; 8];
            word.copy_from_slice(bytes);
            f64::from_be_bytes(word)
        }
    }
}

/// The header as lines of text the standard library's `netCDF4` module parses, each a tag and
/// fields separated by tabs, in the file's order:
///
/// - `D`, a dimension's name, its length (the number of records for the unlimited one), and
///   `1` when it is unlimited or `0`;
/// - `V`, a variable's name, its type as `netCDF4` names it (`f4`), and its dimensions' names
///   separated by commas;
/// - `A`, the variable an attribute belongs to (empty for the file's own), its kind (`str`,
///   `int` or `float`), its number of values, its name, and its values: the text, or each
///   number as Python prints a `float` or an `int`, separated by commas.
pub(crate) fn describe(header: &Header) -> Vec<String> {
    let mut lines = Vec::new();
    for (index, (name, length)) in header.dimensions.iter().enumerate() {
        let unlimited = u8::from(*length == 0);
        lines.push(format!("D\t{name}\t{}\t{unlimited}", header.length(index)));
    }
    for attribute in &header.attributes {
        lines.push(attribute_line("", attribute));
    }
    for variable in &header.variables {
        let spans: Vec<&str> = variable
            .dimensions
            .iter()
            .map(|&dimension| header.dimensions[dimension].0.as_str())
            .collect();
        lines.push(format!(
            "V\t{}\t{}\t{}",
            variable.name,
            variable.kind.name(),
            spans.join(",")
        ));
        for attribute in &variable.attributes {
            lines.push(attribute_line(&variable.name, attribute));
        }
    }
    lines
}

/// The `A` line of `attribute`, of the variable named `owner` or of the file when it is empty.
fn attribute_line(owner: &str, attribute: &Attribute) -> String {
    let (kind, count, text) = match &attribute.values {
        Values::Text(text) => ("str", 1, text.clone()),
        Values::Numbers(numbers) => {
            let integer = matches!(attribute.kind, Kind::Byte | Kind::Short | Kind::Int);
            let shown: Vec<String> = numbers
                .iter()
                .map(|&number| {
                    if integer {
                        #[allow(
                            clippy::cast_possible_truncation,
                            reason = "the value is a classic integer, exact in an f64"
                        )]
                        let whole = number as i64;
                        whole.to_string()
                    } else {
                        python_float(number)
                    }
                })
                .collect();
            let kind = if integer { "int" } else { "float" };
            (kind, numbers.len(), shown.join(","))
        }
    };
    format!("A\t{owner}\t{kind}\t{count}\t{}\t{text}", attribute.name)
}

/// `value` as Python's `repr` prints a `float`: the shortest text that reads back to it, with a
/// `.0` on a whole number, an exponent of at least two digits and a sign, and `inf`, `-inf` and
/// `nan` as Python spells them.
fn python_float(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "inf" } else { "-inf" }.to_owned();
    }
    let text = format!("{value:?}");
    match text.split_once('e') {
        Some((digits, exponent)) => {
            let (sign, magnitude) = exponent
                .strip_prefix('-')
                .map_or(("+", exponent), |magnitude| ("-", magnitude));
            format!("{digits}e{sign}{magnitude:0>2}")
        }
        None => text,
    }
}

/// Reads every value of `variable` of `header`, in the file at `path`, into `memory` as
/// elements of `pixel`, after checking that `pixel` can hold them: with `scale_factor` and
/// `add_offset` applied, as `netCDF4` applies them, for a float `pixel`. `memory` holds one
/// element for each value of the variable, row-major.
///
/// # Errors
///
/// Fails as the operating system does, and with `INVALID_DATA` when `pixel` cannot hold the
/// values, they are scaled and `pixel` is an integer type, or the file ends before they do.
pub(crate) fn read_variable(
    path: &str,
    header: &Header,
    variable: &Variable,
    pixel: Pixel,
    memory: &mut [u8],
) -> Result<(), OsFailure> {
    let failed = |error: io::Error| OsFailure::from_io(&error, path);
    let invalid = |message: String| OsFailure::new(ErrorKind::InvalidData, message, path);
    let scale = scaling(variable);
    let float = matches!(pixel, Pixel::F32 | Pixel::F64);
    if !variable.kind.fits(pixel) || (scale.is_some() && !float) {
        let reason = if scale.is_some() && variable.kind.fits(pixel) {
            " scaled by scale_factor or add_offset"
        } else {
            ""
        };
        return Err(invalid(format!(
            "the NetCDF variable {} holds {}{reason}, which does not read as `{}`; read it as a \
             float array",
            variable.name,
            variable.kind.name(),
            pixel.name()
        )));
    }
    let width = variable.kind.size();
    let chunk = header.values(variable).saturating_mul(width as u64);
    let starts: Vec<u64> = if header.is_record(variable) {
        let step = header.record_size();
        (0..header.records)
            .map(|record| variable.begin.saturating_add(record.saturating_mul(step)))
            .collect()
    } else {
        vec![variable.begin]
    };
    let mut file = fs::File::open(path).map_err(failed)?;
    let mut elements = memory.chunks_exact_mut(pixel.size());
    // A piece of whole values at a time, so a large variable needs no second copy in memory.
    let piece = BUFFER - BUFFER % width;
    let mut stored = vec![0_u8; piece];
    for start in starts {
        file.seek(SeekFrom::Start(start)).map_err(failed)?;
        let mut left = chunk;
        while left > 0 {
            let size = usize::try_from(left).map_or(piece, |left| left.min(piece));
            file.read_exact(&mut stored[..size]).map_err(|error| {
                if error.kind() == io::ErrorKind::UnexpectedEof {
                    invalid(format!(
                        "the file ends inside the NetCDF variable {}",
                        variable.name
                    ))
                } else {
                    failed(error)
                }
            })?;
            for (value, element) in stored[..size].chunks_exact(width).zip(&mut elements) {
                convert(value, variable.kind, scale, pixel, element);
            }
            left -= size as u64;
        }
    }
    Ok(())
}

/// How a variable's values are scaled, as `netCDF4` scales them.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Scale {
    /// `scale_factor`, when the variable has one.
    factor: Option<f64>,
    /// `add_offset`, when the variable has one.
    offset: Option<f64>,
    /// Whether `NumPy` computes in 32-bit floats: the stored type is `i1`, `i2` or `f4` and every
    /// factor present is an `NC_FLOAT`, so the stored array and the factors promote to `float32`.
    /// Any other mix promotes to `float64`.
    single: bool,
}

impl Scale {
    /// `value` multiplied by the factor and then the offset added, each only when present, in
    /// the precision `NumPy` computes in.
    fn apply(self, value: f64) -> f64 {
        if self.single {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "the value and factors are f32 values, exact in an f32"
            )]
            let (mut scaled, factor, offset) = (
                value as f32,
                self.factor.map(|factor| factor as f32),
                self.offset.map(|offset| offset as f32),
            );
            if let Some(factor) = factor {
                scaled *= factor;
            }
            if let Some(offset) = offset {
                scaled += offset;
            }
            f64::from(scaled)
        } else {
            let mut scaled = value;
            if let Some(factor) = self.factor {
                scaled *= factor;
            }
            if let Some(offset) = self.offset {
                scaled += offset;
            }
            scaled
        }
    }
}

/// How `variable`'s values are scaled, or `None` when it has neither `scale_factor` nor
/// `add_offset`. A factor is its attribute's first number; a text attribute is not one.
fn scaling(variable: &Variable) -> Option<Scale> {
    let find = |name: &str| {
        variable
            .attributes
            .iter()
            .find(|attribute| attribute.name == name)
            .and_then(|attribute| match &attribute.values {
                Values::Numbers(numbers) => numbers.first().map(|&value| (value, attribute.kind)),
                Values::Text(_) => None,
            })
    };
    let factor = find("scale_factor");
    let offset = find("add_offset");
    if factor.is_none() && offset.is_none() {
        return None;
    }
    let single = matches!(variable.kind, Kind::Byte | Kind::Short | Kind::Float)
        && [factor, offset]
            .iter()
            .flatten()
            .all(|&(_, kind)| kind == Kind::Float);
    Some(Scale {
        factor: factor.map(|(value, _)| value),
        offset: offset.map(|(value, _)| value),
        single,
    })
}

/// Writes into `element` the value of one stored big-endian value `stored` of `kind` as an
/// element of `pixel`, which [`read_variable`] has checked holds it, scaled by `scale`.
fn convert(stored: &[u8], kind: Kind, scale: Option<Scale>, pixel: Pixel, element: &mut [u8]) {
    let mut value = number(stored, kind);
    if let Some(scale) = scale {
        value = scale.apply(value);
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the checked kind fits the integer pixel, and f32 is the type asked for"
    )]
    match pixel {
        Pixel::I8 => element.copy_from_slice(&(value as i8).to_ne_bytes()),
        Pixel::I16 => element.copy_from_slice(&(value as i16).to_ne_bytes()),
        Pixel::I32 => element.copy_from_slice(&(value as i32).to_ne_bytes()),
        Pixel::I64 => element.copy_from_slice(&(value as i64).to_ne_bytes()),
        Pixel::F32 => element.copy_from_slice(&(value as f32).to_ne_bytes()),
        Pixel::F64 => element.copy_from_slice(&value.to_ne_bytes()),
        Pixel::U8 | Pixel::U16 | Pixel::U32 | Pixel::U64 => {}
    }
}

#[cfg(test)]
mod tests;
