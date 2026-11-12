//! Scalar locals as registers instead of stack slots (roadmap 2.7, decision 0048).
//!
//! The backend writes every local as a stack slot, loaded and stored at each use, and leaves it
//! to LLVM's `mem2reg` to turn the slots into registers (decision 0018). `clang -O0`, which a
//! debug build uses, runs no `mem2reg`, so every number a loop touches went through memory:
//! the `nbody` kernel ran in 5.3 s at `-O0`, and in 2.3 s with `mem2reg` alone. The extra
//! loads and stores also doubled the text `clang` reads: a 4,706-line program's module halved
//! from 383,000 lines to 186,000, and its machine code at `-O0` took 0.84 s instead of 1.5 s.
//!
//! So a *promoted* local, a number or `bool` that is only ever assigned whole and read whole,
//! never has a slot. Each assignment gives it a new value, and a read takes the value it holds
//! at that point: SSA form, built the way Braun and others describe in "Simple and Efficient
//! Construction of Static Single Assignment Form" (2013), on the body's own blocks:
//!
//! - Inside one block, a read takes the value of the last assignment before it.
//! - A read in a block before any assignment in it takes the value the local held on entry,
//!   which is a `phi` at the top of the block: on each edge into the block, the value the local
//!   held at the end of the edge's source, found the same way, back to the entry, where a
//!   parameter holds its argument.
//! - A `phi` whose operands are all one value, or itself, is that value, and is left out.
//!
//! Every other local, and every use the backend makes through an address, such as passing a
//! local by pointer or writing a field, keeps its slot. Rather than predict every such use, the
//! function is written once with every candidate promoted; a candidate whose address was asked
//! for is then demoted, and the function written again. Results never change: a register and a
//! slot hold the same bits.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

/// What every branch to a body block contains: `label %bb` and the block's number
/// ([`block_label`](super::block_label)).
const BRANCH: &str = "label %bb";

/// The text that marks where a block's `phi` instructions go, written right after its label
/// and replaced once the function is written. Left in, it would be an LLVM comment.
const MARKER: &str = "  ;phis ";

/// One edge into a body block, as written: a branch from the LLVM block `from_label`, part of
/// the body block `from` or the entry block when `None`, with the values each promoted local
/// was given in `from` before the branch.
pub(crate) struct Edge {
    /// The body block branched to.
    target: usize,
    /// The LLVM block the branch is in.
    from_label: String,
    /// The body block that LLVM block belongs to, or `None` for the entry block.
    from: Option<usize>,
    /// The value of each promoted local assigned or read in `from` before the branch.
    values: HashMap<usize, String>,
}

/// The state of building SSA form for one function.
#[derive(Default)]
pub(crate) struct Ssa {
    /// Whether each local is promoted, by local.
    pub(crate) promoted: Vec<bool>,
    /// The promoted locals whose address was asked for, to demote on the next attempt.
    pub(crate) escaped: BTreeSet<usize>,
    /// The LLVM type of each promoted local.
    types: HashMap<usize, String>,
    /// The value of each promoted local parameter on entry: its argument.
    arguments: HashMap<usize, String>,
    /// The body block being written, or `None` for the entry block.
    block: Option<usize>,
    /// The LLVM block being written, which an edge leaves from.
    label: String,
    /// The current value of each promoted local assigned or read in the body block being
    /// written.
    values: HashMap<usize, String>,
    /// Every edge into a body block, as written.
    edges: Vec<Edge>,
    /// The `phi` of each local read on entry to each block: (block, local).
    phis: BTreeSet<(usize, usize)>,
}

impl Ssa {
    /// The SSA state for a body with `locals` locals, of which `promoted` are promoted, each
    /// with its LLVM type, and the promoted parameters with their arguments.
    pub(crate) fn new(
        locals: usize,
        promoted: HashMap<usize, String>,
        arguments: HashMap<usize, String>,
    ) -> Self {
        let mut flags = vec![false; locals];
        for local in promoted.keys() {
            flags[*local] = true;
        }
        Self {
            promoted: flags,
            types: promoted,
            arguments,
            label: "start".to_owned(),
            ..Self::default()
        }
    }

    /// Whether `local` is promoted.
    pub(crate) fn is_promoted(&self, local: usize) -> bool {
        self.promoted.get(local).copied().unwrap_or(false)
    }

    /// Starts the body block `block`, whose LLVM label is `label`, and returns the marker line to
    /// write after the label, where its `phi`s go.
    pub(crate) fn start_block(&mut self, block: usize, label: &str) -> String {
        self.block = Some(block);
        label.clone_into(&mut self.label);
        self.values.clear();
        format!("{MARKER}{block}\n")
    }

