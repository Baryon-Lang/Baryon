//! Writing a module from definitions written apart (roadmap 2.2.2): the module holds some of
//! the program's functions, or all of them, and only the constants, declarations and metadata
//! nodes those functions made.
//!
//! Each definition numbers its own constants (`@baryon.str.N`) and metadata nodes (`!N`) from
//! the start, so its text never depends on another definition's. This module numbers them
//! again for the module that holds them, in the order the definitions made them, and writes a
//! constant two definitions both made once. That is what lets `baryon watch` skip `clang` for
//! a module an edit did not reach: the same functions give the same module text, byte for
//! byte, whatever the rest of the program does. It also means a definition's text can be kept
//! and reused, or written on another thread, without asking the others for numbers.
//!
//! The numbers are found by reading the functions' text. A constant's name is
//! `@baryon.<kind>.N`, where the kind is `str`, `loc`, `spec`, `dict` or `dict.parts`; a dict
//! layout's two names share their number. A metadata node is `!N`. Nothing else in the text
//! the backend writes looks like either outside quotes, and the reader skips everything
//! between quotes, where a message or a file name could hold any characters: inside quotes,
//! the backend writes a quote itself as `\22`.

use std::collections::{BTreeSet, HashMap};

/// The constants and metadata nodes one definition made, numbered within it: what
/// [`module`] numbers again for the module that holds the definition.
pub(crate) struct Pieces {
    /// Each constant's definition, one or two lines without the final newline; its number is
    /// its position.
    pub(crate) constants: Vec<String>,
    /// Each subprogram node with its number.
    pub(crate) subprograms: Vec<(usize, String)>,
    /// Each loop metadata node with its number. The subprograms and loop nodes together take
    /// the numbers from the reserved ones up, in the order they were made.
    pub(crate) loops: Vec<(usize, String)>,
}

/// The prefixes of the constants' names, longest first so `dict.parts` is not read as `dict`.
const CONSTANT_PREFIXES: [&str; 5] = [
    "@baryon.dict.parts.",
    "@baryon.dict.",
    "@baryon.str.",
    "@baryon.loc.",
    "@baryon.spec.",
];

/// A number no constant has, which a constant's own name takes while it is compared with the
/// constants already written.
const ITSELF: usize = usize::MAX;

