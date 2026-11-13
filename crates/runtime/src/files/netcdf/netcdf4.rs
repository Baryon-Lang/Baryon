//! NetCDF-4 files (decision 0052, point 6): a `NetCDF` file stored as HDF5, read through the
//! native HDF5 reader of [`crate::files::hdf5`] and described with the same [`Header`] a classic
//! file has, so `netCDF4.Dataset` reads both alike.
//!
//! The `NetCDF` C library stores each variable of the root group as an HDF5 dataset of the same
//! name. Each dimension is a dataset marked as a *dimension scale* (its attribute `CLASS` is
//! `DIMENSION_SCALE`) and numbered by its attribute `_Netcdf4Dimid`; a dimension that is not
//! also a variable is a placeholder dataset whose `NAME` attribute starts with
//! [`NOT_A_VARIABLE`]. Each variable names its dimensions' numbers in its attribute
//! `_Netcdf4Coordinates`. A dimension whose dataset has no limit along its axis is unlimited,
//! and its length is the longest any variable along it reaches. The attributes the library
//! keeps for itself, [`HIDDEN`], are not shown, as `netCDF4` does not show them.
//!
//! A deflated variable's chunks are inflated by the standard library's `zlib` (decision 0049),
//! as `h5by` inflates them: [`chunks`] lists them, the library inflates them, and [`read`]
//! takes them back.
//!
//! Cost: opening reads every HDF5 object header once, as `h5by.File` does. A read gives the
//! HDF5 reader, for each axis, the upward slice that picks exactly the positions picked, reads
//! them as `f64`s, then copies them into the array in the order picked, so a read costs the
//! values picked alone, however far apart they lie.

use super::reading::{Scale, place};
use super::{Attribute, Header, Kind, Selection, Values, Variable};
use crate::files::fits::Pixel;
use crate::files::hdf5::{self, Datatype, Hdf5, Inflate, Object};
use crate::files::netcdf::{Item, SubscriptFailure};
use crate::files::{ErrorKind, OsFailure};

/// The start of the `NAME` attribute of a dimension's placeholder dataset, which is not a
/// variable.
const NOT_A_VARIABLE: &str = "This is a netCDF dimension but not a netCDF variable.";

/// The attributes the `NetCDF` library writes for itself, which `netCDF4` does not list.
const HIDDEN: [&str; 10] = [
    "CLASS",
    "DIMENSION_LIST",
    "NAME",
    "REFERENCE_LIST",
    "_IsNetcdf4",
    "_NCProperties",
    "_Netcdf4Coordinates",
    "_Netcdf4Dimid",
    "_SuperblockVersion",
    "_nc3_strict",
];

/// What a NetCDF-4 file adds to its [`Header`]: the HDF5 objects, and each dimension's length.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Netcdf4 {
    /// The file's HDF5 objects.
    file: Hdf5,
    /// Each dimension's length, in the order of [`Header::dimensions`]: an unlimited one's is
    /// the longest any variable reaches along it.
    pub(super) lengths: Vec<u64>,
}

/// Reads the header of the NetCDF-4 file at `path`: its dimensions in the order of their
/// numbers, the root group's variables and attributes in name order, and its HDF5 objects for
/// the reads to come. Groups below the root are not read.
///
/// # Errors
///
/// Fails as [`hdf5::read_file`] does, and with `INVALID_DATA` when a variable or an attribute
/// holds a type the classic formats do not have, or the file's dimensions are not as the
/// `NetCDF` library writes them.
pub(super) fn header(path: &str) -> Result<Header, OsFailure> {
    let file = hdf5::read_file(path)?;
    let invalid = |message: String| OsFailure::new(ErrorKind::InvalidData, message, path);
    let root: Vec<&Object> = file
        .objects
        .iter()
        .filter(|object| object.path.matches('/').count() == 1 && object.dataset.is_some())
        .collect();
    let mut scales: Vec<(i128, &Object)> = Vec::new();
    for object in &root {
        if text(object, "CLASS") == Some("DIMENSION_SCALE") {
            let Some(&number) = integers(object, "_Netcdf4Dimid").and_then(<[i128]>::first) else {
                return Err(invalid(format!(
                    "the NetCDF-4 dimension {} has no number (_Netcdf4Dimid)",
                    name(object)
                )));
            };
            scales.push((number, object));
        }
    }
    scales.sort_by_key(|&(number, _)| number);
    let numbers: Vec<i128> = scales.iter().map(|&(number, _)| number).collect();
    let mut dimensions = Vec::new();
    let mut lengths = Vec::new();
    for &(_, object) in &scales {
        let (length, unlimited) = extent(object, 0);
        dimensions.push((name(object).to_owned(), if unlimited { 0 } else { length }));
        lengths.push(length);
    }
    let mut variables = Vec::new();
    for object in root {
        if text(object, "NAME").is_some_and(|text| text.starts_with(NOT_A_VARIABLE)) {
            continue;
        }
        let variable = variable(object, &numbers).map_err(invalid)?;
        for (axis, &dimension) in variable.dimensions.iter().enumerate() {
            lengths[dimension] = lengths[dimension].max(extent(object, axis).0);
        }
        variables.push(variable);
    }
    let records = dimensions
        .iter()
        .zip(&lengths)
        .filter(|((_, length), _)| *length == 0)
        .map(|(_, &length)| length)
        .max()
        .unwrap_or(0);
    let attributes = match file.find("/") {
        Some(object) => attributes(object, "the NetCDF-4 file").map_err(invalid)?,
        None => Vec::new(),
    };
    Ok(Header {
        wide: false,
        records,
        dimensions,
        attributes,
        variables,
        netcdf4: Some(Box::new(Netcdf4 { file, lengths })),
    })
}