    /// Starts an LLVM block inside the body block being written, such as one a statement
    /// branches to; promoted values carry on into it.
    pub(crate) fn start_label(&mut self, label: &str) {
        label.clone_into(&mut self.label);
    }

    /// Records the instruction `instruction` as written in the current LLVM block: each label
    /// of a body block it names is an edge into that block.
    pub(crate) fn record(&mut self, instruction: &str) {
        // Only a `br` or a `switch` names a block; checking the opening word first spares a
        // search of every other instruction.
        if !(instruction.starts_with("br ") || instruction.starts_with("switch ")) {
            return;
        }
        let mut rest = instruction;
        while let Some(position) = rest.find(BRANCH) {
            rest = &rest[position + BRANCH.len()..];
            let digits = rest
                .find(|character: char| !character.is_ascii_digit())
                .unwrap_or(rest.len());
            if let Ok(target) = rest[..digits].parse() {
                self.edges.push(Edge {
                    target,
                    from_label: self.label.clone(),
                    from: self.block,
                    values: self.values.clone(),
                });
            }
            rest = &rest[digits..];
        }
    }

    /// Records the entry block's branch to the first body block.
    pub(crate) fn enter(&mut self) {
        self.record(&format!("br {BRANCH}0"));
    }

    /// Gives the promoted `local` the value `value` from here on.
    pub(crate) fn assign(&mut self, local: usize, value: String) {
        self.values.insert(local, value);
    }

    /// The value the promoted `local` holds here: its last assignment in this block, or the
    /// `phi` that gives its value on entry to the block.
    pub(crate) fn read(&mut self, local: usize) -> String {
        if let Some(value) = self.values.get(&local) {
            return value.clone();
        }
        let value = match self.block {
            Some(block) => {
                self.phis.insert((block, local));
                phi_name(local, block)
            }
            None => self.entry_value(local),
        };
        self.values.insert(local, value.clone());
        value
    }

    /// The value of `local` on entry to the function: its argument, or `poison` for a local
    /// read before any assignment, which a valid body never does.
    fn entry_value(&self, local: usize) -> String {
        self.arguments
            .get(&local)
            .cloned()
            .unwrap_or_else(|| "poison".to_owned())
    }

    /// Replaces each block's marker in `code` with its `phi`s, and every `phi` that turned out
    /// to be one value with that value.
    ///
    /// Each `phi` gets one operand per edge into its block, in the order the edges were
    /// written: the value the local held at the edge's source, which may need a `phi` of its
    /// own in the source block. Then any `phi` whose operands, ignoring itself, are one value is
    /// replaced by it, until none is left, so a local that does not change around a loop needs
    /// no `phi`.
    pub(crate) fn finish(&self, code: &str) -> String {
        let blocks = self
            .edges
            .iter()
            .map(|edge| edge.target + 1)
            .max()
            .unwrap_or(0);
        let mut edges: Vec<Vec<&Edge>> = vec![Vec::new(); blocks];
        for edge in &self.edges {
            edges[edge.target].push(edge);
        }
        let mut resolver = Resolver {
            ssa: self,
            edges,
            walked: vec![0; blocks],
            walk: 0,
            entries: HashMap::new(),
            phis: Vec::new(),
            pending: Vec::new(),
        };
        let reads: Vec<((usize, usize), Value)> = self
            .phis
            .iter()
            .map(|&(block, local)| ((block, local), resolver.entry(block, local)))
            .collect();
        while let Some(number) = resolver.pending.pop() {
            resolver.fill(number);
        }
        let phis = resolver.phis;
        let replaced = trivial(&phis);
        let resolve = |value: &Value| -> String {
            match value {
                Value::Phi(number) => replaced[*number]
                    .clone()
                    .unwrap_or_else(|| phi_name(phis[*number].local, phis[*number].block)),
                Value::Text(text) => text.clone(),
            }
        };
        let names: HashMap<String, String> = reads
            .iter()
            .map(|((block, local), value)| (phi_name(*local, *block), resolve(value)))
            .collect();
        let mut by_block: BTreeMap<usize, String> = BTreeMap::new();
        for (number, phi) in phis.iter().enumerate() {
            if replaced[number].is_some() {
                continue;
            }
            let name = phi_name(phi.local, phi.block);
            let ty = self.types.get(&phi.local).map_or("i64", String::as_str);
            let text = by_block.entry(phi.block).or_default();
            if phi.incoming.is_empty() {
                // A block nothing branches to still needs the name defined.
                let _ = writeln!(text, "  {name} = freeze {ty} poison");
                continue;
            }
            let pairs: Vec<String> = phi
                .incoming
                .iter()
                .map(|(value, label)| format!("[ {}, %{label} ]", resolve(value)))
                .collect();
            let _ = writeln!(text, "  {name} = phi {ty} {}", pairs.join(", "));
        }
        rewrite(code, &by_block, &names)
    }
}

