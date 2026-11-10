//! The function compiled code calls to `print` an array (the `print_array` module): one call for
//! the whole array, whatever its rank and element type, since the format of each element depends
//! on all the others.
//!
//! The array arrives as its header's parts, the same for an `Array`, which may be a view, and a
//! `Fixed` array, whose strides compiled code writes as constants: the address of its first
//! element, its rank, and the addresses of its lengths and its strides, counted in elements. The
//! element type is a number, one of the `ELEMENT_*` constants here.

use crate::print;
use crate::print_array::{self, Elements, Plan};

/// The element type `bool`, one byte holding 0 or 1.
pub const ELEMENT_BOOL: u32 = 0;
/// The element type `i8`.
pub const ELEMENT_I8: u32 = 1;
/// The element type `i16`.
pub const ELEMENT_I16: u32 = 2;
/// The element type `i32`.
pub const ELEMENT_I32: u32 = 3;
/// The element type `i64`.
pub const ELEMENT_I64: u32 = 4;
/// The element type `u8`.
pub const ELEMENT_U8: u32 = 5;
/// The element type `u16`.
pub const ELEMENT_U16: u32 = 6;
/// The element type `u32`.
pub const ELEMENT_U32: u32 = 7;
/// The element type `u64`.
pub const ELEMENT_U64: u32 = 8;
/// The element type `f32`.
pub const ELEMENT_F32: u32 = 9;
/// The element type `f64`.
pub const ELEMENT_F64: u32 = 10;
/// The element type `complex64`: two `f32`s, the real part first.
pub const ELEMENT_COMPLEX64: u32 = 11;
/// The element type `complex128`: two `f64`s, the real part first.
pub const ELEMENT_COMPLEX128: u32 = 12;

/// Prints an array as `NumPy`'s `print` writes an `ndarray` (decision 0033): `[1.  2.5]`, a
/// matrix one row per line, `[]` when it has no elements, and only the corners, with `...`,
/// when it has more than 1000. `element` names the element type, one of the `ELEMENT_*`
/// constants; any other number prints nothing, since only a compiler bug passes one.
///
/// Cost: at most 1000 elements read and formatted, however large the array.
///
/// # Safety
///
/// `lengths` and `strides` each point to `rank` readable `i64`s, the lengths not negative, and
/// for every position within the lengths, `data` plus the sum of each index times its stride,
/// in elements of the type `element` names, is an initialized element that nothing writes
/// during the call.
///
/// # Example
///
/// A 2 × 2 matrix of `f64`, row-major:
///
/// ```
/// use runtime::{ELEMENT_F64, baryon_rt_print_array};
///
/// let values = [1.0_f64, 2.5, 3.0, 4.0];
/// let (lengths, strides) = ([2_i64, 2], [2_i64, 1]);
/// // SAFETY: the shape and strides cover the four values. Prints "[[1.  2.5]\n [3.  4. ]]".
/// unsafe {
///     baryon_rt_print_array(
///         1,
///         ELEMENT_F64,
///         values.as_ptr().cast(),
///         2,
///         lengths.as_ptr(),
///         strides.as_ptr(),
///     );
/// }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_print_array(
    stream: i32,
    element: u32,
    data: *const u8,
    rank: u32,
    lengths: *const i64,
    strides: *const i64,
) {
    let rank = rank as usize;
    // SAFETY: the caller guarantees `rank` readable `i64`s at `lengths`.
    let lengths = unsafe { std::slice::from_raw_parts(lengths, rank) };
    // SAFETY: the caller guarantees `rank` readable `i64`s at `strides`.
    let strides = unsafe { std::slice::from_raw_parts(strides, rank) };
    let lengths: Vec<usize> = lengths
        .iter()
        .map(|&length| usize::try_from(length).unwrap_or(0))
        .collect();
    let Some(size) = size(element) else {
        return;
    };
    let plan = Plan::new(&lengths);
    let offsets = offsets(&plan, strides);
    // SAFETY: each offset is a position within the lengths, so the caller guarantees an
    // element of `size` bytes there.
    let bytes = unsafe { gather(data, &offsets, size) };
    let Some(elements) = decode(element, &bytes) else {
        return;
    };
    print::write(stream, &print_array::array(&lengths, &plan, &elements));
}

/// The size in bytes of an element of the type `element` names, or `None` for a number that
/// names none.
fn size(element: u32) -> Option<usize> {
    match element {
        ELEMENT_BOOL | ELEMENT_I8 | ELEMENT_U8 => Some(1),
        ELEMENT_I16 | ELEMENT_U16 => Some(2),
        ELEMENT_I32 | ELEMENT_U32 | ELEMENT_F32 => Some(4),
        ELEMENT_I64 | ELEMENT_U64 | ELEMENT_F64 | ELEMENT_COMPLEX64 => Some(8),
        ELEMENT_COMPLEX128 => Some(16),
        _ => None,
    }
}

