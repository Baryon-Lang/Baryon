//! The module's shared constants: one per distinct message, file name, source location, format
//! specification and `dict` layout, written once however many places use it. Failed checks
//! (see the `panic` module), formatting and `dict`s refer to them by name. The same value also
//! holds the module's debug information and loop metadata, which share its numbering.

use crate::debug::Debug;
use crate::text::FORMAT_SPEC;
use source::{FileId, Sources, Span};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use types::{Align, FormatSpec, Sign};

/// What a constant holds, which is also how a repeated one is found again.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    /// The bytes of a message or file name.
    Bytes(Vec<u8>),
    /// A format specification, by its text.
    Spec(String),
    /// A `dict` layout: the entry size, the key size, then each key part's offset and kind.
    Dict(Vec<u64>),
    /// The location of the byte at `offset` in `file`.
    Location {
        /// The file.
        file: FileId,
        /// The byte offset in it.
        offset: u32,
    },
}

/// The module's message, file name and location constants, written once each.
pub(crate) struct Constants<'a> {
    /// The source files, for file names, positions and the text of an `assert`.
    sources: &'a dyn Sources,
    /// Each constant's definition, in the order they were made; the constant's number is its
    /// position here.
    definitions: Vec<(Key, String)>,
    /// The number of each constant, by what it holds.
    numbers: BTreeMap<Key, usize>,
    /// The module's debug information, when the driver asked for it.
    pub(crate) debug: Option<Debug>,
    /// Whether integer overflow wraps, as in a release build, instead of panicking: overflow
    /// [`Assert`](mir::TerminatorKind::Assert)s are not written, and `prange` reductions
    /// combine with wrapping arithmetic (decision 0030).
    pub(crate) wraps: bool,
    /// The loop metadata node of each `simd` loop, in the order they were made. They share
    /// the numbering of the debug information's nodes, after which they are written.
    loops: Vec<String>,
}

/// How many constants and subprograms there were, to roll back to.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Mark {
    /// The number of constants.
    constants: usize,
    /// The number of subprograms.
    subprograms: usize,
    /// The number of loop metadata nodes.
    loops: usize,
}

impl<'a> Constants<'a> {
    /// No constants yet; file names, positions and source text are read from `sources`.
    pub(crate) fn new(sources: &'a dyn Sources) -> Self {
        Self {
            sources,
            definitions: Vec::new(),
            numbers: BTreeMap::new(),
            debug: None,
            wraps: false,
            loops: Vec::new(),
        }
    }

    /// How many constants and subprograms there are, to [`roll_back`](Self::roll_back) to.
    pub(crate) fn mark(&self) -> Mark {
        Mark {
            constants: self.definitions.len(),
            subprograms: self.debug.as_ref().map_or(0, Debug::mark),
            loops: self.loops.len(),
        }
    }

    /// Forgets every constant and subprogram made after `mark`, when the function that made
    /// them failed.
    pub(crate) fn roll_back(&mut self, mark: Mark) {
        for (key, _) in self.definitions.drain(mark.constants..) {
            self.numbers.remove(&key);
        }
        if let Some(debug) = &mut self.debug {
            debug.roll_back(mark.subprograms);
        }
        self.loops.truncate(mark.loops);
    }

    /// A subprogram for the function `name` starting at `span`, as `!N`, or `None` without
    /// debug information.
    pub(crate) fn subprogram(&mut self, name: &str, span: Span) -> Option<String> {
        let sources = self.sources;
        let loops = self.loops.len();
        self.debug
            .as_mut()
            .map(|debug| debug.subprogram(sources, name, span, loops))
    }

    /// The subprogram of the C `main`, as `!N`, or `None` without debug information.
    pub(crate) fn entry_subprogram(&mut self) -> Option<String> {
        let sources = self.sources;
        let loops = self.loops.len();
        self.debug.as_mut().map(|debug| debug.entry(sources, loops))
    }

    /// A new loop metadata node asking LLVM to vectorize the loop, as `!N`, for the back edge
    /// of a `simd` loop (decision 0011). LLVM wants each loop's node distinct and naming itself
    /// first.
    pub(crate) fn vectorize(&mut self) -> String {
        let number = self.debug.as_ref().map_or(0, Debug::next_number) + self.loops.len();
        self.loops.push(format!(
            "!{number} = distinct !{{!{number}, !{{!\"llvm.loop.vectorize.enable\", i1 true}}}}"
        ));
        format!("!{number}")
    }

    /// The `!dbg` location of `span` in the subprogram `scope`.
    pub(crate) fn debug_location(&self, scope: &str, span: Span) -> String {
        Debug::location(self.sources, scope, span)
    }

    /// Every constant's definition, one per line, in the order they were made.
    pub(crate) fn finish(self) -> String {
        self.definitions
            .into_iter()
            .map(|(_, definition)| definition + "\n")
            .collect()
    }

    /// The debug information's metadata, when it is on, then each loop metadata node.
    pub(crate) fn metadata(&self) -> String {
        let mut text = self
            .debug
            .as_ref()
            .map(|debug| debug.finish(self.sources))
            .unwrap_or_default();
        for node in &self.loops {
            text.push_str(node);
            text.push('\n');
        }
        text
    }