/// Finds the value each promoted local holds on entry to each block, once the function is
/// written and every edge is known.
struct Resolver<'s> {
    /// The function's SSA state.
    ssa: &'s Ssa,
    /// The edges into each body block, in the order they were written, by block; a block past
    /// the end has none.
    edges: Vec<Vec<&'s Edge>>,
    /// The number of the last walk [`entry`](Self::entry) made through each block, by block, so
    /// a walk knows the blocks it has passed without a set of its own.
    walked: Vec<u32>,
    /// The number of the walk being made.
    walk: u32,
    /// The value of each (block, local) on entry, once found.
    entries: HashMap<(usize, usize), Value>,
    /// Every `phi` made so far.
    phis: Vec<Phi<'s>>,
    /// The `phi`s whose operands are still to be found.
    pending: Vec<usize>,
}

impl<'s> Resolver<'s> {
    /// The value `local` holds on entry to `block`.
    ///
    /// A block with one edge into it takes the value at that edge's source, without a `phi`:
    /// the walk follows such blocks back until it finds an assignment, the entry block, or a
    /// block with several edges or none, which gets a `phi`. Every block walked through gets
    /// the value found.
    fn entry(&mut self, block: usize, local: usize) -> Value {
        let mut chain: Vec<usize> = Vec::new();
        self.walk += 1;
        let mut current = block;
        let value = loop {
            if let Some(value) = self.entries.get(&(current, local)) {
                break value.clone();
            }
            let incoming = self.edges.get(current).map_or(&[][..], Vec::as_slice);
            if let [edge] = incoming
                && self.walked[current] != self.walk
            {
                self.walked[current] = self.walk;
                chain.push(current);
                match self.at_source(edge, local) {
                    Ok(value) => break value,
                    Err(Some(from)) => current = from,
                    Err(None) => break Value::Text(self.ssa.entry_value(local)),
                }
            } else {
                // Several edges, none, or a loop of single edges: a `phi`.
                let number = self.phis.len();
                self.phis.push(Phi::new(current, local));
                self.pending.push(number);
                self.entries.insert((current, local), Value::Phi(number));
                break Value::Phi(number);
            }
        };
        for walked_block in chain {
            self.entries.insert((walked_block, local), value.clone());
        }
        value
    }

    /// The value `local` held where `edge` left: what its source block gave it, or, when the
    /// source block neither assigned it nor read it before a `phi`, `Err` with the source block
    /// whose entry value it is, `None` for the entry block.
    ///
    /// A value that is another local's entry value, as after `x = y` with `y` read on entry,
    /// is that local's entry value, found the same way, so no value returned names a `phi`
    /// that may be left out.
    fn at_source(&mut self, edge: &Edge, local: usize) -> Result<Value, Option<usize>> {
        let Some(value) = edge.values.get(&local) else {
            return Err(edge.from);
        };
        match parse_phi(value) {
            Some((block, read)) if read == local && Some(block) == edge.from => Err(edge.from),
            Some((block, read)) => Ok(self.entry(block, read)),
            None => Ok(Value::Text(value.clone())),
        }
    }

    /// Finds the operands of `phi` number `number`, one per edge into its block.
    fn fill(&mut self, number: usize) {
        let (block, local) = (self.phis[number].block, self.phis[number].local);
        let incoming: Vec<&'s Edge> = self.edges.get(block).cloned().unwrap_or_default();
        let mut operands = Vec::with_capacity(incoming.len());
        for edge in incoming {
            let value = match self.at_source(edge, local) {
                Ok(value) => value,
                Err(Some(from)) => self.entry(from, local),
                Err(None) => Value::Text(self.ssa.entry_value(local)),
            };
            operands.push((value, edge.from_label.as_str()));
        }
        self.phis[number].incoming = operands;
    }
}

/// A `phi` being built: the local it gives a value on entry to its block, and its operands.
struct Phi<'e> {
    /// The body block it is at the top of.
    block: usize,
    /// The local it gives a value.
    local: usize,
    /// Its value on each edge into the block, with the LLVM block the edge leaves from.
    incoming: Vec<(Value, &'e str)>,
}

impl Phi<'_> {
    /// A `phi` of `local` at `block`, its operands not found yet.
    fn new(block: usize, local: usize) -> Self {
        Self {
            block,
            local,
            incoming: Vec::new(),
        }
    }
}

/// An operand of a `phi`: another `phi`, by number, or any other value, as text.
#[derive(Clone, PartialEq, Eq)]
enum Value {
    /// The `phi` with this number.
    Phi(usize),
    /// A value written in the function, such as `%t12`, a constant or an argument.
    Text(String),
}