/// The text of the module holding `definitions`, each a function's text and the pieces it
/// made, in order: the functions, then their constants, then `declarations`, the declarations
/// of what they call that the module does not define, then `header`, the compile unit and
/// module flags, which take the metadata numbers below `reserved`, then the subprograms and
/// the loop nodes.
///
/// Constants and nodes are numbered in the order the definitions made them, and a constant two
/// definitions both made is written once, under the first one's number. For a whole program
/// that is the numbering one shared set of constants would give.
///
/// Cost: one pass over the functions' text, and over each constant and node.
pub(crate) fn module<'t>(
    definitions: impl Iterator<Item = (&'t str, &'t Pieces)>,
    declarations: impl Iterator<Item = &'t str>,
    header: &str,
    reserved: usize,
) -> String {
    let mut text = String::new();
    let mut constants = String::new();
    let mut subprograms = String::new();
    let mut loops = String::new();
    let mut written: HashMap<String, usize> = HashMap::new();
    let mut next_node = reserved;
    for (index, (function, pieces)) in definitions.enumerate() {
        let mut nodes: HashMap<usize, usize> = HashMap::new();
        let mut made: Vec<_> = pieces.subprograms.iter().chain(&pieces.loops).collect();
        made.sort_by_key(|(number, _)| *number);
        for (number, _) in made {
            nodes.insert(*number, next_node);
            next_node += 1;
        }
        let node = |old: usize| nodes.get(&old).copied().unwrap_or(old);
        let mut numbers: Vec<usize> = Vec::with_capacity(pieces.constants.len());
        for (own, constant) in pieces.constants.iter().enumerate() {
            let mut key = String::new();
            let mapped = |old: usize| {
                if old == own {
                    ITSELF
                } else {
                    numbers.get(old).copied().unwrap_or(old)
                }
            };
            rewrite(constant, &mut key, mapped, node);
            if let Some(&number) = written.get(&key) {
                numbers.push(number);
                continue;
            }
            let number = written.len();
            let mut final_text = String::new();
            rewrite(
                &key,
                &mut final_text,
                |old| if old == ITSELF { number } else { old },
                |old| old,
            );
            constants.push_str(&final_text);
            constants.push('\n');
            written.insert(key, number);
            numbers.push(number);
        }
        if index > 0 {
            text.push('\n');
        }
        let constant = |old: usize| numbers.get(old).copied().unwrap_or(old);
        rewrite(function, &mut text, constant, node);
        for (_, subprogram) in &pieces.subprograms {
            rewrite(subprogram, &mut subprograms, |old| old, node);
            subprograms.push('\n');
        }
        for (_, node_text) in &pieces.loops {
            rewrite(node_text, &mut loops, |old| old, node);
            loops.push('\n');
        }
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
    let metadata = [header, &subprograms, &loops].concat();
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

/// Appends `text` to `out` with each constant's number replaced by `constant` of it, and each
/// metadata node's by `node` of it.
fn rewrite(
    text: &str,
    out: &mut String,
    mut constant: impl FnMut(usize) -> usize,
    mut node: impl FnMut(usize) -> usize,
) {
    let bytes = text.as_bytes();
    let mut copied = 0;
    let mut index = 0;
    while let Some(offset) = bytes[index..]
        .iter()
        .position(|&byte| matches!(byte, b'"' | b'@' | b'!'))
    {
        let at = index + offset;
        index = at + 1;
        let (start, is_node) = match bytes[at] {
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
        let new = if is_node { node(old) } else { constant(old) };
        out.push_str(&text[copied..start]);
        out.push_str(&new.to_string());
        copied = start + digits;
        index = copied;
    }
    out.push_str(&text[copied..]);
}

#[cfg(test)]
mod tests {
    use super::{Pieces, module};

    /// A function's pieces: a file name, a location pointing to it, a message holding text
    /// that looks like a name, a subprogram and a loop node.
    fn pieces() -> Pieces {
        Pieces {
            constants: vec![
                "@baryon.str.0 = c\"heat.byn\"".to_owned(),
                "@baryon.loc.1 = { ptr @baryon.str.0 }".to_owned(),
                "@baryon.str.2 = c\"see @baryon.str.0 !7\"".to_owned(),
            ],
            subprograms: vec![(3, "!3 = distinct !DISubprogram(unit: !0)".to_owned())],
            loops: vec![(4, "!4 = distinct !{!4}".to_owned())],
        }
    }

    #[test]
    fn one_definition_keeps_its_numbers() {
        let pieces = pieces();
        let text = module(
            [(
                "f !dbg !3 @baryon.str.2 @baryon.loc.1 !llvm.loop !4",
                &pieces,
            )]
            .into_iter(),
            ["declare void @g()"].into_iter(),
            "!0 = unit\n",
            3,
        );
        assert_eq!(
            text,
            "f !dbg !3 @baryon.str.2 @baryon.loc.1 !llvm.loop !4\n\
             @baryon.str.0 = c\"heat.byn\"\n\
             @baryon.loc.1 = { ptr @baryon.str.0 }\n\
             @baryon.str.2 = c\"see @baryon.str.0 !7\"\n\
             \n\
             declare void @g()\n\
             \n\
             !0 = unit\n\
             !3 = distinct !DISubprogram(unit: !0)\n\
             !4 = distinct !{!4}\n"
        );
    }

    #[test]
    fn a_second_definition_shares_equal_constants_and_numbers_after_the_first() {
        let first = pieces();
        let second = Pieces {
            constants: vec![
                "@baryon.str.0 = c\"heat.byn\"".to_owned(),
                "@baryon.loc.1 = { ptr @baryon.str.0 }".to_owned(),
                "@baryon.str.2 = c\"new\"".to_owned(),
            ],
            subprograms: vec![(3, "!3 = distinct !DISubprogram(unit: !0)".to_owned())],
            loops: Vec::new(),
        };
        let text = module(
            [
                ("f !dbg !3 !llvm.loop !4", &first),
                ("g !dbg !3 @baryon.loc.1 @baryon.str.2", &second),
            ]
            .into_iter(),
            [].into_iter(),
            "!0 = unit\n",
            3,
        );
        assert_eq!(
            text,
            "f !dbg !3 !llvm.loop !4\n\
             g !dbg !5 @baryon.loc.1 @baryon.str.3\n\
             @baryon.str.0 = c\"heat.byn\"\n\
             @baryon.loc.1 = { ptr @baryon.str.0 }\n\
             @baryon.str.2 = c\"see @baryon.str.0 !7\"\n\
             @baryon.str.3 = c\"new\"\n\
             \n\
             !0 = unit\n\
             !3 = distinct !DISubprogram(unit: !0)\n\
             !5 = distinct !DISubprogram(unit: !0)\n\
             !4 = distinct !{!4}\n"
        );
    }

    #[test]
    fn a_dict_layout_keeps_one_number_for_its_two_names() {
        let first = Pieces {
            constants: vec!["@baryon.str.0 = c\"x\"".to_owned()],
            subprograms: Vec::new(),
            loops: Vec::new(),
        };
        let second = Pieces {
            constants: vec![
                "@baryon.dict.parts.0 = []\n@baryon.dict.0 = { ptr @baryon.dict.parts.0 }"
                    .to_owned(),
            ],
            subprograms: Vec::new(),
            loops: Vec::new(),
        };
        let text = module(
            [("f @baryon.str.0", &first), ("g @baryon.dict.0", &second)].into_iter(),
            [].into_iter(),
            "",
            0,
        );
        assert_eq!(
            text,
            "f @baryon.str.0\ng @baryon.dict.1\n@baryon.str.0 = c\"x\"\n\
             @baryon.dict.parts.1 = []\n@baryon.dict.1 = { ptr @baryon.dict.parts.1 }\n"
        );
    }
}
