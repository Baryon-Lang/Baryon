//! The layout of a `NetCDF` classic file for the `writing` module (decision 0047): each
//! variable's slot and fill value, where the C library `netCDF4` calls puts each variable, and
//! the rename that makes a rewritten file the file in one step.

use super::encoding::{encode, encode_header};
use super::{Header, Kind, Values, Variable};
use std::ffi::OsString;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::Path;

/// The size of the buffers that stream a file: a megabyte, many records' worth for most files.
pub(super) const BUFFER: usize = 1 << 20;

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

/// The big-endian bytes of `variable`'s fill value: its `_FillValue` attribute's, or the
/// type's default, as the C library fills.
pub(super) fn fill(variable: &Variable) -> Vec<u8> {
    let own = variable
        .attributes
        .iter()
        .find(|attribute| attribute.name == "_FillValue" && attribute.kind == variable.kind)
        .and_then(|attribute| match &attribute.values {
            Values::Numbers(numbers) => numbers.first().copied(),
            Values::Text(_) => None,
        });
    match own {
        Some(value) => encode(value, variable.kind),
        None => match variable.kind {
            Kind::Byte => vec![0x81],
            Kind::Char => vec![0],
            Kind::Short => vec![0x80, 0x01],
            Kind::Int => vec![0x80, 0, 0, 0x01],
            Kind::Float => vec![0x7C, 0xF0, 0, 0],
            Kind::Double => vec![0x47, 0x9E, 0, 0, 0, 0, 0, 0],
        },
    }
}

/// `header` with each variable's size and beginning set as the module's layout puts them: the
/// header, the fixed-size variables, then the records, each variable where the last one ends.
///
/// # Errors
///
/// `netCDF4`'s message when a variable is too large for the header's 32-bit sizes, or a
/// classic (CDF-1) file grows past its 32-bit offsets.
pub(super) fn laid_out(header: &Header) -> Result<Header, String> {
    let too_large = || "NetCDF: One or more variable sizes violate format constraints".to_owned();
    let mut laid = header.clone();
    let slots: Vec<usize> = laid
        .variables
        .iter()
        .map(|variable| slot(header, variable))
        .collect();
    for (variable, &size) in laid.variables.iter_mut().zip(&slots) {
        variable.size = (size as u64)
            .checked_next_multiple_of(4)
            .ok_or_else(too_large)?;
    }
    let mut at = encode_header(&laid).len() as u64;
    // The fixed variables first, then the record variables, each where the last one ends.
    for records in [false, true] {
        for (variable, &size) in laid.variables.iter_mut().zip(&slots) {
            let is_record = variable
                .dimensions
                .first()
                .is_some_and(|&dimension| header.dimensions[dimension].1 == 0);
            if is_record == records {
                variable.begin = at;
                at = at.checked_add(size as u64).ok_or_else(too_large)?;
            }
        }
    }
    if !laid.wide
        && laid
            .variables
            .iter()
            .any(|variable| variable.begin > u64::from(u32::MAX))
    {
        return Err(
            "the file is too large for the classic format; write it with \
                    format=\"NETCDF3_64BIT_OFFSET\""
                .to_owned(),
        );
    }
    Ok(laid)
}

/// Makes what `contents` writes the file at `path` in one step: it is written to a new file beside
/// it, flushed to the disk, then renamed over it, so a program that stops partway, a crash or a
/// full disk leave the old file or the new one, never one cut short. A definition rewrites the
/// whole file, so a file cut short would lose everything written before it, not just the
/// change.
///
/// The new file is `.<name>.baryon-<process>` in the same directory, since a rename is atomic
/// only within one file system, and it takes the old file's permissions. It is removed when
/// anything fails. A symbolic link at `path` is replaced by the file, not followed.
///
/// # Errors
///
/// The first error the operating system gives, or `contents` gives: making, writing or flushing
/// the new file, or renaming it.
pub(super) fn replace(
    path: &str,
    contents: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> io::Result<()> {
    let target = Path::new(path);
    let name = target
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the path names no file"))?;
    let mut temporary = OsString::from(".");
    temporary.push(name);
    temporary.push(format!(".baryon-{}", std::process::id()));
    let temporary = target.with_file_name(temporary);
    let written = (|| {
        let mut file = BufWriter::with_capacity(BUFFER, fs::File::create(&temporary)?);
        contents(&mut file)?;
        let file = file.into_inner().map_err(io::IntoInnerError::into_error)?;
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
        replace(path.to_str().unwrap(), |file| file.write_all(b"new")).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        replace(directory.join("fresh.nc").to_str().unwrap(), |file| {
            file.write_all(b"made")
        })
        .unwrap();
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
        assert!(replace(target.to_str().unwrap(), |file| file.write_all(b"new")).is_err());
        assert_eq!(names(&directory), ["data.nc"]);
        assert_eq!(fs::read(target.join("inside")).unwrap(), b"kept");
        let missing = directory.join("missing").join("data.nc");
        assert!(replace(missing.to_str().unwrap(), |file| file.write_all(b"new")).is_err());
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
        replace(path.to_str().unwrap(), |file| file.write_all(b"new")).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o640);
        fs::remove_dir_all(&directory).unwrap();
    }
}
