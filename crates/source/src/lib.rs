//! Source text and locations: the foundation every compiler stage shares.
//!
//! A [`SourceMap`] owns every loaded file. Stages refer to a file by its [`FileId`] and to a
//! piece of text by a [`Span`], so they never copy source text or hold references into it.
//! Turning a span back into something a person can read is the [`SourceFile`]'s job.
//!
//! A [`SourceMap`] suits a batch of files loaded once. A collection whose files change, such as
//! the compiler's incremental database, builds each [`SourceFile`] itself, issues its own
//! [`FileId`]s, and implements [`Sources`] so diagnostics can still be rendered from it.
//!
//! ```
//! use source::{Position, SourceMap, Span};
//!
//! let mut sources = SourceMap::new();
//! let file = sources.add("orbit.npl", "mass = 5.97e24\nradius = 6.37e6\n")?;
//!
//! // The span of `radius` on the second line: bytes 15 to 21.
//! let radius = Span::new(file, 15, 21);
//! let text = sources.get(radius.file());
//! assert_eq!(text.position(radius.start()), Position { line: 2, column: 1 });
//! assert_eq!(text.line(2), Some("radius = 6.37e6"));
//! # Ok::<(), source::SourceError>(())
//! ```

use std::error::Error;
use std::fmt;

/// Identifies one file in a [`SourceMap`] or another collection of files.
///
/// An id is meaningful only to the collection that issued it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(
    /// Index of the file in its collection's file list.
    u32,
);

impl FileId {
    /// The id at `index` in a collection's file list. A collection other than [`SourceMap`]
    /// uses this to issue its own ids, numbering its files from 0.
    ///
    /// ```
    /// use source::FileId;
    ///
    /// assert_eq!(FileId::from_index(3).index(), 3);
    /// ```
    #[must_use]
    pub fn from_index(index: u32) -> Self {
        Self(index)
    }

    /// The file's index in the list of the collection that issued the id.
    #[must_use]
    pub fn index(self) -> u32 {
        self.0
    }
}

/// A byte range `start..end` within one source file.
///
/// Offsets are `u32`, which limits a single file to 4 GiB and keeps a span at 12 bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    /// The file the span points into.
    file: FileId,
    /// Byte offset of the first byte in the span.
    start: u32,
    /// Byte offset one past the last byte in the span.
    end: u32,
}

impl Span {
    /// Creates the span `start..end` in `file`.
    ///
    /// # Panics
    ///
    /// Panics if `start > end`, which is always a bug in the caller.
    #[must_use]
    pub fn new(file: FileId, start: u32, end: u32) -> Self {
        assert!(start <= end, "span start {start} is after its end {end}");
        Self { file, start, end }
    }

    /// The file this span points into.
    #[must_use]
    pub fn file(self) -> FileId {
        self.file
    }

    /// Byte offset of the first byte in the span.
    #[must_use]
    pub fn start(self) -> u32 {
        self.start
    }

    /// Byte offset one past the last byte in the span.
    #[must_use]
    pub fn end(self) -> u32 {
        self.end
    }
}

/// A human-readable location: 1-based line and 1-based column.
///
/// Columns count Unicode scalar values, not bytes, so they match what an editor shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    /// Line number, starting at 1.
    pub line: u32,
    /// Column number, starting at 1.
    pub column: u32,
}

/// One loaded source file: its name, its text and an index of where each line starts.
#[derive(Debug)]
pub struct SourceFile {
    /// The name the file was loaded under, usually its path.
    name: String,
    /// The full text of the file.
    text: String,
    /// Byte offset where each line starts, in order. Always begins with 0.
    line_starts: Vec<u32>,
}

impl SourceFile {
    /// Builds a file and the index of where its lines start. Cost: one pass over the text.
    ///
    /// ```
    /// use source::SourceFile;
    ///
    /// let file = SourceFile::new("heat.npl", "alpha = 0.1\nsteps = 100\n")?;
    /// assert_eq!(file.line(2), Some("steps = 100"));
    /// # Ok::<(), source::SourceError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Fails if the text is 4 GiB or larger, because offsets are `u32`.
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Result<Self, SourceError> {
        let (name, text) = (name.into(), text.into());
        if u32::try_from(text.len()).is_err() {
            return Err(SourceError::FileTooLarge { name });
        }
        let line_starts = std::iter::once(0)
            .chain(
                text.match_indices('\n')
                    .map(|(index, _)| offset_of(index + 1)),
            )
            .collect();
        Ok(Self {
            name,
            text,
            line_starts,
        })
    }

    /// The name the file was loaded under, usually its path.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The full text of the file.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Converts a byte offset to a line and column. Offsets past the end clamp to the end.
    #[must_use]
    pub fn position(&self, offset: u32) -> Position {
        let offset = offset.min(offset_of(self.text.len()));
        let line_index = self.line_starts.partition_point(|&start| start <= offset) - 1;
        let line_start = self.line_starts[line_index];
        let column = self.text[line_start as usize..]
            .char_indices()
            .take_while(|&(index, _)| offset_of(index) < offset - line_start)
            .count();
        Position {
            line: offset_of(line_index) + 1,
            column: offset_of(column) + 1,
        }
    }