/// The block and local of the `phi` named `value`, or `None` for any other value.
fn parse_phi(value: &str) -> Option<(usize, usize)> {
    let rest = value.strip_prefix(PHI_PREFIX)?;
    let (local, block) = rest.split_once('.')?;
    Some((block.parse().ok()?, local.parse().ok()?))
}

/// The name of the `phi` that gives `local` its value on entry to `block`.
fn phi_name(local: usize, block: usize) -> String {
    format!("{PHI_PREFIX}{local}.{block}")
}

/// How the name of every `phi` this module makes starts. The `$` appears nowhere else the
/// backend writes, outside text constants, so [`rewrite`] finds names by looking for it.
const PHI_PREFIX: &str = "%ssa$";

/// For each of `phis`, by number, the value it stands for when it is one value, or `None`
/// when it stays a `phi`: a `phi` whose operands, once each replaced `phi` is followed, are all
/// one value or itself. Repeats until no `phi` changes, since replacing one can make another
/// one value.
fn trivial(phis: &[Phi<'_>]) -> Vec<Option<String>> {
    let mut replaced: Vec<Option<Value>> = vec![None; phis.len()];
    let follow = |replaced: &[Option<Value>], value: &Value| -> Value {
        let mut value = value.clone();
        while let Value::Phi(number) = value {
            match &replaced[number] {
                Some(next) => value = next.clone(),
                None => break,
            }
        }
        value
    };
    loop {
        let mut changed = false;
        for (number, phi) in phis.iter().enumerate().rev() {
            if replaced[number].is_some() {
                continue;
            }
            let mut only: Option<Value> = None;
            let mut several = false;
            for (value, _) in &phi.incoming {
                let value = follow(&replaced, value);
                if value == Value::Phi(number) {
                    continue;
                }
                match &only {
                    None => only = Some(value),
                    Some(existing) if *existing == value => {}
                    Some(_) => {
                        several = true;
                        break;
                    }
                }
            }
            if let (false, Some(value)) = (several, only) {
                replaced[number] = Some(value);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    (0..phis.len())
        .map(|number| {
            replaced[number]
                .as_ref()
                .map(|_| match follow(&replaced, &Value::Phi(number)) {
                    Value::Phi(other) => phi_name(phis[other].local, phis[other].block),
                    Value::Text(text) => text,
                })
        })
        .collect()
}

/// `code` with each block's marker replaced by its `phi`s in `phis`, and the name of each `phi`
/// that turned out to be one value replaced by that value, as `names` says.
///
/// Scans `code` once for each pattern, keeping the next place each occurs, rather than line by
/// line: the function's text is most of what the backend writes, and markers and `phi` names
/// are rare in it.
fn rewrite(code: &str, phis: &BTreeMap<usize, String>, names: &HashMap<String, String>) -> String {
    // Each pattern is found by its one rare character, `;` or `$`, which a byte search finds
    // far faster than a search for the whole pattern, then checked whole.
    let next = |pattern: &str, from: usize| {
        let key = pattern.find([';', '$'])?;
        let character = char::from(pattern.as_bytes()[key]);
        let mut start = from + key;
        loop {
            let at = start + code.get(start..)?.find(character)?;
            if at - key >= from && code[at - key..].starts_with(pattern) {
                return Some(at - key);
            }
            start = at + 1;
        }
    };
    let mut text = String::with_capacity(code.len());
    let mut done = 0;
    let mut marker = next(MARKER, 0);
    let mut name = next(PHI_PREFIX, 0);
    loop {
        let position = match (marker, name) {
            (None, None) => break,
            (Some(marker), Some(name)) => marker.min(name),
            (Some(position), None) | (None, Some(position)) => position,
        };
        text.push_str(&code[done..position]);
        if marker == Some(position) {
            // A marker line is `MARKER`, the block's number and a line end.
            let after = &code[position + MARKER.len()..];
            let end = after.find('\n').map_or(after.len(), |end| end + 1);
            let block_phis = after[..end]
                .trim_end()
                .parse()
                .ok()
                .and_then(|block: usize| phis.get(&block));
            if let Some(block_phis) = block_phis {
                text.push_str(block_phis);
            }
            done = position + MARKER.len() + end;
            marker = next(MARKER, done);
        } else {
            let rest = &code[position..];
            let end = rest[PHI_PREFIX.len()..]
                .find(|character: char| !(character.is_ascii_digit() || character == '.'))
                .map_or(rest.len(), |end| end + PHI_PREFIX.len());
            let found = &rest[..end];
            text.push_str(names.get(found).map_or(found, String::as_str));
            done = position + end;
            name = next(PHI_PREFIX, done);
        }
    }
    text.push_str(&code[done..]);
    text
}