    /// The constant holding the parts of `spec` as the runtime's `FormatSpec` lays them out, as
    /// `@baryon.spec.N`, made the first time it is asked for: the fill's code point, the
    /// alignment, sign, `#`, `0`, grouping and type as bytes, 0 where left out, then the width
    /// and precision, -1 where left out.
    pub(crate) fn format_spec(&mut self, spec: &FormatSpec) -> String {
        let key = Key::Spec(spec.to_string());
        if let Some(&number) = self.numbers.get(&key) {
            return format!("@baryon.spec.{number}");
        }
        let name = format!("@baryon.spec.{}", self.definitions.len());
        let align = match spec.align {
            None => 0,
            Some(Align::Left) => b'<',
            Some(Align::Right) => b'>',
            Some(Align::Center) => b'^',
            Some(Align::AfterSign) => b'=',
        };
        let sign = match spec.sign {
            Sign::Negative => b'-',
            Sign::Always => b'+',
            Sign::Space => b' ',
        };
        let byte = |character: Option<char>| character.map_or(0, |character| character as u32);
        let number = |value: Option<u32>| value.map_or(-1, i64::from);
        let definition = format!(
            "{name} = private unnamed_addr constant {FORMAT_SPEC} {{ i32 {}, i8 {align}, i8 {sign}, \
             i8 {}, i8 {}, i8 {}, i8 {}, i64 {}, i64 {} }}",
            u32::from(spec.fill),
            u8::from(spec.alternate),
            u8::from(spec.zero),
            byte(spec.grouping),
            byte(spec.presentation.letter()),
            number(spec.width),
            number(spec.precision),
        );
        self.push(key, definition);
        name
    }

    /// The layout constant of a `dict` type, as `@baryon.dict.N`, made the first time it is
    /// asked for: the runtime's `DictLayout`, `{ i64 entry_size, i64 key_size, i64 part_count,
    /// ptr parts }`, pointing to an array of `{ i64 offset, i64 kind }` key parts, kind 0 for a
    /// `str` and the size in bytes for an integer.
    pub(crate) fn dict_layout(
        &mut self,
        entry_size: u64,
        key_size: u64,
        parts: &[(u64, u64)],
    ) -> String {
        let mut words = vec![entry_size, key_size];
        words.extend(parts.iter().flat_map(|&(offset, kind)| [offset, kind]));
        let key = Key::Dict(words);
        if let Some(&number) = self.numbers.get(&key) {
            return format!("@baryon.dict.{number}");
        }
        let number = self.definitions.len();
        let name = format!("@baryon.dict.{number}");
        let items: Vec<String> = parts
            .iter()
            .map(|(offset, kind)| format!("{{ i64, i64 }} {{ i64 {offset}, i64 {kind} }}"))
            .collect();
        let count = parts.len();
        let definition = format!(
            "@baryon.dict.parts.{number} = private unnamed_addr constant [{count} x {{ i64, i64 }}]              [{}]\n{name} = private unnamed_addr constant {{ i64, i64, i64, ptr }}              {{ i64 {entry_size}, i64 {key_size}, i64 {count}, ptr @baryon.dict.parts.{number} }}",
            items.join(", ")
        );
        self.push(key, definition);
        name
    }

    /// The constant holding `bytes`, as `@baryon.str.N`, made the first time it is asked for.
    pub(crate) fn bytes(&mut self, bytes: &[u8]) -> String {
        let key = Key::Bytes(bytes.to_vec());
        if let Some(&number) = self.numbers.get(&key) {
            return format!("@baryon.str.{number}");
        }
        let name = format!("@baryon.str.{}", self.definitions.len());
        let mut escaped = String::new();
        for &byte in bytes {
            if byte == b'"' || byte == b'\\' || !(0x20..0x7f).contains(&byte) {
                let _ = write!(escaped, "\\{byte:02X}");
            } else {
                escaped.push(char::from(byte));
            }
        }
        let definition = format!(
            "{name} = private unnamed_addr constant [{} x i8] c\"{escaped}\"",
            bytes.len()
        );
        self.push(key, definition);
        name
    }

    /// The `Location` constant of where `span` starts, as `@baryon.loc.N`.
    pub(crate) fn location(&mut self, span: Span) -> String {
        let key = Key::Location {
            file: span.file(),
            offset: span.start(),
        };
        if let Some(&number) = self.numbers.get(&key) {
            return format!("@baryon.loc.{number}");
        }
        let file = self.sources.file(span.file());
        let position = file.position(span.start());
        let name_bytes = file.name().as_bytes().to_vec();
        let file_name = self.bytes(&name_bytes);
        let name = format!("@baryon.loc.{}", self.definitions.len());
        let definition = format!(
            "{name} = private unnamed_addr constant {{ ptr, i64, i32, i32 }} \
             {{ ptr {file_name}, i64 {}, i32 {}, i32 {} }}",
            name_bytes.len(),
            position.line,
            position.column
        );
        self.push(key, definition);
        name
    }

    /// The source text of `span` with its whitespace runs made single spaces, so a statement
    /// written over several lines reads as one.
    pub(crate) fn source(&self, span: Span) -> String {
        let text = self.sources.file(span.file()).text();
        text.get(span.start() as usize..span.end() as usize)
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Records a new constant.
    fn push(&mut self, key: Key, definition: String) {
        self.numbers.insert(key.clone(), self.definitions.len());
        self.definitions.push((key, definition));
    }
}