    /// The text of a 1-based line, without its line ending, or `None` past the last line.
    #[must_use]
    pub fn line(&self, line: u32) -> Option<&str> {
        let index = usize::try_from(line).ok()?.checked_sub(1)?;
        let start = *self.line_starts.get(index)? as usize;
        let end = self
            .line_starts
            .get(index + 1)
            .map_or(self.text.len(), |&next| next as usize);
        let text = &self.text[start..end];
        let text = text.strip_suffix('\n').unwrap_or(text);
        Some(text.strip_suffix('\r').unwrap_or(text))
    }
}

/// Owns every source file in a compilation.
#[derive(Debug, Default)]
pub struct SourceMap {
    /// Every file added so far. A [`FileId`] is an index into this list.
    files: Vec<SourceFile>,
}

impl SourceMap {
    /// Creates an empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a file and returns its id.
    ///
    /// # Errors
    ///
    /// Fails if the text is 4 GiB or larger, or if the map already holds `u32::MAX` files.
    pub fn add(
        &mut self,
        name: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<FileId, SourceError> {
        let id = u32::try_from(self.files.len()).map_err(|_| SourceError::TooManyFiles)?;
        self.files.push(SourceFile::new(name, text)?);
        Ok(FileId(id))
    }

    /// The file with the given id.
    ///
    /// # Panics
    ///
    /// Panics if `id` was issued by a different `SourceMap`.
    #[must_use]
    pub fn get(&self, id: FileId) -> &SourceFile {
        &self.files[id.0 as usize]
    }
}

impl Sources for SourceMap {
    fn file(&self, id: FileId) -> &SourceFile {
        self.get(id)
    }
}

/// Anything that looks up source files by id: a [`SourceMap`], or the compiler's incremental
/// database. Diagnostic rendering reads file names and lines through this trait, so it works
/// with either.
///
/// ```
/// use source::{SourceMap, Sources};
///
/// let mut sources = SourceMap::new();
/// let file = sources.add("main.npl", "x = 1\n")?;
/// assert_eq!(sources.file(file).name(), "main.npl");
/// # Ok::<(), source::SourceError>(())
/// ```
pub trait Sources: fmt::Debug {
    /// The file with the given id.
    ///
    /// # Panics
    ///
    /// May panic if `id` was issued by a different collection.
    fn file(&self, id: FileId) -> &SourceFile;
}

/// Why a source file could not be built or added to a [`SourceMap`].
#[derive(Debug, PartialEq, Eq)]
pub enum SourceError {
    /// The file is 4 GiB or larger.
    FileTooLarge {
        /// The name the file was being loaded under.
        name: String,
    },
    /// The map already holds the maximum number of files.
    TooManyFiles,
}

impl fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileTooLarge { name } => {
                write!(formatter, "source file `{name}` is 4 GiB or larger")
            }
            Self::TooManyFiles => write!(formatter, "too many source files"),
        }
    }
}

impl Error for SourceError {}

/// Converts an in-file index to a `u32` offset. `SourceFile::new` guarantees it fits.
fn offset_of(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single_file(text: &str) -> (SourceMap, FileId) {
        let mut sources = SourceMap::new();
        let id = sources.add("test.npl", text).unwrap();
        (sources, id)
    }

    #[test]
    fn position_counts_lines_and_columns_from_one() {
        let (sources, id) = single_file("def f():\n    return 1\n");
        let file = sources.get(id);
        assert_eq!(file.position(0), Position { line: 1, column: 1 });
        assert_eq!(file.position(13), Position { line: 2, column: 5 });
    }

    #[test]
    fn position_counts_characters_not_bytes() {
        let (sources, id) = single_file("μ = 1");
        assert_eq!(sources.get(id).position(3), Position { line: 1, column: 3 });
    }

    #[test]
    fn position_past_the_end_clamps_to_the_end() {
        let (sources, id) = single_file("x\n");
        assert_eq!(
            sources.get(id).position(99),
            Position { line: 2, column: 1 }
        );
    }

    #[test]
    fn line_strips_unix_and_windows_line_endings() {
        let (sources, id) = single_file("a = 1\r\nb = 2\nc = 3");
        let file = sources.get(id);
        assert_eq!(file.line(1), Some("a = 1"));
        assert_eq!(file.line(2), Some("b = 2"));
        assert_eq!(file.line(3), Some("c = 3"));
        assert_eq!(file.line(0), None);
        assert_eq!(file.line(4), None);
    }

    #[test]
    fn files_get_distinct_ids() {
        let mut sources = SourceMap::new();
        let first = sources.add("a.npl", "a").unwrap();
        let second = sources.add("b.npl", "b").unwrap();
        assert_ne!(first, second);
        assert_eq!(sources.get(second).name(), "b.npl");
    }

    #[test]
    #[should_panic(expected = "span start 2 is after its end 1")]
    fn span_rejects_reversed_ranges() {
        let (_, id) = single_file("abc");
        let _ = Span::new(id, 2, 1);
    }
}
