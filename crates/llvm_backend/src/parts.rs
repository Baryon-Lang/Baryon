//! Splitting a program's functions into parts, so each part can be its own LLVM module and the
//! driver can compile the parts at once, one `clang` per core (roadmap 2.6).
//!
//! `clang` optimizes and generates code one module at a time, on one core. A program written as
//! one module therefore uses one core however many the machine has. Split into four, the
//! 4,706-line benchmark program of roadmap 2.0 compiled in 0.46 s instead of 1.6 s at `-O0`, and
//! 2.7 s instead of 9.6 s at `-O2`, on four cores.
//!
//! Splitting never changes what a function computes: each function's text is the same in any
//! part, and LLVM optimizes one function's arithmetic without looking at which module holds its
//! callers, so float sums keep their order and results keep their bits. What a split can change
//! is inlining across parts: a call to a function in another part is never inlined, which only
//! matters at `-O2`.

/// The smallest part worth a `clang` of its own, in bytes of function text.
///
/// Every part carries the program's constants, declarations and line-table metadata, and every
/// `clang` costs about 20 ms to start, so a small program compiles fastest as one part. 200 KB is
/// about 4,000 lines of LLVM IR, some 50 lines of Baryon.
pub(crate) const MINIMUM_PART: usize = 200_000;

/// Assigns the definitions whose text lengths are `sizes` to at most `parts` parts, each at
/// least [`MINIMUM_PART`] long where there is that much text, and returns each part's definition
/// indices in increasing order.
///
/// Each definition goes whole to one part, largest first, into the part with the least text so
/// far, the lowest-numbered on a tie: the classic greedy split, within a third of the best
/// balance and the same on every run. Empty parts are left out, so there is always at least one
/// part when there is a definition, and none when there are none.
pub(crate) fn assign(sizes: &[usize], parts: usize) -> Vec<Vec<usize>> {
    let total: usize = sizes.iter().sum();
    let count = parts.min(total / MINIMUM_PART).max(1);
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&index| (std::cmp::Reverse(sizes[index]), index));
    let mut assigned = vec![Vec::new(); count];
    let mut loads = vec![0; count];
    for index in order {
        let lightest = (0..count)
            .min_by_key(|&part| (loads[part], part))
            .unwrap_or(0);
        loads[lightest] += sizes[index];
        assigned[lightest].push(index);
    }
    for part in &mut assigned {
        part.sort_unstable();
    }
    assigned.retain(|part| !part.is_empty());
    assigned
}

/// One in this many definitions ends a [`chunk`](chunks), chosen by its symbol's hash.
const CHUNK_EVERY: u64 = 16;

/// The least function text a chunk ends at on its symbol's hash alone, in bytes: below it, a
/// chunk's `clang` would cost more to start than its text costs to compile.
const MINIMUM_CHUNK: usize = 100_000;

/// The most function text a chunk holds before the definition that reaches it ends it, in
/// bytes: about 20,000 lines of LLVM IR, which `clang -O0` turns into machine code in about
/// 80 ms on the machine of roadmap 2.2.2.
const MAXIMUM_CHUNK: usize = 2_000_000;

/// The least text a definition has to get a chunk of its own, in bytes: about 2,000 lines of
/// LLVM IR. Compiling a definition that large costs more than starting another `clang`, so an
/// edit to it should not also recompile the definitions around it, and an edit around it
/// should not recompile it. On the 4,706-line program of roadmap 2.0, `main`'s 520 KB of text
/// shared a 1.3 MB chunk, which `clang` compiled in about 150 ms, against about 70 ms alone.
const LARGE_DEFINITION: usize = 200_000;

