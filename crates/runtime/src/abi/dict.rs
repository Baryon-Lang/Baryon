//! The functions compiled code calls for `dict` (the `dict` module, decision 0035): find a key,
//! add one, remove an entry, step to the next entry, free a table, and panic for a missing key.
//!
//! A `dict` value is one pointer to a [`Table`], null while the `dict` is empty, so an empty
//! `dict` allocates nothing. Every call takes the [`DictLayout`] constant the compiler emitted
//! for the `dict`'s type. Compiled code reaches an entry itself, at `table.entries + position *
//! entry_size + 8`, and reads `len` from the table's second word.

use crate::dict::{DictLayout, Key, KeyPart, Table};
use crate::location::Location;
use crate::panic::{Failure, panic};

/// The position of the entry whose key equals the key at `key`, or -1 when there is none,
/// including when `table` is null.
///
/// # Safety
///
/// `table` is null or a table from [`baryon_rt_dict_insert`] that has not been freed; `layout`
/// points to the layout of the `dict`'s type; `key` points to a key of that type.
///
/// # Example
///
/// ```
/// use runtime::{DictLayout, KeyPart, baryon_rt_dict_find, baryon_rt_dict_free,
///     baryon_rt_dict_insert};
///
/// static PARTS: [KeyPart; 1] = [KeyPart { offset: 0, kind: 8 }];
/// let layout = DictLayout { entry_size: 24, key_size: 8, part_count: 1, parts: PARTS.as_ptr() };
/// let mut table = std::ptr::null_mut();
/// let (seven, eight) = (7_i64, 8_i64);
/// // SAFETY: the table starts null and is freed once; the layout and keys are valid.
/// unsafe {
///     let position = baryon_rt_dict_insert(&mut table, &layout, (&raw const seven).cast(),
///         std::ptr::null());
///     assert_eq!(baryon_rt_dict_find(table, &layout, (&raw const seven).cast()), position);
///     assert_eq!(baryon_rt_dict_find(table, &layout, (&raw const eight).cast()), -1);
///     baryon_rt_dict_free(table);
/// }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_dict_find(
    table: *const Table,
    layout: *const DictLayout,
    key: *const u8,
) -> i64 {
    // SAFETY: the caller guarantees `table` is null or a live table, used only for this call.
    let Some(table) = (unsafe { table.as_ref() }) else {
        return -1;
    };
    // SAFETY: the caller guarantees `layout` points to the `dict` type's layout.
    let layout = unsafe { &*layout };
    // SAFETY: the caller guarantees `key` points to a key of the layout's type.
    let key = unsafe { key_at(layout, key) };
    table.find(layout, key)
}

/// Adds the key at `key`, which is not in the `dict` yet, as its last entry, and returns the
/// entry's position. The key's bytes are moved into the table; the entry's value is left for the
/// caller to write. When `*dict` is null a table is allocated first. Adding may rebuild the
/// table, so positions from before the call may change.
///
/// # Panics
///
/// Ends the program with a panic, reporting `location`, when the system has no memory left.
///
/// # Safety
///
/// `dict` points to the `dict`'s pointer, null or a table from this function that has not been
/// freed; `layout` points to the layout of the `dict`'s type; `key` points to a key of that type
/// that is not in the `dict`; and `location` is null or points to a valid [`Location`] that lives
/// as long as the program.
///
/// # Example
///
/// See [`baryon_rt_dict_find`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_dict_insert(
    dict: *mut *mut Table,
    layout: *const DictLayout,
    key: *const u8,
    location: *const Location,
) -> i64 {
    // SAFETY: the caller guarantees `layout` points to the `dict` type's layout.
    let layout = unsafe { &*layout };
    // SAFETY: the caller guarantees `key` points to a key of the layout's type.
    let key = unsafe { key_at(layout, key) };
    // SAFETY: the caller guarantees `dict` points to the `dict`'s pointer.
    let pointer = unsafe { &mut *dict };
    if pointer.is_null() {
        match Table::new(layout) {
            Ok(table) => *pointer = Box::into_raw(Box::new(table)),
            // SAFETY: the caller's promise about `location`, passed on.
            Err(failure) => panic(failure, unsafe { super::location(location) }),
        }
    }
    // SAFETY: `*pointer` is a live table: the caller's, or the one just allocated.
    let table = unsafe { &mut **pointer };
    match table.insert(layout, key) {
        Ok(position) => position,
        // SAFETY: the caller's promise about `location`, passed on.
        Err(failure) => panic(failure, unsafe { super::location(location) }),
    }
}

