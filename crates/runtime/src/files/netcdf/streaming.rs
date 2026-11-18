//! Changing a `NetCDF` classic file's definitions without holding its values (decision 0047,
//! row 3.14.7): a new dimension, variable or attribute changes the header's length, which moves
//! every variable, so the file is written again in its new layout. The values stream from the
//! old file to the new one through a fixed buffer, so a definition costs one pass over the file
//! and a few megabytes of memory whatever the file's size. The new file is written beside the
//! old one and renamed over it, as [`replace`] does, so a crash leaves the old file or the new.

use super::storage::{BUFFER, fill, laid_out, replace, slot};
use super::{Header, Variable};
use crate::files::{ErrorKind, OsFailure};
use std::fs;
use std::io::{self, BufReader, Read, Write};

/// Checks that the file at `path`, `length` bytes long, holds every value `header` says it
/// does, so that a damaged header is reported, not read past the file's end.
///
/// # Errors
///
/// `INVALID_DATA` naming the first variable whose values the file ends inside.
pub(super) fn check(path: &str, header: &Header, length: u64) -> Result<(), OsFailure> {
    let step = header.record_size();
    for variable in &header.variables {
        let size = slot(header, variable) as u64;
        let count = if header.is_record(variable) {
            header.records
        } else {
            1
        };
        if size == 0 || count == 0 {
            continue;
        }
        let end = (count - 1)
            .checked_mul(step)
            .and_then(|last| variable.begin.checked_add(last))
            .and_then(|last| last.checked_add(size));
        if end.is_none_or(|end| end > length) {
            return Err(OsFailure::new(
                ErrorKind::InvalidData,
                format!("the file ends inside the NetCDF variable {}", variable.name),
                path,
            ));
        }
    }
    Ok(())
}

/// Writes the file at `path` again with the header `new`: the values of each variable `old`,
/// the file's header now, already has are copied from the file, and a variable `new` adds is
/// filled with its fill value. `old` is `None` for a new, empty file. A variable keeps its place
/// in the list, so the `n`th variable of `old` is the `n`th of `new`.
///
/// When a second record variable joins, the first one's slots, unpadded while it was the only
/// one, grow to a multiple of 4. The C library moves each record as a block of the new slot's
/// size from where the old record began, so the padding holds the bytes that followed it, and
/// zeros past the file's end; this does the same.
///
/// # Errors
///
/// `INVALID_DATA` with `netCDF4`'s message when the new layout breaks the format's limits, and
/// what the operating system gives otherwise; the old file is then left as it was.
pub(super) fn rewrite(path: &str, old: Option<&Header>, new: &Header) -> Result<(), OsFailure> {
    let laid =
        laid_out(new).map_err(|message| OsFailure::new(ErrorKind::InvalidData, &message, path))?;
    let source = old
        .map(|old| fs::File::open(path).map(|file| Source::new(old, file)))
        .transpose()
        .map_err(|error| OsFailure::from_io(&error, path))?;
    replace(path, |out| copy(source, &laid, out)).map_err(|error| OsFailure::from_io(&error, path))
}

/// Writes the file `laid` describes to `out`: its header, then each variable's values, from
/// `source` or filled.
fn copy(mut source: Option<Source<'_>>, laid: &Header, out: &mut dyn Write) -> io::Result<()> {
    out.write_all(&super::encoding::encode_header(laid))?;
    let mut write = |source: &mut Option<Source<'_>>, index: usize, record: u64| {
        let variable = &laid.variables[index];
        let size = slot(laid, variable) as u64;
        match source
            .as_mut()
            .and_then(|source| source.at(index, record).map(|at| (source, at)))
        {
            Some((source, at)) => source.copy(at, size, out),
            None => repeat(&fill(variable), size, out),
        }
    };
    for (index, variable) in laid.variables.iter().enumerate() {
        if !laid.is_record(variable) {
            write(&mut source, index, 0)?;
        }
    }
    for record in 0..laid.records {
        for (index, variable) in laid.variables.iter().enumerate() {
            if laid.is_record(variable) {
                write(&mut source, index, record)?;
            }
        }
    }
    Ok(())
}

/// The old file, read forward through a buffer.
struct Source<'a> {
    /// Its header.
    header: &'a Header,
    /// The bytes between one record and the next in it.
    step: u64,
    /// The file, buffered.
    file: BufReader<fs::File>,
    /// Where the buffered reader stands in the file.
    position: u64,
}

impl<'a> Source<'a> {
    /// The file `file`, whose header is `header`, read from its start.
    fn new(header: &'a Header, file: fs::File) -> Self {
        Self {
            header,
            step: header.record_size(),
            file: BufReader::with_capacity(BUFFER, file),
            position: 0,
        }
    }

    /// Where the values of the variable at `index`, in its `record`th record if it has
    /// records, begin in the file: `None` for a variable the file does not have yet.
    fn at(&self, index: usize, record: u64) -> Option<u64> {
        let variable: &Variable = self.header.variables.get(index)?;
        Some(if self.header.is_record(variable) {
            variable.begin + record * self.step
        } else {
            variable.begin
        })
    }

    /// Copies `size` bytes from `at` in the file to `out`, with zeros for any past its end.
    fn copy(&mut self, at: u64, size: u64, out: &mut dyn Write) -> io::Result<()> {
        // Records are read in order, so the moves are short and mostly forward, and a short
        // move stays inside the buffer.
        let offset = i64::try_from(i128::from(at) - i128::from(self.position))
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "an offset past 2^63"))?;
        self.file.seek_relative(offset)?;
        let copied = io::copy(&mut self.file.by_ref().take(size), out)?;
        self.position = at + copied;
        repeat(&[0], size - copied, out)
    }
}

/// Writes `size` bytes of `pattern` repeated to `out`, a buffer at a time.
fn repeat(pattern: &[u8], size: u64, out: &mut dyn Write) -> io::Result<()> {
    // No longer than needed: most writes here are one small slot.
    let length = usize::try_from(size).map_or(BUFFER, |size| size.min(BUFFER));
    let block: Vec<u8> = pattern.iter().copied().cycle().take(length).collect();
    let mut left = size;
    while left > 0 {
        // The buffer is a multiple of every pattern's length (1, 2, 4 or 8 bytes), so each
        // block starts where a value starts.
        let count = left.min(block.len() as u64);
        out.write_all(&block[..usize::try_from(count).unwrap_or(length)])?;
        left -= count;
    }
    Ok(())
}