/// The variable whose dataset is `object`, its dimensions found among the dimension numbers
/// `numbers`, which [`Header::dimensions`] lists in that order.
fn variable(object: &Object, numbers: &[i128]) -> Result<Variable, String> {
    let name = name(object);
    let Some(dataset) = &object.dataset else {
        return Err(format!("the NetCDF-4 variable {name} is not a dataset"));
    };
    let kind = match dataset.datatype {
        Datatype::Integer {
            size, signed: true, ..
        } => match size {
            1 => Some(Kind::Byte),
            2 => Some(Kind::Short),
            4 => Some(Kind::Int),
            _ => None,
        },
        Datatype::Float { size: 4, .. } => Some(Kind::Float),
        Datatype::Float { size: 8, .. } => Some(Kind::Double),
        Datatype::Text { size: 1 } => Some(Kind::Char),
        _ => None,
    };
    let Some(kind) = kind else {
        return Err(format!(
            "the NetCDF-4 variable {name} holds {}, which is not read yet",
            dataset.datatype.name()
        ));
    };
    let coordinates = match integers(object, "_Netcdf4Coordinates") {
        Some(coordinates) => coordinates.to_vec(),
        None if dataset.shape.is_empty() => Vec::new(),
        None => match integers(object, "_Netcdf4Dimid") {
            Some(own) => own.to_vec(),
            None => {
                return Err(format!(
                    "the NetCDF-4 variable {name} does not list its dimensions \
                     (_Netcdf4Coordinates), which is not read yet"
                ));
            }
        },
    };
    let mut spans = Vec::new();
    for number in coordinates {
        let Some(index) = numbers.iter().position(|&known| known == number) else {
            return Err(format!(
                "the NetCDF-4 variable {name} names a dimension the file does not have"
            ));
        };
        spans.push(index);
    }
    if spans.len() != dataset.shape.len() {
        return Err(format!(
            "the NetCDF-4 variable {name} has {} dimensions but a rank of {}",
            spans.len(),
            dataset.shape.len()
        ));
    }
    Ok(Variable {
        name: name.to_owned(),
        dimensions: spans,
        attributes: attributes(object, &format!("the NetCDF-4 variable {name}"))?,
        kind,
        begin: 0,
        size: 0,
    })
}

/// The attributes of `object` that `netCDF4` shows, in name order, `owner` naming it in a
/// message: text as `NC_CHAR`, integers as `NC_INT` and floats as `NC_FLOAT` or `NC_DOUBLE` by
/// their size.
fn attributes(object: &Object, owner: &str) -> Result<Vec<Attribute>, String> {
    let mut shown = Vec::new();
    for attribute in &object.attributes {
        if HIDDEN.contains(&attribute.name.as_str()) {
            continue;
        }
        let unread = || {
            format!(
                "the attribute {} of {owner} holds {} values of {}, which is not read yet",
                attribute.name,
                attribute.shape.iter().product::<u64>(),
                attribute.datatype.name()
            )
        };
        let (kind, values) = match &attribute.values {
            hdf5::Values::Text(texts) => match &texts[..] {
                [text] => (Kind::Char, Values::Text(text.clone())),
                _ => return Err(unread()),
            },
            #[allow(
                clippy::cast_precision_loss,
                reason = "an attribute's integers are shown as `f64`s, exact up to 2^53"
            )]
            hdf5::Values::Integers(integers) => (
                Kind::Int,
                Values::Numbers(integers.iter().map(|&value| value as f64).collect()),
            ),
            hdf5::Values::Floats(floats) => {
                let kind = match attribute.datatype {
                    Datatype::Float { size: 4, .. } => Kind::Float,
                    _ => Kind::Double,
                };
                (kind, Values::Numbers(floats.clone()))
            }
            hdf5::Values::Unread => return Err(unread()),
        };
        shown.push(Attribute {
            name: attribute.name.clone(),
            kind,
            values,
        });
    }
    Ok(shown)
}

