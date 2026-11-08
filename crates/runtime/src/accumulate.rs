//! Accumulated arrays: the private copies a `prange` loop adds into when it lists an array in
//! `accumulate=`, and the fixed tree that combines them (decisions 0013 and 0024).
//!
//! Every chunk of the loop gets its own copy of the array, but only of the parts it touches.
//! The array's scalars, the numbers inside its elements, are split into blocks of
//! [`BLOCK_BYTES`]. A chunk makes its block table the first time it adds, and a block the first
//! time it adds into it, filled with the identity: `-0.0` for floats, `0` for integers. A block
//! no chunk touched is therefore exactly the identity, so leaving it out of a sum changes no bits.
//! The block size changes speed and memory, never results.
//!
//! The copies combine in the reference's pairwise tree ("Reproducible reductions"): at stride
//! `s = 1, 2, 4, ...`, copy `i` absorbs copy `i + s` for every `i` divisible by `2s`. A node of
//! the tree runs as soon as both of its halves are complete, on the thread that completed the
//! second, and frees the right half's blocks. The tree is the same as combining after the loop;
//! only the moment each node runs changes. After the loop, the combined copy is added into the
//! array, the array's own value first.
//!
//! Memory: at most `threads + (threads + 1) × ceil(log2(chunks))` copies are alive at once, each
//! with a table of `8` bytes per block (decision 0024 derives the bound).

use crate::Location;
use crate::parallel::chunk_count;
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU8, Ordering};

/// The size of one block of a private copy, in bytes: one page on most systems. Changing it
/// changes only speed and memory, so it changes only with a benchmark that justifies it.
pub(crate) const BLOCK_BYTES: usize = 4096;

/// One block's storage. Words of 8 bytes keep it aligned for every scalar kind, and a boxed
/// array is one pointer, so a block table costs 8 bytes per block.
type Block = Box<[u64; BLOCK_BYTES / 8]>;

/// One chunk's private copy: an entry per block of the array, empty until the chunk adds into it.
type Table = Box<[Option<Block>]>;

/// The kind of scalar an accumulated array holds. Compiled code passes it as the `i32` code
/// [`Kind::from_code`] reads: the integers from `i8` to `u64`, then `f32` and `f64`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// `i8`, code 0.
    I8,
    /// `i16`, code 1.
    I16,
    /// `i32`, code 2.
    I32,
    /// `i64`, code 3.
    I64,
    /// `u8`, code 4.
    U8,
    /// `u16`, code 5.
    U16,
    /// `u32`, code 6.
    U32,
    /// `u64`, code 7.
    U64,
    /// `f32`, code 8.
    F32,
    /// `f64`, code 9.
    F64,
}

impl Kind {
    /// The kind compiled code means by `code`, or `None` for a code no kind has.
    pub(crate) fn from_code(code: i32) -> Option<Self> {
        const KINDS: [Kind; 10] = [
            Kind::I8,
            Kind::I16,
            Kind::I32,
            Kind::I64,
            Kind::U8,
            Kind::U16,
            Kind::U32,
            Kind::U64,
            Kind::F32,
            Kind::F64,
        ];
        usize::try_from(code)
            .ok()
            .and_then(|index| KINDS.get(index).copied())
    }

    /// The size of one scalar, in bytes.
    pub(crate) fn size(self) -> usize {
        match self {
            Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 => 8,
        }
    }
}

/// Calls the generic function `$function` at the Rust type of the scalar kind `$kind`.
macro_rules! with_scalar {
    ($kind:expr, $function:ident($($argument:expr),*)) => {
        match $kind {
            Kind::I8 => $function::<i8>($($argument),*),
            Kind::I16 => $function::<i16>($($argument),*),
            Kind::I32 => $function::<i32>($($argument),*),
            Kind::I64 => $function::<i64>($($argument),*),
            Kind::U8 => $function::<u8>($($argument),*),
            Kind::U16 => $function::<u16>($($argument),*),
            Kind::U32 => $function::<u32>($($argument),*),
            Kind::U64 => $function::<u64>($($argument),*),
            Kind::F32 => $function::<f32>($($argument),*),
            Kind::F64 => $function::<f64>($($argument),*),
        }
    };
}

