//! A `NetCDF` classic file held in memory for the `writing` module (decision 0047): its header
//! and every variable's stored bytes, read from a file and written to one again in the layout
//! the C library `netCDF4` calls gives a file: the header, the fixed-size variables, then the
//! records.

use super::encoding::encode_header;
use super::{Header, Variable, read_header};
use crate::files::{ErrorKind, OsFailure};
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

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
    if header.netcdf4.is_some() {
        return Err(OsFailure::new(
            ErrorKind::InvalidData,
            "the file is a NetCDF-4 (HDF5) file, which is not written yet",
            path,
        ));
    }
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
    replace(path, &bytes).map_err(|error| OsFailure::from_io(&error, path))
}

/// Makes `bytes` the file at `path` in one step: they are written to a new file beside it,
/// flushed to the disk, then renamed over it, so a program that stops partway, a crash or a
/// full disk leave the old file or the new one, never one cut short. Each change rewrites the
/// whole file, so a file cut short would lose everything written before it, not just the
/// change.
///
/// The new file is `.<name>.baryon-<process>` in the same directory, since a rename is atomic
/// only within one file system, and it takes the old file's permissions. It is removed when
/// anything fails. A symbolic link at `path` is replaced by the file, not followed.
///
/// # Errors
///
/// The first error the operating system gives: making, writing or flushing the new file, or
/// renaming it.
fn replace(path: &str, bytes: &[u8]) -> io::Result<()> {
    let target = Path::new(path);
    let name = target
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the path names no file"))?;
    let mut temporary = OsString::from(".");
    temporary.push(name);
    temporary.push(format!(".baryon-{}", std::process::id()));
    let temporary = target.with_file_name(temporary);
    let written = (|| {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        if let Ok(old) = fs::metadata(target) {
            file.set_permissions(old.permissions())?;
        }
        file.sync_all()?;
        fs::rename(&temporary, target)
    })();
    if written.is_err() {
        // The first error is the one to report; the new file may not exist at all.
        let _ = fs::remove_file(&temporary);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::replace;
    use std::fs;
    use std::path::PathBuf;

    /// A new empty directory for one test, named after it and this process.
    fn directory(test: &str) -> PathBuf {
        let name = format!("baryon-netcdf-storage-{}-{test}", std::process::id());
        let path = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    /// The names of the files in `directory`, sorted.
    fn names(directory: &PathBuf) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_file_is_replaced_whole_and_nothing_is_left_beside_it() {
        let directory = directory("replaced");
        let path = directory.join("out.nc");
        fs::write(&path, b"old bytes, longer than the new").unwrap();
        replace(path.to_str().unwrap(), b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        replace(directory.join("fresh.nc").to_str().unwrap(), b"made").unwrap();
        assert_eq!(names(&directory), ["fresh.nc", "out.nc"]);
        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_failed_replace_leaves_the_old_file_and_no_new_one() {
        let directory = directory("failed");
        // A rename cannot put a file over a directory, so the new file is written and then
        // the rename fails.
        let target = directory.join("data.nc");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("inside"), b"kept").unwrap();
        assert!(replace(target.to_str().unwrap(), b"new").is_err());
        assert_eq!(names(&directory), ["data.nc"]);
        assert_eq!(fs::read(target.join("inside")).unwrap(), b"kept");
        let missing = directory.join("missing").join("data.nc");
        assert!(replace(missing.to_str().unwrap(), b"new").is_err());
        assert_eq!(names(&directory), ["data.nc"]);
        fs::remove_dir_all(&directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_new_file_keeps_the_old_ones_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let directory = directory("permissions");
        let path = directory.join("out.nc");
        fs::write(&path, b"old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        replace(path.to_str().unwrap(), b"new").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o640);
        fs::remove_dir_all(&directory).unwrap();
    }
}
