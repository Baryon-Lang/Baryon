//! Tests of the table on its own: integer, tuple and `str` keys, insertion order, removal and
//! growth.

use super::{DictLayout, Key, KeyPart, STR_PART, Table};

/// The parts of an `i64` key.
static INTEGER: [KeyPart; 1] = [KeyPart { offset: 0, kind: 8 }];

/// The layout of a `dict[i64, i64]`: hash, key and value, 24 bytes.
fn integers() -> DictLayout {
    DictLayout {
        entry_size: 24,
        key_size: 8,
        part_count: 1,
        parts: INTEGER.as_ptr(),
    }
}

/// The key `value` of an integer `dict`, given as its bytes, `value.to_ne_bytes()`.
fn integer(value: &[u8; 8]) -> Key<'_> {
    // SAFETY: an integer key has no `str` part.
    #[allow(unsafe_code, reason = "builds a key under test")]
    unsafe {
        Key::new(value, &INTEGER)
    }
}

/// The keys of `table`, read back in iteration order.
fn keys(table: &Table, layout: &DictLayout) -> Vec<i64> {
    let mut keys = Vec::new();
    let mut position = table.next(layout, 0);
    while position >= 0 {
        let key = table.key_at(layout, position, &INTEGER);
        keys.push(i64::from_ne_bytes(key.bytes.try_into().unwrap()));
        position = table.next(layout, position + 1);
    }
    keys
}

#[test]
fn finds_what_it_holds_and_nothing_else() {
    let layout = integers();
    let mut table = Table::new(&layout).unwrap();
    for value in 0..1000_i64 {
        let key = value * 7919;
        assert_eq!(table.find(&layout, integer(&key.to_ne_bytes())), -1);
        let position = table.insert(&layout, integer(&key.to_ne_bytes())).unwrap();
        assert_eq!(table.find(&layout, integer(&key.to_ne_bytes())), position);
    }
    assert_eq!(table.len(), 1000);
    for value in 0..1000_i64 {
        assert!(table.find(&layout, integer(&(value * 7919).to_ne_bytes())) >= 0);
        assert_eq!(
            table.find(&layout, integer(&(value * 7919 + 1).to_ne_bytes())),
            -1
        );
    }
}

#[test]
fn keeps_insertion_order_through_growth_and_removal() {
    let layout = integers();
    let mut table = Table::new(&layout).unwrap();
    for key in [5_i64, -3, 40, 2, 17] {
        table.insert(&layout, integer(&key.to_ne_bytes())).unwrap();
    }
    let position = table.find(&layout, integer(&40_i64.to_ne_bytes()));
    table.remove(&layout, position);
    assert_eq!(table.find(&layout, integer(&40_i64.to_ne_bytes())), -1);
    assert_eq!(keys(&table, &layout), [5, -3, 2, 17]);
    // Adding the removed key again puts it last, as Python does.
    table
        .insert(&layout, integer(&40_i64.to_ne_bytes()))
        .unwrap();
    for key in 100..200_i64 {
        table.insert(&layout, integer(&key.to_ne_bytes())).unwrap();
    }
    let expected: Vec<i64> = [5, -3, 2, 17, 40].into_iter().chain(100..200).collect();
    assert_eq!(keys(&table, &layout), expected);
    assert_eq!(table.len(), 105);
}

#[test]
fn reclaims_holes_instead_of_growing_forever() {
    let layout = integers();
    let mut table = Table::new(&layout).unwrap();
    for key in 0..100_000_i64 {
        let position = table.insert(&layout, integer(&key.to_ne_bytes())).unwrap();
        table.remove(&layout, position);
    }
    assert_eq!(table.len(), 0);
    assert_eq!(table.next(&layout, 0), -1);
    assert!(table.index.len() <= 8);
}

#[test]
fn tuple_keys_compare_their_parts_and_ignore_padding() {
    // `tuple[bool, i64]`: the flag at byte 0, seven bytes of padding, the integer at byte 8.
    static PARTS: [KeyPart; 2] = [
        KeyPart { offset: 0, kind: 1 },
        KeyPart { offset: 8, kind: 8 },
    ];
    /// The key in `bytes`.
    fn view(bytes: &[u8; 16]) -> Key<'_> {
        // SAFETY: the keys have no `str` part.
        #[allow(unsafe_code, reason = "builds keys under test")]
        unsafe {
            Key::new(bytes.as_slice(), &PARTS)
        }
    }
    let layout = DictLayout {
        entry_size: 32,
        key_size: 16,
        part_count: 2,
        parts: PARTS.as_ptr(),
    };
    let key = |flag: u8, padding: u8, value: i64| {
        let mut bytes = [padding; 16];
        bytes[0] = flag;
        bytes[8..].copy_from_slice(&value.to_ne_bytes());
        bytes
    };
    let mut table = Table::new(&layout).unwrap();
    let stored = key(1, 0xAA, 42);
    let position = table.insert(&layout, view(&stored)).unwrap();
    assert_eq!(table.find(&layout, view(&key(1, 0x55, 42))), position);
    assert_eq!(table.find(&layout, view(&key(0, 0xAA, 42))), -1);
    assert_eq!(table.find(&layout, view(&key(1, 0xAA, 43))), -1);
}

#[test]
fn str_keys_compare_characters_not_addresses_or_flags() {
    static PARTS: [KeyPart; 1] = [KeyPart {
        offset: 0,
        kind: STR_PART,
    }];
    /// The key in `bytes`.
    fn view(bytes: &[u8; 16]) -> Key<'_> {
        // SAFETY: each header points to its text, which outlives the table.
        #[allow(unsafe_code, reason = "builds keys under test")]
        unsafe {
            Key::new(bytes.as_slice(), &PARTS)
        }
    }
    let layout = DictLayout {
        entry_size: 32,
        key_size: 16,
        part_count: 1,
        parts: PARTS.as_ptr(),
    };
    let header = |text: &str, literal: bool| {
        let mut bytes = [0_u8; 16];
        bytes[..8].copy_from_slice(&(text.as_ptr() as usize).to_ne_bytes());
        let flag = if literal { 1 << 63 } else { 0 };
        bytes[8..].copy_from_slice(&(text.len() as u64 | flag).to_ne_bytes());
        bytes
    };
    let owned = String::from("argon");
    let (argon, neon) = (header("argon", true), header("neon", true));
    let copy = header(&owned, false);
    let mut table = Table::new(&layout).unwrap();
    let position = table.insert(&layout, view(&argon)).unwrap();
    assert_eq!(table.find(&layout, view(&copy)), position);
    assert_eq!(table.find(&layout, view(&neon)), -1);
    assert_eq!(table.find(&layout, view(&header("", true))), -1);
}

#[test]
fn hashes_are_deterministic_and_never_a_hole() {
    // The seed is a constant, so a key hashes the same in every run and probes the same slots.
    // Iteration order never depends on the hash: it is insertion order.
    assert_eq!(
        integer(&1_i64.to_ne_bytes()).hash(),
        integer(&1_i64.to_ne_bytes()).hash()
    );
    assert_ne!(
        integer(&1_i64.to_ne_bytes()).hash(),
        integer(&2_i64.to_ne_bytes()).hash()
    );
    assert_eq!(integer(&0_i64.to_ne_bytes()).hash() >> 63, 0);
}