/// A scalar an accumulated array may hold: its additive identity and its checked addition.
trait Scalar: Copy {
    /// The value a private copy starts at: adding it changes no bits of any value.
    const IDENTITY: Self;

    /// `self + other`, or `None` when an integer sum overflows. Floats never fail.
    fn combine(self, other: Self) -> Option<Self>;
}

/// Implements [`Scalar`] for integer types: identity 0, checked addition.
macro_rules! integer_scalar {
    ($($ty:ty),*) => {$(
        impl Scalar for $ty {
            const IDENTITY: Self = 0;

            fn combine(self, other: Self) -> Option<Self> {
                self.checked_add(other)
            }
        }
    )*};
}

integer_scalar!(i8, i16, i32, i64, u8, u16, u32, u64);

/// Implements [`Scalar`] for float types: identity `-0.0`, which unlike `0.0` leaves `-0.0`
/// unchanged, and plain addition.
macro_rules! float_scalar {
    ($($ty:ty),*) => {$(
        impl Scalar for $ty {
            const IDENTITY: Self = -0.0;

            fn combine(self, other: Self) -> Option<Self> {
                Some(self + other)
            }
        }
    )*};
}

float_scalar!(f32, f64);

/// `block` seen as the scalars it holds.
fn scalars<T: Scalar>(block: &mut Block) -> &mut [T] {
    let count = BLOCK_BYTES / size_of::<T>();
    // SAFETY: the block is `BLOCK_BYTES` bytes aligned to 8, which holds `count` values of any
    // scalar type at their alignment; every bit pattern is a valid integer or float; and the
    // slice borrows the block mutably, so nothing else reads or writes it meanwhile.
    #[allow(unsafe_code, reason = "a block is untyped storage for one scalar kind")]
    unsafe {
        std::slice::from_raw_parts_mut(block.as_mut_ptr().cast::<T>(), count)
    }
}

/// A new block filled with the identity of `T`.
fn fresh<T: Scalar>() -> Block {
    let mut block = Box::new([0; BLOCK_BYTES / 8]);
    scalars::<T>(&mut block).fill(T::IDENTITY);
    block
}

/// Adds `right` into `left`, scalar by scalar, `left` first. Returns `false` when an integer sum
/// overflows.
fn absorb<T: Scalar>(left: &mut Block, right: &mut Block) -> bool {
    let right = &*scalars::<T>(right);
    for (into, &value) in scalars::<T>(left).iter_mut().zip(right) {
        match into.combine(value) {
            Some(sum) => *into = sum,
            None => return false,
        }
    }
    true
}

/// An integer sum overflowed while combining copies, or while adding the result into the array.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Overflow;

/// The private copies of one accumulated array for one `prange` loop, and the tree that
/// combines them. Compiled code holds it only behind the pointer
/// [`baryon_rt_accumulator_new`](crate::baryon_rt_accumulator_new) returns.
pub struct Accumulator {
    /// What the array's scalars are.
    kind: Kind,
    /// How many scalars the array holds: its elements times the scalars in one element.
    scalars: usize,
    /// How many blocks cover the scalars.
    blocks: usize,
    /// How many chunks the loop has: one copy each.
    chunks: usize,
    /// Each chunk's copy, made on its first add. Chunk `k`'s copy is touched only by the thread
    /// running chunk `k` until the chunk is done, then only by the thread that combines it.
    copies: Box<[UnsafeCell<Option<Table>>]>,
    /// How many halves of each tree node are complete, level by level: node `n` of level `l`
    /// combines copy `n × 2^(l+1)` with copy `n × 2^(l+1) + 2^l`.
    arrivals: Box<[Box<[AtomicU8]>]>,
    /// Where the loop is in the source, for an overflow panic.
    pub(crate) location: Option<&'static Location>,
}

#[allow(
    unsafe_code,
    reason = "chunks on different threads own different copies"
)]
// SAFETY: the copies are reached only under the protocol `copies` describes: a chunk's copy has
// one owner at a time, and ownership passes from one thread to another through an
// acquire-release update of `arrivals`, so no two threads ever touch one copy at once.
unsafe impl Sync for Accumulator {}