/// Splits the definitions `symbols`, whose text lengths are `sizes`, into runs that an edit
/// leaves alone unless it reaches one of their definitions, for `baryon watch` (roadmap
/// 2.2.2), and returns each run's definition indices in order.
///
/// [`assign`] balances parts by size, so one function growing moves others between parts, and
/// every part changes. Here a definition ends its run where its symbol's hash says so, about
/// one in [`CHUNK_EVERY`], as content-defined chunking does: where a run ends depends on the
/// definitions near it, not on how much text came before. A run also ends at the definition
/// that brings it past [`MAXIMUM_CHUNK`], and never on a hash before [`MINIMUM_CHUNK`]; both
/// depend on sizes, but only within the run, so an edit can move the ends of its own run and
/// the next few, never all the rest. A definition of [`LARGE_DEFINITION`] or more is a run of
/// its own.
pub(crate) fn chunks(symbols: &[&str], sizes: &[usize]) -> Vec<Vec<usize>> {
    let mut chunks = Vec::new();
    let mut chunk = Vec::new();
    let mut size = 0;
    for (index, (symbol, length)) in symbols.iter().zip(sizes).enumerate() {
        if *length >= LARGE_DEFINITION && !chunk.is_empty() {
            chunks.push(std::mem::take(&mut chunk));
            size = 0;
        }
        chunk.push(index);
        size += length;
        let ends = fnv(symbol).is_multiple_of(CHUNK_EVERY) && size >= MINIMUM_CHUNK;
        if ends || size >= MAXIMUM_CHUNK || *length >= LARGE_DEFINITION {
            chunks.push(std::mem::take(&mut chunk));
            size = 0;
        }
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    chunks
}

/// The 64-bit FNV-1a hash of `text`: the same on every run and machine, unlike the standard
/// library's, whose keys are random.
fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::{
        LARGE_DEFINITION, MAXIMUM_CHUNK, MINIMUM_CHUNK, MINIMUM_PART, assign, chunks, fnv,
    };

    #[test]
    fn a_small_program_is_one_chunk() {
        assert_eq!(chunks(&["a", "b", "c"], &[10, 20, 30]), [vec![0, 1, 2]]);
    }

    #[test]
    fn a_chunk_ends_at_its_maximum() {
        assert_eq!(
            chunks(&["a", "b", "c"], &[MAXIMUM_CHUNK, 10, 10]),
            [vec![0], vec![1, 2]]
        );
    }

    #[test]
    fn a_large_definition_is_a_chunk_of_its_own() {
        assert_eq!(
            chunks(&["a", "b", "c", "d"], &[10, LARGE_DEFINITION, 10, 10]),
            [vec![0], vec![1], vec![2, 3]]
        );
    }

    #[test]
    fn a_grown_definition_leaves_later_chunks_alone() {
        let symbols: Vec<String> = (0..200).map(|index| format!("main.f{index}")).collect();
        let symbols: Vec<&str> = symbols.iter().map(String::as_str).collect();
        let mut sizes = vec![MINIMUM_CHUNK; symbols.len()];
        let before = chunks(&symbols, &sizes);
        sizes[3] *= 3;
        let after = chunks(&symbols, &sizes);
        assert!(before.len() > 10, "{before:?}");
        let changed = before.iter().filter(|chunk| !after.contains(chunk)).count();
        assert!(
            changed <= 2,
            "{changed} chunks changed: {before:?} {after:?}"
        );
    }

    #[test]
    fn the_hash_is_fnv_1a() {
        assert_eq!(fnv(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv("a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn a_small_program_stays_one_part() {
        assert_eq!(assign(&[10, 20, 30], 8), [vec![0, 1, 2]]);
    }

    #[test]
    fn no_definitions_give_no_parts() {
        assert!(assign(&[], 4).is_empty());
    }

    #[test]
    fn large_definitions_spread_evenly_and_keep_their_order() {
        let sizes = [MINIMUM_PART; 8];
        assert_eq!(
            assign(&sizes, 4),
            [vec![0, 4], vec![1, 5], vec![2, 6], vec![3, 7]]
        );
    }

    #[test]
    fn the_largest_definitions_are_placed_first() {
        let sizes = [MINIMUM_PART, 3 * MINIMUM_PART, MINIMUM_PART, MINIMUM_PART];
        assert_eq!(assign(&sizes, 2), [vec![1], vec![0, 2, 3]]);
    }

    #[test]
    fn parts_never_outnumber_what_the_text_fills() {
        let sizes = [MINIMUM_PART; 2];
        assert_eq!(assign(&sizes, 16).len(), 2);
    }
}