/// The name of `object`, its path without the leading `/`.
fn name(object: &Object) -> &str {
    object.path.trim_start_matches('/')
}

/// The single text value of the attribute `wanted` of `object`, if it has one.
fn text<'a>(object: &'a Object, wanted: &str) -> Option<&'a str> {
    object
        .attributes
        .iter()
        .find(|attribute| attribute.name == wanted)
        .and_then(|attribute| match &attribute.values {
            hdf5::Values::Text(texts) => texts.first().map(String::as_str),
            _ => None,
        })
}

/// The integers of the attribute `wanted` of `object`, if it has them.
fn integers<'a>(object: &'a Object, wanted: &str) -> Option<&'a [i128]> {
    object
        .attributes
        .iter()
        .find(|attribute| attribute.name == wanted)
        .and_then(|attribute| match &attribute.values {
            hdf5::Values::Integers(values) => Some(values.as_slice()),
            _ => None,
        })
}

/// The length of `object`'s dataset along `axis`, and whether it has no limit there.
fn extent(object: &Object, axis: usize) -> (u64, bool) {
    object.dataset.as_ref().map_or((0, false), |dataset| {
        (
            dataset.shape.get(axis).copied().unwrap_or(0),
            dataset.most.get(axis) == Some(&u64::MAX),
        )
    })
}

/// The dataset of `variable`, a variable of the NetCDF-4 file at `path` described by
/// `netcdf4`, as long along each axis as its dimensions are.
///
/// # Errors
///
/// `INVALID_DATA` when the file has no such dataset, or it is shorter than its unlimited
/// dimension, which is not read yet.
fn dataset<'a>(
    path: &str,
    netcdf4: &'a Netcdf4,
    variable: &Variable,
) -> Result<&'a hdf5::Dataset, OsFailure> {
    let invalid = |message: String| OsFailure::new(ErrorKind::InvalidData, message, path);
    let name = &variable.name;
    let Some(dataset) = netcdf4
        .file
        .find(name)
        .and_then(|object| object.dataset.as_ref())
    else {
        return Err(invalid(format!("the NetCDF file has no variable {name}")));
    };
    let lengths: Vec<u64> = variable
        .dimensions
        .iter()
        .map(|&dimension| netcdf4.lengths[dimension])
        .collect();
    if dataset.shape != lengths {
        return Err(invalid(format!(
            "the NetCDF-4 variable {name} is shorter than its unlimited dimension, which is not \
             read yet"
        )));
    }
    Ok(dataset)
}

/// The HDF5 subscript that reads `selection` of the variable `name`: for each axis, the slice
/// of [`block`] and where each position picked lies in what it reads.
///
/// # Errors
///
/// `INVALID_DATA` when an axis's positions are in no equal steps, which no subscript of
/// integers and slices picks.
fn blocks(
    path: &str,
    name: &str,
    selection: &Selection,
) -> Result<(Vec<Item>, Vec<Vec<usize>>), OsFailure> {
    let mut items = Vec::new();
    let mut places = Vec::new();
    for axis in &selection.axes {
        let Some((item, within)) = block(&axis.positions) else {
            return Err(OsFailure::new(
                ErrorKind::InvalidData,
                format!(
                    "the values picked of the NetCDF-4 variable {name} are in no equal steps, \
                     which is not read yet"
                ),
                path,
            ));
        };
        items.push(item);
        places.push(within);
    }
    Ok((items, places))
}

/// The address and stored size of each deflated chunk that a read of the values `selection`
/// picks of `variable` takes inflated, in the order [`read`] takes them: none when it picks no
/// values or the variable is not deflated.
///
/// # Errors
///
/// Fails as [`hdf5::deflated_chunks`] does, and as [`dataset`] and [`blocks`] do.
pub(super) fn chunks(
    path: &str,
    netcdf4: &Netcdf4,
    (variable, selection): (&Variable, &Selection),
) -> Result<Vec<(u64, u64)>, OsFailure> {
    let dataset = dataset(path, netcdf4, variable)?;
    if selection.count() == 0 {
        return Ok(Vec::new());
    }
    let (items, _) = blocks(path, &variable.name, selection)?;
    hdf5::deflated_chunks(path, &netcdf4.file, (&variable.name, dataset, &items))
        .map_err(|failure| plain(path, failure))
}