impl Accumulator {
    /// An accumulator for a loop of `iterations` iterations adding into `scalars` scalars of
    /// `kind`. It allocates the per-chunk slots and the tree's counters, under 40 KiB for 1024
    /// chunks; no copy exists until a chunk adds.
    pub(crate) fn new(
        iterations: i64,
        scalars: usize,
        kind: Kind,
        location: Option<&'static Location>,
    ) -> Self {
        let chunks = usize::try_from(chunk_count(iterations)).unwrap_or(0);
        let per_block = BLOCK_BYTES / kind.size();
        let mut arrivals = Vec::new();
        let mut stride = 1;
        while stride < chunks {
            let nodes = chunks.div_ceil(2 * stride);
            arrivals.push((0..nodes).map(|_| AtomicU8::new(0)).collect());
            stride *= 2;
        }
        Self {
            kind,
            scalars,
            blocks: scalars.div_ceil(per_block),
            chunks,
            copies: (0..chunks).map(|_| UnsafeCell::new(None)).collect(),
            arrivals: arrivals.into_boxed_slice(),
            location,
        }
    }

    /// How many scalars one block holds.
    fn per_block(&self) -> usize {
        BLOCK_BYTES / self.kind.size()
    }

    /// The address of scalar `scalar` in chunk `chunk`'s copy, making the copy's table and the
    /// block if this is the chunk's first add there. The scalar starts at the identity.
    ///
    /// # Safety
    ///
    /// The caller is running chunk `chunk`, which has not called [`Self::chunk_done`], so no
    /// other thread touches its copy; `chunk` is below the chunk count and `scalar` below the
    /// scalar count.
    #[allow(unsafe_code, reason = "a chunk's copy is reached through its cell")]
    pub(crate) unsafe fn slot(&self, chunk: usize, scalar: usize) -> *mut u8 {
        // SAFETY: the caller runs `chunk`, so this thread owns its copy.
        let copy = unsafe { &mut *self.copies[chunk].get() };
        let table = copy.get_or_insert_with(|| (0..self.blocks).map(|_| None).collect());
        let per_block = self.per_block();
        let block =
            table[scalar / per_block].get_or_insert_with(|| with_scalar!(self.kind, fresh()));
        let offset = (scalar % per_block) * self.kind.size();
        block.as_mut_ptr().cast::<u8>().wrapping_add(offset)
    }

    /// Records that chunk `chunk` has run all its iterations, and runs every tree node this
    /// completes: each absorbs its right half into its left, `left ⊕ right`, then frees the
    /// right half.
    ///
    /// # Errors
    ///
    /// [`Overflow`] when an integer sum overflows in a node this call runs.
    ///
    /// # Safety
    ///
    /// The caller ran chunk `chunk`, every add it made is finished, and it calls this once.
    #[allow(unsafe_code, reason = "a finished chunk hands its copy to the tree")]
    pub(crate) unsafe fn chunk_done(&self, chunk: usize) -> Result<(), Overflow> {
        let mut index = chunk;
        let mut stride = 1;
        for level in &self.arrivals {
            let base = index - index % (2 * stride);
            let right = base + stride;
            if right < self.chunks {
                let arrived = level[base / (2 * stride)].fetch_add(1, Ordering::AcqRel);
                if arrived == 0 {
                    return Ok(());
                }
                // SAFETY: both halves of this node are complete, and the other half's thread
                // arrived first and returned, so this thread alone owns both copies; the
                // acquire-release counter makes that thread's writes visible here.
                let left = unsafe { &mut *self.copies[base].get() };
                // SAFETY: as for the left copy.
                let right = unsafe { &mut *self.copies[right].get() };
                self.absorb(left, right.take())?;
            }
            index = base;
            stride *= 2;
        }
        Ok(())
    }

    /// Adds the copy `right` into `left`, block by block. A block present on one side only is
    /// moved, not added, since the missing side is exactly the identity.
    fn absorb(&self, left: &mut Option<Table>, right: Option<Table>) -> Result<(), Overflow> {
        let Some(mut right) = right else {
            return Ok(());
        };
        let Some(left) = left else {
            *left = Some(right);
            return Ok(());
        };
        for (into, from) in left.iter_mut().zip(right.iter_mut()) {
            match (into.as_mut(), from.take()) {
                (_, None) => {}
                (None, Some(block)) => *into = Some(block),
                (Some(into), Some(mut block)) => {
                    if !with_scalar!(self.kind, absorb(into, &mut block)) {
                        return Err(Overflow);
                    }
                }
            }
        }
        Ok(())
    }

