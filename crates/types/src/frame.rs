//! What a `DataFrame` type is made of: how it is stored, and when two of them are one table
//! (decision 0045).

use crate::{Column, Type, TypeKind};

impl<'db> Type<'db> {
    /// How a `DataFrame` is stored: a tuple of its columns, each an `Array` of rank 1, in the
    /// schema's order. A column of `str` is a text array, so its entry is the tuple of two
    /// buffers that stores one ([`Type::text_storage`]). `None` when the type is not a frame.
    ///
    /// Lowering replaces a frame type by this one, so the mid-level IR and every stage after it
    /// store, move, borrow and free a frame as the tuple of arrays it is, with no code of their
    /// own for frames.
    ///
    /// # Example
    ///
    /// ```
    /// use types::{Column, Float, Primitive, Type, TypeKind};
    ///
    /// # #[salsa::db]
    /// # #[derive(Clone, Default)]
    /// # struct Database { storage: salsa::Storage<Self> }
    /// # #[salsa::db]
    /// # impl salsa::Database for Database {}
    /// let database = Database::default();
    /// let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    /// let price = Column { name: "price".to_owned(), element: f64 };
    /// let frame = Type::new(&database, TypeKind::DataFrame { columns: vec![price], row: None });
    ///
    /// let array = Type::new(&database, TypeKind::Array { element: f64, rank: 1 });
    /// let tuple = Type::new(&database, TypeKind::Tuple(vec![array]));
    /// assert_eq!(frame.frame_storage(&database), Some(tuple));
    /// assert_eq!(f64.frame_storage(&database), None);
    /// ```
    #[must_use]
    pub fn frame_storage(self, db: &'db dyn salsa::Database) -> Option<Type<'db>> {
        let TypeKind::DataFrame { columns, .. } = self.kind(db) else {
            return None;
        };
        let arrays = columns
            .iter()
            .map(|column| {
                let array = column.array(db);
                array.text_storage(db).unwrap_or(array)
            })
            .collect();
        Some(Type::new(db, TypeKind::Tuple(arrays)))
    }

    /// Whether `self` and `other` are frames of one table: the same column names and element
    /// types in the same order, whatever struct, if any, names the rows of each.
    ///
    /// Two `Type`s that are equal are the same type, but two frame types that differ only in
    /// the struct that names their rows hold the same table (decision 0045, part 1), so the
    /// type checker accepts one where the other is expected by asking this.
    ///
    /// # Example
    ///
    /// ```
    /// use types::{Column, Float, Primitive, Type, TypeKind};
    ///
    /// # #[salsa::db]
    /// # #[derive(Clone, Default)]
    /// # struct Database { storage: salsa::Storage<Self> }
    /// # #[salsa::db]
    /// # impl salsa::Database for Database {}
    /// let database = Database::default();
    /// let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    /// let frame = |name: &str| {
    ///     let columns = vec![Column { name: name.to_owned(), element: f64 }];
    ///     Type::new(&database, TypeKind::DataFrame { columns, row: None })
    /// };
    /// assert!(frame("price").same_table(&database, frame("price")));
    /// assert!(!frame("price").same_table(&database, frame("cost")));
    /// assert!(!frame("price").same_table(&database, f64));
    /// ```
    #[must_use]
    pub fn same_table(self, db: &'db dyn salsa::Database, other: Type<'db>) -> bool {
        match (self.kind(db), other.kind(db)) {
            (
                TypeKind::DataFrame { columns: ours, .. },
                TypeKind::DataFrame {
                    columns: theirs, ..
                },
            ) => ours == theirs,
            _ => false,
        }
    }
}

impl<'db> Column<'db> {
    /// The type of the column read as a value, `df.price`: an `Array` of rank 1 of its
    /// elements, or the text array, `Array[str, 1]`, when its elements are `str`. A text
    /// array is its own kind of type ([`TypeKind::TextArray`]), so every reader of a column
    /// asks here instead of building the array type itself.
    ///
    /// # Example
    ///
    /// ```
    /// use types::{Column, Float, Primitive, Type, TypeKind};
    ///
    /// # #[salsa::db]
    /// # #[derive(Clone, Default)]
    /// # struct Database { storage: salsa::Storage<Self> }
    /// # #[salsa::db]
    /// # impl salsa::Database for Database {}
    /// let database = Database::default();
    /// let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    /// let price = Column { name: "price".to_owned(), element: f64 };
    /// let numbers = Type::new(&database, TypeKind::Array { element: f64, rank: 1 });
    /// assert_eq!(price.array(&database), numbers);
    ///
    /// let str = Type::new(&database, TypeKind::Str);
    /// let symbol = Column { name: "symbol".to_owned(), element: str };
    /// assert_eq!(symbol.array(&database), Type::new(&database, TypeKind::TextArray));
    /// ```
    #[must_use]
    pub fn array(&self, db: &'db dyn salsa::Database) -> Type<'db> {
        let element = self.element;
        let kind = match element.kind(db) {
            TypeKind::Str => TypeKind::TextArray,
            _ => TypeKind::Array { element, rank: 1 },
        };
        Type::new(db, kind)
    }
}
