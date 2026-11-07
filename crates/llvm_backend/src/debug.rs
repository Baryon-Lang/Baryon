//! Debug information: line tables that map each instruction back to the line and column of the
//! source it came from, so a debugger can step through `.byn` files, a profiler can name the
//! hot line, and a backtrace can say where each frame was.
//!
//! The module carries DWARF line tables only (LLVM's `LineTablesOnly`): no variables or types
//! yet. A function gets a `DISubprogram`, and each instruction written from a statement or
//! terminator a `!dbg` location at that statement's span. Locations, files and the subroutine
//! type are written inline at their use, since LLVM unifies equal ones; only the compile unit
//! and each subprogram, which must be distinct, are numbered. Debug information is off unless
//! the driver asks for it with [`Module::debug_info`](crate::Module::debug_info).
//!
//! The compile unit's language is `DW_LANG_Python`, the closest DWARF has, so debuggers show
//! source lines without guessing a language from the file name.

use source::{FileId, Sources, Span};
use std::fmt::Write as _;

/// The number of the first subprogram's node: `!0` is the compile unit and `!1` and `!2` the
/// module flags.
const FIRST_SUBPROGRAM: usize = 3;

/// The debug information of one module: where the sources live, and each function's
/// subprogram so far.
pub(crate) struct Debug {
    /// The absolute directory the source files' names are relative to: the package's root.
    directory: String,
    /// Each subprogram node's definition, in order; the node's number is its position plus
    /// [`FIRST_SUBPROGRAM`].
    subprograms: Vec<String>,
    /// The file of the first function, which the compile unit names as its own.
    first_file: Option<FileId>,
}

impl Debug {
    /// No subprograms yet; file names are relative to `directory`.
    pub(crate) fn new(directory: &str) -> Self {
        Self {
            directory: directory.to_owned(),
            subprograms: Vec::new(),
            first_file: None,
        }
    }

    /// How many subprograms there are, to [`roll_back`](Self::roll_back) to.
    pub(crate) fn mark(&self) -> usize {
        self.subprograms.len()
    }

    /// Forgets every subprogram made after `mark`, when the function that made it failed.
    pub(crate) fn roll_back(&mut self, mark: usize) {
        self.subprograms.truncate(mark);
        if self.subprograms.is_empty() {
            self.first_file = None;
        }
    }

    /// A new subprogram for the function `name`, whose definition starts at `span`, as `!N`.
    pub(crate) fn subprogram(&mut self, sources: &dyn Sources, name: &str, span: Span) -> String {
        let number = FIRST_SUBPROGRAM + self.subprograms.len();
        let line = sources.file(span.file()).position(span.start()).line;
        let file = self.file(sources, span.file());
        self.first_file.get_or_insert(span.file());
        self.subprograms.push(format!(
            "!{number} = distinct !DISubprogram(name: \"{}\", scope: {file}, file: {file}, \
             line: {line}, type: !DISubroutineType(types: !{{}}), scopeLine: {line}, \
             spFlags: DISPFlagDefinition | DISPFlagOptimized, unit: !0)",
            escape(name)
        ));
        format!("!{number}")
    }

    /// A new subprogram for the C `main` the module's entry point is, as `!N`. It has no source
    /// of its own, so it is artificial and sits at line 0 of the first function's file. Without
    /// it, the package's `main` inlined into it would lose its line table.
    pub(crate) fn entry(&mut self, sources: &dyn Sources) -> String {
        let number = FIRST_SUBPROGRAM + self.subprograms.len();
        let file = self.unit_file(sources);
        self.subprograms.push(format!(
            "!{number} = distinct !DISubprogram(name: \"main\", scope: {file}, file: {file}, \
             line: 0, type: !DISubroutineType(types: !{{}}), scopeLine: 0, \
             flags: DIFlagArtificial, spFlags: DISPFlagDefinition | DISPFlagOptimized, unit: !0)"
        ));
        format!("!{number}")
    }

    /// The location of `span` in the subprogram `scope`, to follow an instruction.
    pub(crate) fn location(sources: &dyn Sources, scope: &str, span: Span) -> String {
        let position = sources.file(span.file()).position(span.start());
        format!(
            "!DILocation(line: {}, column: {}, scope: {scope})",
            position.line, position.column
        )
    }

    /// The compile unit's file: the first function's, or an unnamed one in the directory when
    /// there is no function.
    fn unit_file(&self, sources: &dyn Sources) -> String {
        match self.first_file {
            Some(file) => self.file(sources, file),
            None => format!(
                "!DIFile(filename: \"\", directory: \"{}\")",
                escape(&self.directory)
            ),
        }
    }

    /// The `DIFile` of `file`, written inline.
    fn file(&self, sources: &dyn Sources, file: FileId) -> String {
        format!(
            "!DIFile(filename: \"{}\", directory: \"{}\")",
            escape(sources.file(file).name()),
            escape(&self.directory)
        )
    }

    /// The compile unit, the module flags and every subprogram, one definition per line.
    pub(crate) fn finish(&self, sources: &dyn Sources) -> String {
        let file = self.unit_file(sources);
        let mut text = String::from("!llvm.dbg.cu = !{!0}\n!llvm.module.flags = !{!1, !2}\n");
        let _ = writeln!(
            text,
            "!0 = distinct !DICompileUnit(language: DW_LANG_Python, file: {file}, \
             producer: \"baryon\", isOptimized: true, runtimeVersion: 0, \
             emissionKind: LineTablesOnly)"
        );
        text.push_str("!1 = !{i32 7, !\"Dwarf Version\", i32 4}\n");
        text.push_str("!2 = !{i32 2, !\"Debug Info Version\", i32 3}\n");
        for subprogram in &self.subprograms {
            text.push_str(subprogram);
            text.push('\n');
        }
        text
    }
}

/// `text` as the inside of an LLVM metadata string: a quote, a backslash and anything outside
/// printable ASCII as `\XX`.
fn escape(text: &str) -> String {
    let mut escaped = String::new();
    for byte in text.bytes() {
        if byte == b'"' || byte == b'\\' || !(0x20..0x7f).contains(&byte) {
            let _ = write!(escaped, "\\{byte:02X}");
        } else {
            escaped.push(char::from(byte));
        }
    }
    escaped
}