    /// Adds the combined copy into the array, each scalar as `array ⊕ copy`, and frees every
    /// copy. Untouched blocks are skipped: the identity changes nothing.
    ///
    /// The array is described as compiled code holds it (decision 0008): `data`, then each
    /// dimension's length and stride in elements, outermost first, and `lanes` scalars per
    /// element, laid out one after another. Scalar `s` is lane `s % lanes` of the element at
    /// row-major position `s / lanes`.
    ///
    /// # Errors
    ///
    /// [`Overflow`] when an integer sum overflows.
    ///
    /// # Safety
    ///
    /// Every chunk has called [`Self::chunk_done`]; `data`, `lengths` and `strides` describe a
    /// live array of this accumulator's kind whose lengths multiply out, times `lanes`, to the
    /// scalar count; and no other thread touches the array meanwhile.
    #[allow(unsafe_code, reason = "writes into the array compiled code passes")]
    pub(crate) unsafe fn finish(
        self,
        data: *mut u8,
        lengths: &[i64],
        strides: &[i64],
        lanes: usize,
    ) -> Result<(), Overflow> {
        let Some(first) = self.copies.into_vec().into_iter().next() else {
            return Ok(());
        };
        let Some(mut table) = first.into_inner() else {
            return Ok(());
        };
        let target = Target {
            data,
            lengths,
            strides,
            lanes,
            scalars: self.scalars,
        };
        let per_block = BLOCK_BYTES / self.kind.size();
        for (index, block) in table.iter_mut().enumerate() {
            if let Some(block) = block {
                // SAFETY: the caller's promise about the array, passed on.
                let added =
                    unsafe { with_scalar!(self.kind, add_into(&target, index * per_block, block)) };
                if !added {
                    return Err(Overflow);
                }
            }
        }
        Ok(())
    }
}

/// The array an accumulator's result is added into, as [`Accumulator::finish`] describes it.
struct Target<'a> {
    /// The first element.
    data: *mut u8,
    /// Each dimension's length, outermost first.
    lengths: &'a [i64],
    /// Each dimension's stride, in elements.
    strides: &'a [i64],
    /// Scalars per element.
    lanes: usize,
    /// Scalars in the whole array.
    scalars: usize,
}

impl Target<'_> {
    /// The element offset, in elements from `data`, of the element at row-major position
    /// `position`.
    fn element(&self, mut position: usize) -> isize {
        let mut offset = 0_isize;
        for (&length, &stride) in self.lengths.iter().zip(self.strides).rev() {
            let length = usize::try_from(length).unwrap_or(1).max(1);
            let index = isize::try_from(position % length).unwrap_or(0);
            offset += index * isize::try_from(stride).unwrap_or(0);
            position /= length;
        }
        offset
    }
}

/// Adds `block`, the scalars from `first` on, into `target`, the array's value first. Returns
/// `false` when an integer sum overflows.
///
/// # Safety
///
/// `target` describes a live array of `T` that nothing else touches meanwhile.
#[allow(unsafe_code, reason = "writes into the array compiled code passes")]
unsafe fn add_into<T: Scalar>(target: &Target<'_>, first: usize, block: &mut Block) -> bool {
    let last = target.scalars.min(first + BLOCK_BYTES / size_of::<T>());
    for (scalar, &value) in (first..last).zip(scalars::<T>(block).iter()) {
        let lane = isize::try_from(scalar % target.lanes).unwrap_or(0);
        let lanes = isize::try_from(target.lanes).unwrap_or(1);
        let offset = target.element(scalar / target.lanes) * lanes + lane;
        // SAFETY: the caller's promise: the offset names a scalar of the live array.
        let address = unsafe { target.data.cast::<T>().offset(offset) };
        // SAFETY: as above, and nothing else touches the array.
        let current = unsafe { address.read() };
        let Some(sum) = current.combine(value) else {
            return false;
        };
        // SAFETY: as above.
        unsafe { address.write(sum) };
    }
    true
}

#[cfg(test)]
mod tests;
