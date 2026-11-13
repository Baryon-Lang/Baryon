//! Writing one module of a program split into several (roadmap 2.6 and 2.2.2): the module holds
//! some of the program's functions, and only the constants, declarations and metadata nodes
//! those functions point to, numbered afresh in the order they are first pointed to.
//!
//! The renumbering is what lets `baryon watch` skip `clang` for a module an edit did not
//! reach. The backend numbers constants (`@baryon.str.N`) and metadata nodes (`!N`) across the
//! whole program in the order it makes them, so a new message in one function would shift the
//! number of every constant made after it, and change the text of every module that points to
//! one. Numbered within the module, a module's text depends only on its own functions: the
//! same functions give the same text, byte for byte, whatever the rest of the program does.
//!
//! The numbers are found by reading the functions' text. A constant's name is
//! `@baryon.<kind>.N`, where the kind is `str`, `loc`, `spec`, `dict` or `dict.parts`; a dict
//! layout's two names share their number. A metadata node is `!N`. Nothing else in the text
//! the backend writes looks like either outside quotes, and the reader skips everything
//! between quotes, where a message or a file name could hold any characters: inside quotes,
//! the backend writes a quote itself as `\22`.

use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Every constant and metadata node of a program, apart, for [`module`] to choose from.
pub(crate) struct Pieces {
    /// Each constant's definition, one or two lines without the final newline; its number is
    /// its position.
    pub(crate) constants: Vec<String>,
    /// The compile unit and module flags, `!0` to `!2`, which every module carries when the
    /// program has debug information; empty otherwise.
    pub(crate) header: String,
    /// Each numbered metadata node by its number: the subprograms and the loop nodes.
    pub(crate) nodes: BTreeMap<usize, String>,
    /// The metadata numbers kept as they are, those of the header: 3 with debug information,
    /// 0 without.
    pub(crate) reserved: usize,
}

/// The prefixes of the constants' names, longest first so `dict.parts` is not read as `dict`.
const CONSTANT_PREFIXES: [&str; 5] = [
    "@baryon.dict.parts.",
    "@baryon.dict.",
    "@baryon.str.",
    "@baryon.loc.",
    "@baryon.spec.",
];

/// The text of the module holding the functions `texts`, in order, then the constants they
/// point to, then `declarations`, the declarations of what they call that the module does not
/// define, then the metadata nodes they point to. The layout is the one
/// [`Module::finish`](crate::Module::finish) writes for a whole program.
///
/// Cost: one pass over the functions' text, and over each constant and node the module holds.
pub(crate) fn module<'t>(
    texts: impl Iterator<Item = &'t str>,
    declarations: impl Iterator<Item = &'t str>,
    pieces: &Pieces,
) -> String {
    let mut renumber = Renumber {
        constants: Numbering::new(0),
        nodes: Numbering::new(pieces.reserved),
    };
    let mut text = String::new();
    for (index, function) in texts.enumerate() {
        if index > 0 {
            text.push('\n');
        }
        renumber.rewrite(function, &mut text);
    }
    let mut constants = String::new();
    let mut written = 0;
    // Writing a constant can point to one not yet written, as a location points to its file
    // name, which then takes the next number and is written in its turn.
    while let Some(&old) = renumber.constants.order.get(written) {
        if let Some(definition) = pieces.constants.get(old) {
            renumber.rewrite(definition, &mut constants);
            constants.push('\n');
        }
        written += 1;
    }
    if !constants.is_empty() {
        text.push('\n');
        text.push_str(&constants);
    }
    let mut declared = declarations.peekable();
    if declared.peek().is_some() {
        text.push('\n');
        for declaration in declared {
            text.push_str(declaration);
            text.push('\n');
        }
    }
    let mut metadata = pieces.header.clone();
    let mut written = 0;
    while let Some(&old) = renumber.nodes.order.get(written) {
        if let Some(node) = pieces.nodes.get(&old) {
            renumber.rewrite(node, &mut metadata);
            metadata.push('\n');
        }
        written += 1;
    }
    if !metadata.is_empty() {
        text.push('\n');
        text.push_str(&metadata);
    }
    text
}

/// The symbols of the functions `definitions` call that none of them defines, in symbol
/// order: what a module holding them declares.
pub(crate) fn declared<'d>(
    definitions: impl Iterator<Item = (&'d str, &'d BTreeSet<String>)> + Clone,
) -> BTreeSet<&'d str> {
    let defined: BTreeSet<&str> = definitions.clone().map(|(symbol, _)| symbol).collect();
    definitions
        .flat_map(|(_, calls)| calls.iter().map(String::as_str))
        .filter(|symbol| !defined.contains(symbol))
        .collect()
}

/// New numbers for one kind of name, given in the order the old ones are first met.
struct Numbering {
    /// The numbers below this keep their own.
    reserved: usize,
    /// The new number of each old one met so far.
    new: HashMap<usize, usize>,
    /// The old numbers in the order they were met; one's new number is its position plus
    /// `reserved`.
    order: Vec<usize>,
}