/// The elements of the type `element` names, decoded from `bytes`, their copies laid end to
/// end, or `None` for a number that names no type.
fn decode(element: u32, bytes: &[u8]) -> Option<Elements> {
    /// Each `N`-byte run of `bytes`, as an array.
    fn runs<const N: usize>(bytes: &[u8]) -> impl Iterator<Item = [u8; N]> + '_ {
        bytes
            .chunks_exact(N)
            .map(|run| run.try_into().unwrap_or([0; N]))
    }
    Some(match element {
        ELEMENT_BOOL => Elements::Bool(bytes.iter().map(|&byte| byte != 0).collect()),
        ELEMENT_I8 => Elements::Signed(
            runs(bytes)
                .map(|b| i64::from(i8::from_ne_bytes(b)))
                .collect(),
        ),
        ELEMENT_I16 => Elements::Signed(
            runs(bytes)
                .map(|b| i64::from(i16::from_ne_bytes(b)))
                .collect(),
        ),
        ELEMENT_I32 => Elements::Signed(
            runs(bytes)
                .map(|b| i64::from(i32::from_ne_bytes(b)))
                .collect(),
        ),
        ELEMENT_I64 => Elements::Signed(runs(bytes).map(i64::from_ne_bytes).collect()),
        ELEMENT_U8 => Elements::Unsigned(bytes.iter().map(|&byte| u64::from(byte)).collect()),
        ELEMENT_U16 => Elements::Unsigned(
            runs(bytes)
                .map(|b| u64::from(u16::from_ne_bytes(b)))
                .collect(),
        ),
        ELEMENT_U32 => Elements::Unsigned(
            runs(bytes)
                .map(|b| u64::from(u32::from_ne_bytes(b)))
                .collect(),
        ),
        ELEMENT_U64 => Elements::Unsigned(runs(bytes).map(u64::from_ne_bytes).collect()),
        ELEMENT_F32 => Elements::F32(runs(bytes).map(f32::from_ne_bytes).collect()),
        ELEMENT_F64 => Elements::F64(runs(bytes).map(f64::from_ne_bytes).collect()),
        ELEMENT_COMPLEX64 => {
            let parts: Vec<f32> = runs(bytes).map(f32::from_ne_bytes).collect();
            Elements::Complex64(
                parts
                    .chunks_exact(2)
                    .map(|pair| (pair[0], pair[1]))
                    .collect(),
            )
        }
        ELEMENT_COMPLEX128 => {
            let parts: Vec<f64> = runs(bytes).map(f64::from_ne_bytes).collect();
            Elements::Complex128(
                parts
                    .chunks_exact(2)
                    .map(|pair| (pair[0], pair[1]))
                    .collect(),
            )
        }
        _ => return None,
    })
}

/// The offset from the first element, in elements, of each position `plan` keeps, in row-major
/// order: the sum of each index times its dimension's stride.
fn offsets(plan: &Plan, strides: &[i64]) -> Vec<isize> {
    if plan.is_empty() {
        return Vec::new();
    }
    let mut offsets = Vec::new();
    let mut counters = vec![0_usize; plan.kept.len()];
    loop {
        let offset = counters
            .iter()
            .zip(&plan.kept)
            .zip(strides)
            .map(|((&counter, kept), &stride)| {
                i64::try_from(kept[counter])
                    .unwrap_or(i64::MAX)
                    .wrapping_mul(stride)
            })
            .fold(0_i64, i64::wrapping_add);
        offsets.push(isize::try_from(offset).unwrap_or(isize::MAX));
        // Step the counters as an odometer, the last dimension fastest.
        let mut dimension = plan.kept.len();
        loop {
            if dimension == 0 {
                return offsets;
            }
            dimension -= 1;
            counters[dimension] += 1;
            if counters[dimension] < plan.kept[dimension].len() {
                break;
            }
            counters[dimension] = 0;
        }
    }
}

/// The `size` bytes of the element at each of `offsets` from `data`, counted in elements,
/// copied end to end.
///
/// # Safety
///
/// `data` plus each offset times `size` bytes starts `size` initialized bytes.
#[allow(unsafe_code, reason = "reads the elements compiled code points to")]
unsafe fn gather(data: *const u8, offsets: &[isize], size: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(offsets.len() * size);
    for &offset in offsets {
        let start = offset.wrapping_mul(size.cast_signed());
        // SAFETY: the caller guarantees the element starting here is in the array.
        let element = unsafe { data.offset(start) };
        // SAFETY: the caller guarantees `size` initialized bytes there.
        bytes.extend_from_slice(unsafe { std::slice::from_raw_parts(element, size) });
    }
    bytes
}
