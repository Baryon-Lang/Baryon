//! Reading a variable's values: the values a selection picks, converted to the array's element
//! type and unpacked by `scale_factor` and `add_offset` as `netCDF4` unpacks them, from a
//! classic file's slot or, through [`super::netcdf4`], from a NetCDF-4 file's HDF5 dataset.

use super::{Header, Kind, Selection, Values, Variable, netcdf4, number};
use crate::files::fits::Pixel;
use crate::files::hdf5::Inflate;
use crate::files::{BUFFER, ErrorKind, OsFailure};
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};

/// Reads the values `selection` picks of `variable` of `header`, in the file at `path`, into
/// `memory` as elements of `pixel`, after checking that `pixel` can hold them: with
/// `scale_factor` and `add_offset` applied, as `netCDF4` applies them, for a float `pixel`.
/// `memory` holds one element for each value picked, row-major. In a classic file each stretch
/// of neighbouring values is read with one seek, so `var[:]` of a fixed variable is one read; a
/// NetCDF-4 file's variable is read as [`netcdf4::read`] reads it, its deflated chunks inflated
/// by `inflate`; a classic file's never are.
///
/// # Errors
///
/// Fails as the operating system does, and with `INVALID_DATA` when `pixel` cannot hold the
/// values, they are scaled and `pixel` is an integer type, or the file ends before they do.
pub(crate) fn read_variable(
    path: &str,
    header: &Header,
    (variable, selection): (&Variable, &Selection),
    pixel: Pixel,
    memory: &mut [u8],
    inflate: &mut Inflate<'_>,
) -> Result<(), OsFailure> {
    let failed = |error: io::Error| OsFailure::from_io(&error, path);
    let invalid = |message: String| OsFailure::new(ErrorKind::InvalidData, message, path);
    let scale = checked(variable, pixel).map_err(invalid)?;
    if let Some(netcdf4) = &header.netcdf4 {
        return netcdf4::read(
            path,
            netcdf4,
            (variable, selection),
            (scale, pixel),
            memory,
            inflate,
        );
    }
    let width = variable.kind.size() as u64;
    let record = header.is_record(variable);
    let lengths: Vec<u64> = variable.dimensions[usize::from(record)..]
        .iter()
        .map(|&dimension| header.length(dimension))
        .collect();
    let step = header.record_size();
    let mut file = fs::File::open(path).map_err(failed)?;
    let mut elements = memory.chunks_exact_mut(pixel.size());
    // A piece of whole values at a time, so a large stretch needs no second copy in memory.
    let piece = BUFFER - BUFFER % variable.kind.size();
    let mut stored = vec![0_u8; piece];
    selection.runs(record, &lengths, |at, offset, count| {
        let start = at
            .unwrap_or(0)
            .saturating_mul(step)
            .saturating_add(offset.saturating_mul(width))
            .saturating_add(variable.begin);
        file.seek(SeekFrom::Start(start)).map_err(failed)?;
        let mut left = count.saturating_mul(width);
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
            for (value, element) in stored[..size]
                .chunks_exact(variable.kind.size())
                .zip(&mut elements)
            {
                convert(value, variable.kind, scale, pixel, element);
            }
            left -= size as u64;
        }
        Ok(())
    })
}

/// The address and stored size of each deflated chunk a read of the values `selection` picks
/// of `variable` of `header`, the file at `path`, takes inflated, in the order
/// [`read_variable`] takes them: none for a classic file, whose variables are never
/// compressed.
///
/// # Errors
///
/// Fails as the operating system does, and with `INVALID_DATA` when a NetCDF-4 file's variable
/// or its chunk index is damaged or not read yet.
pub(crate) fn deflated_chunks(
    path: &str,
    header: &Header,
    (variable, selection): (&Variable, &Selection),
) -> Result<Vec<(u64, u64)>, OsFailure> {
    match &header.netcdf4 {
        Some(netcdf4) => netcdf4::chunks(path, netcdf4, (variable, selection)),
        None => Ok(Vec::new()),
    }
}

/// How `variable`'s values are scaled, after checking that an array of `pixel` can hold them:
/// `pixel` holds the variable's type, and is a float type when the values are scaled.
///
/// # Errors
///
/// The message of the `INVALID_DATA` error when `pixel` cannot hold the values.
fn checked(variable: &Variable, pixel: Pixel) -> Result<Option<Scale>, String> {
    let scale = scaling(variable);
    let float = matches!(pixel, Pixel::F32 | Pixel::F64);
    if !variable.kind.fits(pixel) || (scale.is_some() && !float) {
        let reason = if scale.is_some() && variable.kind.fits(pixel) {
            " scaled by scale_factor or add_offset"
        } else {
            ""
        };
        return Err(format!(
            "the NetCDF variable {} holds {}{reason}, which does not read as `{}`; read it as a \
             float array",
            variable.name,
            variable.kind.name(),
            pixel.name()
        ));
    }
    Ok(scale)
}

/// How a variable's values are scaled, as `netCDF4` scales them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Scale {
    /// `scale_factor`, when the variable has one.
    pub(super) factor: Option<f64>,
    /// `add_offset`, when the variable has one.
    pub(super) offset: Option<f64>,
    /// Whether `NumPy` computes in 32-bit floats: the stored type is `i1`, `i2` or `f4` and every
    /// factor present is an `NC_FLOAT`, so the stored array and the factors promote to `float32`.
    /// Any other mix promotes to `float64`.
    pub(super) single: bool,
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
pub(super) fn scaling(variable: &Variable) -> Option<Scale> {
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
    place(number(stored, kind), scale, pixel, element);
}

/// Writes into `element` the number `value`, a stored value of a type `pixel` holds, as an
/// element of `pixel`, scaled by `scale`.
pub(super) fn place(mut value: f64, scale: Option<Scale>, pixel: Pixel, element: &mut [u8]) {
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
