//! Tests of the types lowering stores as tuples of arrays (decision 0045): text arrays, arrays
//! with missing values and frames.

use super::{Database, class, layout_of, primitive};
use crate::{Column, Type, TypeKind};

/// A text array is written `Array[str, 1]`, takes the room of its two buffers, each the three
/// words of a rank-1 array, and is stored as the tuple of them.
#[test]
fn a_text_array_shows_as_an_array_of_str_and_is_stored_as_two_buffers() {
    let database = Database::default();
    let text = Type::new(&database, TypeKind::TextArray);
    assert_eq!(text.display(&database).to_string(), "Array[str, 1]");
    assert_eq!(layout_of(&database, text), Ok((48, 8)));
    let stored = text.text_storage(&database);
    assert_eq!(
        stored.map(|ty| ty.display(&database).to_string()),
        Some("tuple[Array[u8, 1], Array[i64, 1]]".to_owned())
    );
    assert_eq!(stored.map(|ty| layout_of(&database, ty)), Some(Ok((48, 8))));
}

/// A frame type of the columns `price: f64` and `volume: i64`, with rows named by `row`.
fn trades<'db>(database: &'db Database, row: Option<Type<'db>>) -> Type<'db> {
    let column = |name: &str, element: &str| Column {
        name: name.to_owned(),
        element: primitive(database, element),
    };
    let columns = vec![column("price", "f64"), column("volume", "i64")];
    Type::new(database, TypeKind::DataFrame { columns, row })
}

#[test]
fn a_frame_shows_its_struct_or_its_columns() {
    let database = Database::default();
    let trade = class(&database, "Trade", false);
    let shown = |ty: Type<'_>| ty.display(&database).to_string();
    assert_eq!(shown(trades(&database, Some(trade))), "DataFrame[Trade]");
    assert_eq!(
        shown(trades(&database, None)),
        "DataFrame[price: f64, volume: i64]"
    );
}

#[test]
fn a_frame_is_stored_as_one_array_per_column() {
    let database = Database::default();
    let frame = trades(&database, None);
    assert_eq!(layout_of(&database, frame), Ok((48, 8)));
    let storage = frame.frame_storage(&database).unwrap();
    assert_eq!(
        storage.display(&database).to_string(),
        "tuple[Array[f64, 1], Array[i64, 1]]"
    );
    assert_eq!(layout_of(&database, storage), Ok((48, 8)));
    let empty = TypeKind::DataFrame {
        columns: Vec::new(),
        row: None,
    };
    assert_eq!(
        layout_of(&database, Type::new(&database, empty)),
        Ok((0, 1))
    );
}

/// A column of `str` is a text array, read as `Array[str, 1]` and stored as its two buffers,
/// so a frame with one beside a column of numbers takes three buffers.
#[test]
fn a_text_column_is_stored_as_a_text_array() {
    let database = Database::default();
    let shown = |ty: Type<'_>| ty.display(&database).to_string();
    let text = Type::new(&database, TypeKind::Str);
    let symbol = Column {
        name: "symbol".to_owned(),
        element: text,
    };
    assert_eq!(shown(symbol.array(&database)), "Array[str, 1]");
    let price = Column {
        name: "price".to_owned(),
        element: primitive(&database, "f64"),
    };
    assert_eq!(shown(price.array(&database)), "Array[f64, 1]");
    let columns = vec![symbol, price];
    let frame = Type::new(&database, TypeKind::DataFrame { columns, row: None });
    assert_eq!(layout_of(&database, frame), Ok((72, 8)));
    let storage = frame.frame_storage(&database).unwrap();
    assert_eq!(
        storage.display(&database).to_string(),
        "tuple[tuple[Array[u8, 1], Array[i64, 1]], Array[f64, 1]]"
    );
    assert_eq!(layout_of(&database, storage), Ok((72, 8)));
}

#[test]
fn frames_with_the_same_columns_are_one_table() {
    let database = Database::default();
    let named = trades(&database, Some(class(&database, "Trade", false)));
    let unnamed = trades(&database, None);
    assert_ne!(named, unnamed);
    assert!(named.same_table(&database, unnamed));
    let price = Column {
        name: "price".to_owned(),
        element: primitive(&database, "f64"),
    };
    let narrower = TypeKind::DataFrame {
        columns: vec![price],
        row: None,
    };
    assert!(!named.same_table(&database, Type::new(&database, narrower)));
    assert!(!named.same_table(&database, primitive(&database, "f64")));
}

/// An array with missing values is written with its element type, and is stored as its values
/// and a validity bitmap: two rank-1 arrays, or three for text, whose values are two buffers.
#[test]
fn an_optional_array_is_stored_as_its_values_and_a_validity_bitmap() {
    let database = Database::default();
    let shown = |ty: Type<'_>| ty.display(&database).to_string();
    let numbers = Type::new(
        &database,
        TypeKind::OptionalArray(primitive(&database, "f64")),
    );
    assert_eq!(shown(numbers), "Array[f64 | None, 1]");
    assert_eq!(layout_of(&database, numbers), Ok((48, 8)));
    assert_eq!(
        numbers.optional_storage(&database).map(shown),
        Some("tuple[Array[f64, 1], Array[u8, 1]]".to_owned())
    );
    let text = Type::new(
        &database,
        TypeKind::OptionalArray(Type::new(&database, TypeKind::Str)),
    );
    assert_eq!(shown(text), "Array[str | None, 1]");
    assert_eq!(layout_of(&database, text), Ok((72, 8)));
    assert_eq!(
        text.optional_storage(&database).map(shown),
        Some("tuple[tuple[Array[u8, 1], Array[i64, 1]], Array[u8, 1]]".to_owned())
    );
}

/// A column that may hold `None` adds its validity bitmap to the frame, and reads as an array
/// with missing values.
#[test]
fn an_optional_column_is_stored_with_its_validity_bitmap() {
    let database = Database::default();
    let f64 = primitive(&database, "f64");
    let column = |name: &str, element| Column {
        name: name.to_owned(),
        element,
    };
    let bid = column("bid", Type::new(&database, TypeKind::Optional(f64)));
    assert_eq!(
        bid.array(&database),
        Type::new(&database, TypeKind::OptionalArray(f64))
    );
    let columns = vec![column("price", f64), bid];
    let frame = Type::new(&database, TypeKind::DataFrame { columns, row: None });
    let stored = frame.frame_storage(&database);
    assert_eq!(
        stored.map(|ty| ty.display(&database).to_string()),
        Some("tuple[Array[f64, 1], tuple[Array[f64, 1], Array[u8, 1]]]".to_owned())
    );
    assert_eq!(layout_of(&database, frame), Ok((72, 8)));
    assert_eq!(stored.map(|ty| layout_of(&database, ty)), Some(Ok((72, 8))));
}
