//! A `NetCDF` classic file held in memory for the `writing` module (decision 0047): its header
//! and every variable's stored bytes, read from a file and written to one again in the layout
//! the C library `netCDF4` calls gives a file: the header, the fixed-size variables, then the
//! records.

use super::encoding::encode_header;
use super::{Header, Variable, read_header};
use crate::files::{ErrorKind, OsFailure};
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};

/// A file in memory: its header and every variable's slots.
pub(super) struct Contents {
    /// The header. The variables' sizes and beginnings are worked out again when it is written.
    pub(super) header: Header,
    /// For each variable, in the header's order, its stored bytes: one slot for a fixed-size
    /// variable, and one slot for each record of a record variable, each [`slot`] bytes long.
    pub(super) slots: Vec<Vec<u8>>,
}

/// The bytes of one slot of `variable` of `header`: its values' size padded to a multiple of
/// 4, or unpadded for the only record variable.
pub(super) fn slot(header: &Header, variable: &Variable) -> usize {
    let size = header
        .values(variable)
        .saturating_mul(variable.kind.size() as u64);
    let only = header.is_record(variable)
        && header
            .variables
            .iter()
            .filter(|other| header.is_record(other))
            .count()
            == 1;
    let size = if only {
        size
    } else {
        size.checked_next_multiple_of(4).unwrap_or(u64::MAX)
    };
    usize::try_from(size).unwrap_or(usize::MAX)
}

/// Reads the whole file at `path` into memory.
pub(super) fn load(path: &str) -> Result<Contents, OsFailure> {
    let header = read_header(path)?;
    let failed = |error: io::Error| OsFailure::from_io(&error, path);
    let mut file = fs::File::open(path).map_err(failed)?;
    let length = file.metadata().map_err(failed)?.len();
    let step = header.record_size();
    let mut slots = Vec::new();
    for variable in &header.variables {
        let short = || {
            OsFailure::new(
                ErrorKind::InvalidData,
                format!("the file ends inside the NetCDF variable {}", variable.name),
                path,
            )
        };
        let size = slot(&header, variable);
        let count = if header.is_record(variable) {
            header.records
        } else {
            1
        };
        if size == 0 || count == 0 {
            slots.push(Vec::new());
            continue;
        }
        // The header is checked against the file's length before anything is allocated, so a
        // corrupt header that claims more than the file holds is an error, not a huge
        // allocation: every slot read lies inside the file.
        let end = (count - 1)
            .checked_mul(step)
            .and_then(|last| variable.begin.checked_add(last))
            .and_then(|last| last.checked_add(size as u64))
            .filter(|&end| end <= length)
            .ok_or_else(short)?;
        debug_assert!(end <= length);
        let total = usize::try_from(count)
            .ok()
            .and_then(|count| count.checked_mul(size))
            .ok_or_else(short)?;
        let mut bytes = vec![0_u8; total];
        for (record, chunk) in bytes.chunks_mut(size).enumerate() {
            // Within the file, as checked above.
            let start = variable.begin + record as u64 * step;
            file.seek(SeekFrom::Start(start)).map_err(failed)?;
            file.read_exact(chunk).map_err(|error| {
                if error.kind() == io::ErrorKind::UnexpectedEof {
                    short()
                } else {
                    failed(error)
                }
            })?;
        }
        slots.push(bytes);
    }
    Ok(Contents { header, slots })
}

/// Writes `contents` to `path`, laid out as the module's documentation says, replacing the file.
pub(super) fn save(path: &str, contents: &Contents) -> Result<(), OsFailure> {
    let invalid = |message: &str| OsFailure::new(ErrorKind::InvalidData, message, path);
    let too_large = || invalid("NetCDF: One or more variable sizes violate format constraints");
    let mut header = contents.header.clone();
    let slots: Vec<usize> = header
        .variables
        .iter()
        .map(|variable| slot(&header, variable))
        .collect();
    for (variable, &size) in header.variables.iter_mut().zip(&slots) {
        variable.size = (size as u64)
            .checked_next_multiple_of(4)
            .ok_or_else(too_large)?;
    }
    let length = encode_header(&header).len() as u64;
    let mut at = length;
    // The fixed variables first, then the record variables, each where the last one ends.
    for records in [false, true] {
        for (variable, &size) in header.variables.iter_mut().zip(&slots) {
            let is_record = variable
                .dimensions
                .first()
                .is_some_and(|&dimension| contents.header.dimensions[dimension].1 == 0);
            if is_record == records {
                variable.begin = at;
                at = at.checked_add(size as u64).ok_or_else(too_large)?;
            }
        }
    }
    if !header.wide
        && header
            .variables
            .iter()
            .any(|variable| variable.begin > u64::from(u32::MAX))
    {
        return Err(invalid(
            "the file is too large for the classic format; write it with \
             format=\"NETCDF3_64BIT_OFFSET\"",
        ));
    }
    let mut bytes = encode_header(&header);
    for (index, variable) in header.variables.iter().enumerate() {
        if !header.is_record(variable) {
            bytes.extend_from_slice(&contents.slots[index]);
        }
    }
    for record in 0..usize::try_from(header.records).unwrap_or(0) {
        for (index, variable) in header.variables.iter().enumerate() {
            if header.is_record(variable) {
                let size = slots[index];
                bytes.extend_from_slice(&contents.slots[index][record * size..(record + 1) * size]);
            }
        }
    }
    fs::write(path, bytes).map_err(|error| OsFailure::from_io(&error, path))
}
