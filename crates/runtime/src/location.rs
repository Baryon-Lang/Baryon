//! Where in the source a panic happened, in a form compiled code can write as a constant.

use std::fmt;

/// A source location: file, line and column, as a panic message reports it.
///
/// Compiled code writes one constant `Location` per check that can fail and passes a pointer to
/// it, so the success path carries no location work at all. The layout is C's (`#[repr(C)]`), so
/// the backend can emit it as an ordinary constant: a pointer to the file name's bytes, their
/// length, the line and the column.
///
/// # Invariant
///
/// `file` and `file_length` name `file_length` readable bytes that live as long as the program.
/// [`Location::new`] guarantees it for locations built in Rust; compiled code guarantees it by
/// emitting the file name as a constant, which every runtime function that takes a
/// `*const Location` requires in its `# Safety` section.
///
/// # Example
///
/// ```
/// use runtime::Location;
///
/// static HERE: Location = Location::new("md.npl", 58, 13);
/// assert_eq!(HERE.to_string(), "md.npl:58:13");
/// ```
#[repr(C)]
#[derive(Debug)]
pub struct Location {
    /// The first byte of the file name, which is UTF-8 when the compiler wrote it.
    file: *const u8,
    /// How many bytes the file name has.
    file_length: usize,
    /// The line, counted from 1.
    line: u32,
    /// The column, counted from 1 in characters, as diagnostics count it.
    column: u32,
}

#[allow(
    unsafe_code,
    reason = "the raw file pointer makes the type `!Sync` by default"
)]
// SAFETY: a `Location` is read-only after it is built and its file name lives as long as the
// program (the type's invariant), so sharing one between threads cannot race.
unsafe impl Sync for Location {}

impl Location {
    /// A location in `file`, at `line` and `column`, both counted from 1.
    #[must_use]
    pub const fn new(file: &'static str, line: u32, column: u32) -> Self {
        Self {
            file: file.as_ptr(),
            file_length: file.len(),
            line,
            column,
        }
    }

    /// The file name's bytes.
    fn file_bytes(&self) -> &[u8] {
        // SAFETY: the type's invariant: `file` names `file_length` readable bytes that live as
        // long as the program, so as long as `self`.
        #[allow(unsafe_code, reason = "reads the file name compiled code points to")]
        unsafe {
            std::slice::from_raw_parts(self.file, self.file_length)
        }
    }
}

impl fmt::Display for Location {
    /// Writes `file:line:column`, the form editors and terminals turn into a link. A file name
    /// that is not UTF-8, which only a compiler bug could produce, is written with replacement
    /// characters rather than not at all.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let file = String::from_utf8_lossy(self.file_bytes());
        write!(formatter, "{file}:{}:{}", self.line, self.column)
    }
}