/// `failure` as an `OSError`: the HDF5 reader's `IndexError` and `ValueError` cannot happen
/// for the slices [`block`] makes, which lie inside the variable, so one is a damaged file.
fn plain(path: &str, failure: SubscriptFailure) -> OsFailure {
    match failure {
        SubscriptFailure::Os(failure) => failure,
        SubscriptFailure::Index(message) | SubscriptFailure::Value(message) => {
            OsFailure::new(ErrorKind::InvalidData, message, path)
        }
    }
}

/// Reads the values `selection` picks of `variable`, a variable of the NetCDF-4 file at `path`
/// described by `netcdf4`, into `memory` as elements of `pixel` scaled by `scale`, which
/// [`super::read_variable`] has checked. A deflated chunk is inflated by `inflate`, in the
/// order [`chunks`] lists them.
///
/// # Errors
///
/// Fails as [`hdf5::read_dataset`] does, as [`dataset`] and [`blocks`] do, and with
/// `INVALID_DATA` when the values picked do not fit in memory.
pub(super) fn read(
    path: &str,
    netcdf4: &Netcdf4,
    (variable, selection): (&Variable, &Selection),
    (scale, pixel): (Option<Scale>, Pixel),
    memory: &mut [u8],
    inflate: &mut Inflate<'_>,
) -> Result<(), OsFailure> {
    let dataset = dataset(path, netcdf4, variable)?;
    let name = &variable.name;
    if selection.count() == 0 {
        return Ok(());
    }
    let (items, places) = blocks(path, name, selection)?;
    let sizes: Vec<usize> = places.iter().map(Vec::len).collect();
    // As many `f64`s as values picked, which the caller's array already holds as `pixel`s.
    let mut values = Vec::new();
    let room = sizes
        .iter()
        .try_fold(8_usize, |bytes, &size| bytes.checked_mul(size))
        .filter(|&bytes| values.try_reserve_exact(bytes).is_ok());
    let Some(bytes) = room else {
        return Err(OsFailure::new(
            ErrorKind::InvalidData,
            format!("the values picked of the NetCDF-4 variable {name} do not fit in memory"),
            path,
        ));
    };
    values.resize(bytes, 0);
    hdf5::read_dataset(
        path,
        &netcdf4.file,
        (name, dataset, &items),
        Pixel::F64,
        &mut values,
        inflate,
    )
    .map_err(|failure| plain(path, failure))?;
    let mut elements = memory.chunks_exact_mut(pixel.size());
    let mut picked = vec![0_usize; places.len()];
    loop {
        let at = picked
            .iter()
            .zip(&places)
            .zip(&sizes)
            .fold(0, |at, ((&index, within), &size)| at * size + within[index]);
        let mut word = [0_u8; 8];
        word.copy_from_slice(&values[at * 8..at * 8 + 8]);
        if let Some(element) = elements.next() {
            place(f64::from_ne_bytes(word), scale, pixel, element);
        }
        // The next position picked, row-major: the last axis fastest.
        let mut axis = places.len();
        loop {
            if axis == 0 {
                return Ok(());
            }
            axis -= 1;
            picked[axis] += 1;
            if picked[axis] < places[axis].len() {
                break;
            }
            picked[axis] = 0;
        }
    }
}

/// The slice that picks exactly `positions`, none of them empty, and where each lies in what
/// it reads: a slice in steps, read upwards, when they are in equal steps either way, as every
/// subscript of integers and slices picks them; `None` for positions in no such order.
fn block(positions: &[u64]) -> Option<(Item, Vec<usize>)> {
    let low = *positions.iter().min()?;
    let step = match positions {
        [one, two, ..] => one.abs_diff(*two),
        _ => 1,
    };
    let mut within = Vec::with_capacity(positions.len());
    for &position in positions {
        let offset = position - low;
        if step == 0 || offset % step != 0 {
            return None;
        }
        within.push(usize::try_from(offset / step).ok()?);
    }
    let count = positions.len();
    let mut seen = vec![false; count];
    for &index in &within {
        if index >= count || std::mem::replace(&mut seen[index], true) {
            return None;
        }
    }
    let signed = |value: u64| i64::try_from(value).unwrap_or(i64::MAX);
    let high = low.saturating_add(step.saturating_mul(count as u64 - 1));
    let item = Item::Slice {
        start: Some(signed(low)),
        stop: Some(signed(high).saturating_add(1)),
        stride: Some(signed(step)),
    };
    Some((item, within))
}

#[cfg(test)]
mod tests;