/// Removes the entry at `position`, as `d.pop(k)` does once it has found `k`: the entry becomes
/// a hole, and the other entries keep their positions. The caller has moved the value out and
/// dropped the key first.
///
/// # Safety
///
/// `table` is a table from [`baryon_rt_dict_insert`] that has not been freed, `layout` points to
/// the layout of its type, and `position` is the position of an entry that holds a key.
///
/// # Example
///
/// ```
/// use runtime::{DictLayout, KeyPart, baryon_rt_dict_find, baryon_rt_dict_free,
///     baryon_rt_dict_insert, baryon_rt_dict_next, baryon_rt_dict_remove};
///
/// static PARTS: [KeyPart; 1] = [KeyPart { offset: 0, kind: 8 }];
/// let layout = DictLayout { entry_size: 24, key_size: 8, part_count: 1, parts: PARTS.as_ptr() };
/// let mut table = std::ptr::null_mut();
/// // SAFETY: the table starts null and is freed once; the layout and keys are valid.
/// unsafe {
///     for key in [10_i64, 20, 30] {
///         baryon_rt_dict_insert(&mut table, &layout, (&raw const key).cast(), std::ptr::null());
///     }
///     let twenty = 20_i64;
///     let position = baryon_rt_dict_find(table, &layout, (&raw const twenty).cast());
///     baryon_rt_dict_remove(table, &layout, position);
///     // The keys left, in insertion order, are at positions 0 and 2.
///     assert_eq!(baryon_rt_dict_next(table, &layout, 0), 0);
///     assert_eq!(baryon_rt_dict_next(table, &layout, 1), 2);
///     assert_eq!(baryon_rt_dict_next(table, &layout, 3), -1);
///     baryon_rt_dict_free(table);
/// }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_dict_remove(
    table: *mut Table,
    layout: *const DictLayout,
    position: i64,
) {
    // SAFETY: the caller guarantees `table` is a live table that only this call uses.
    let table = unsafe { &mut *table };
    // SAFETY: the caller guarantees `layout` points to the `dict` type's layout.
    let layout = unsafe { &*layout };
    table.remove(layout, position);
}

/// The first position at or after `position` that holds a key, or -1 when none does, including
/// when `table` is null. A loop over a `dict` starts at 0 and passes one past each position it
/// visits.
///
/// # Safety
///
/// `table` is null or a table from [`baryon_rt_dict_insert`] that has not been freed, and
/// `layout` points to the layout of its type.
///
/// # Example
///
/// See [`baryon_rt_dict_remove`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_dict_next(
    table: *const Table,
    layout: *const DictLayout,
    position: i64,
) -> i64 {
    // SAFETY: the caller guarantees `table` is null or a live table, used only for this call.
    let Some(table) = (unsafe { table.as_ref() }) else {
        return -1;
    };
    // SAFETY: the caller guarantees `layout` points to the `dict` type's layout.
    let layout = unsafe { &*layout };
    table.next(layout, position)
}

/// The number of keys in the table, 0 when `table` is null: `len(d)`. Compiled code reads the
/// count from the table itself; this is the same answer for code that does not know the layout.
///
/// # Safety
///
/// `table` is null or a table from [`baryon_rt_dict_insert`] that has not been freed.
///
/// # Example
///
/// ```
/// // SAFETY: a null table is an empty `dict`.
/// assert_eq!(unsafe { runtime::baryon_rt_dict_length(std::ptr::null()) }, 0);
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_dict_length(table: *const Table) -> i64 {
    // SAFETY: the caller guarantees `table` is null or a live table, used only for this call.
    unsafe { table.as_ref() }.map_or(0, Table::len)
}

/// Frees a table. A null table does nothing, so an empty `dict` drops like any other. The caller
/// has dropped what the keys and values own first.
///
/// # Safety
///
/// `table` is null or a table from [`baryon_rt_dict_insert`] that has not been freed, and is not
/// used after.
///
/// # Example
///
/// See [`baryon_rt_dict_find`].
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_dict_free(table: *mut Table) {
    if !table.is_null() {
        // SAFETY: the caller guarantees `table` came from `Box::into_raw` in
        // `baryon_rt_dict_insert` and is freed once.
        drop(unsafe { Box::from_raw(table) });
    }
}

/// Ends the program with a panic for `d[k]` or `d.pop(k)` of a key that is not in the `dict`.
/// `key` is the key as Python's `repr` writes it, which compiled code builds on the failing path
/// only.
///
/// Prints `panic: key <key> is not in the dict` and the location, as `baryon_rt_panic` does.
///
/// # Safety
///
/// `key` points to `length` readable bytes, and `location` is null or points to a valid
/// [`Location`] that lives as long as the program.
///
/// # Example
///
/// ```no_run
/// use runtime::{Location, baryon_rt_panic_key};
///
/// static HERE: Location = Location::new("md.byn", 14, 12);
/// let key = "'Xe'";
/// // SAFETY: the key is a constant, the location a static.
/// unsafe { baryon_rt_panic_key(key.as_ptr(), key.len(), &HERE) }
/// ```
#[allow(unsafe_code, reason = "exported to compiled code")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baryon_rt_panic_key(
    key: *const u8,
    length: usize,
    location: *const Location,
) -> ! {
    // SAFETY: the caller guarantees `length` readable bytes at `key`.
    let bytes = unsafe { super::strings::bytes(key, length) };
    let key = String::from_utf8_lossy(bytes);
    // SAFETY: the caller's promise about `location`, passed on.
    panic(Failure::MissingKey { key: &key }, unsafe {
        super::location(location)
    })
}

/// The key of `layout`'s type at `key`.
///
/// # Safety
///
/// `layout.parts` points to `layout.part_count` parts, and `key` to `layout.key_size` readable
/// bytes holding a key of the type, whose `str` parts stay readable while the key is used.
#[allow(unsafe_code, reason = "reads a key compiled code laid out")]
unsafe fn key_at(layout: &DictLayout, key: *const u8) -> Key<'_> {
    let count = usize::try_from(layout.part_count).unwrap_or(0);
    // SAFETY: the caller guarantees `parts` points to `part_count` parts.
    let parts: &[KeyPart] = unsafe { std::slice::from_raw_parts(layout.parts, count) };
    let size = usize::try_from(layout.key_size).unwrap_or(0);
    // SAFETY: the caller guarantees `key_size` readable bytes at `key`.
    let bytes = unsafe { super::strings::bytes(key, size) };
    // SAFETY: the caller guarantees the key's `str` parts point to readable text.
    unsafe { Key::new(bytes, parts) }
}