impl Numbering {
    /// No numbers met yet; those below `reserved` are kept.
    fn new(reserved: usize) -> Self {
        Self {
            reserved,
            new: HashMap::new(),
            order: Vec::new(),
        }
    }

    /// The new number of `old`, given now if it is met for the first time.
    fn number(&mut self, old: usize) -> usize {
        if old < self.reserved {
            return old;
        }
        let next = self.reserved + self.order.len();
        *self.new.entry(old).or_insert_with(|| {
            self.order.push(old);
            next
        })
    }
}

/// The two numberings of one module.
struct Renumber {
    /// The constants', `@baryon.str.N` and the rest.
    constants: Numbering,
    /// The metadata nodes', `!N`.
    nodes: Numbering,
}

impl Renumber {
    /// Appends `text` to `out` with each constant's and node's number replaced by its new one.
    fn rewrite(&mut self, text: &str, out: &mut String) {
        let bytes = text.as_bytes();
        let mut copied = 0;
        let mut index = 0;
        while let Some(offset) = bytes[index..]
            .iter()
            .position(|&byte| matches!(byte, b'"' | b'@' | b'!'))
        {
            let at = index + offset;
            index = at + 1;
            let (start, nodes) = match bytes[at] {
                b'"' => {
                    // Skip to the closing quote; a quote inside is written `\22`.
                    index = bytes[index..]
                        .iter()
                        .position(|&byte| byte == b'"')
                        .map_or(bytes.len(), |end| index + end + 1);
                    continue;
                }
                b'!' => (at + 1, true),
                _ => match CONSTANT_PREFIXES
                    .iter()
                    .find(|prefix| text[at..].starts_with(*prefix))
                {
                    Some(prefix) => (at + prefix.len(), false),
                    None => continue,
                },
            };
            let digits = bytes[start..]
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            let Ok(old) = text[start..start + digits].parse::<usize>() else {
                continue;
            };
            let numbering = if nodes {
                &mut self.nodes
            } else {
                &mut self.constants
            };
            out.push_str(&text[copied..start]);
            out.push_str(&numbering.number(old).to_string());
            copied = start + digits;
            index = copied;
        }
        out.push_str(&text[copied..]);
    }
}

#[cfg(test)]
mod tests {
    use super::{Pieces, module};
    use std::collections::BTreeMap;

    /// Two constants, a location pointing to a file name, and a debug header with one
    /// subprogram and one loop node.
    fn pieces() -> Pieces {
        Pieces {
            constants: vec![
                "@baryon.str.0 = c\"heat.byn\"".to_owned(),
                "@baryon.loc.1 = { ptr @baryon.str.0 }".to_owned(),
                "@baryon.str.2 = c\"see @baryon.str.0 !7\"".to_owned(),
            ],
            header: "!0 = unit\n".to_owned(),
            nodes: BTreeMap::from([
                (5, "!5 = distinct !DISubprogram(unit: !0)".to_owned()),
                (6, "!6 = distinct !{!6}".to_owned()),
            ]),
            reserved: 3,
        }
    }

    #[test]
    fn a_module_numbers_what_it_points_to_from_zero() {
        let text = module(
            ["f !dbg !5 @baryon.str.2 @baryon.loc.1 !llvm.loop !6"].into_iter(),
            ["declare void @g()"].into_iter(),
            &pieces(),
        );
        assert_eq!(
            text,
            "f !dbg !3 @baryon.str.0 @baryon.loc.1 !llvm.loop !4\n\
             @baryon.str.0 = c\"see @baryon.str.0 !7\"\n\
             @baryon.loc.1 = { ptr @baryon.str.2 }\n\
             @baryon.str.2 = c\"heat.byn\"\n\
             \n\
             declare void @g()\n\
             \n\
             !0 = unit\n\
             !3 = distinct !DISubprogram(unit: !0)\n\
             !4 = distinct !{!4}\n"
        );
    }

    #[test]
    fn a_module_leaves_out_what_it_does_not_point_to() {
        let text = module(["f @baryon.str.0"].into_iter(), [].into_iter(), &pieces());
        assert_eq!(
            text,
            "f @baryon.str.0\n@baryon.str.0 = c\"heat.byn\"\n\n!0 = unit\n"
        );
    }

    #[test]
    fn a_dict_layout_keeps_one_number_for_its_two_names() {
        let pieces = Pieces {
            constants: vec![
                "@baryon.str.0 = c\"x\"".to_owned(),
                "@baryon.dict.parts.1 = []\n@baryon.dict.1 = { ptr @baryon.dict.parts.1 }"
                    .to_owned(),
            ],
            header: String::new(),
            nodes: BTreeMap::new(),
            reserved: 0,
        };
        let text = module(["f @baryon.dict.1"].into_iter(), [].into_iter(), &pieces);
        assert_eq!(
            text,
            "f @baryon.dict.0\n@baryon.dict.parts.0 = []\n@baryon.dict.0 = { ptr @baryon.dict.parts.0 }\n"
        );
    }
}
