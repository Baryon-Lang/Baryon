//! How an array with missing values, `Array[T | None, 1]`, is stored (decision 0045, part 3).

use crate::{Integer, Primitive, Type, TypeKind};

impl<'db> Type<'db> {
    /// How an array with missing values is stored: a tuple of two parts, as Arrow lays out a
    /// nullable column. The first is the values, stored as an array without missing values
    /// is: an `Array[T, 1]`, or for text the [two buffers](Type::text_storage) of a text
    /// array. The second is the validity bitmap, an `Array[u8, 1]` of one bit for each
    /// element, eight to a byte, lowest bit first: bit `i % 8` of byte `i // 8` is 1 when
    /// element `i` is there and 0 when it is `None`. `None` when the type is not such an
    /// array.
    ///
    /// The value stored where an element is missing is not specified and is never read. A
    /// missing value costs one bit, where an array of `T | None` values one beside the other
    /// would spend a whole aligned flag on each: 16 bytes for an `f64 | None`.
    ///
    /// Lowering replaces the type by this one, so the mid-level IR and every stage after it
    /// store, move, borrow and free the array as the tuple it is, with no code of their own
    /// for missing values.
    ///
    /// # Example
    ///
    /// ```
    /// use types::{Float, Integer, Primitive, Type, TypeKind};
    ///
    /// # #[salsa::db]
    /// # #[derive(Clone, Default)]
    /// # struct Database { storage: salsa::Storage<Self> }
    /// # #[salsa::db]
    /// # impl salsa::Database for Database {}
    /// let database = Database::default();
    /// let f64 = Type::new(&database, TypeKind::Primitive(Primitive::Float(Float::F64)));
    /// let u8 = Type::new(&database, TypeKind::Primitive(Primitive::Integer(Integer::U8)));
    /// let array = |element| Type::new(&database, TypeKind::Array { element, rank: 1 });
    /// let optional = Type::new(&database, TypeKind::OptionalArray(f64));
    ///
    /// let tuple = Type::new(&database, TypeKind::Tuple(vec![array(f64), array(u8)]));
    /// assert_eq!(optional.optional_storage(&database), Some(tuple));
    /// assert_eq!(array(f64).optional_storage(&database), None);
    /// ```
    #[must_use]
    pub fn optional_storage(self, db: &'db dyn salsa::Database) -> Option<Type<'db>> {
        let TypeKind::OptionalArray(element) = self.kind(db) else {
            return None;
        };
        let array = |element| Type::new(db, TypeKind::Array { element, rank: 1 });
        let values = match element.kind(db) {
            TypeKind::Str => Type::new(db, TypeKind::TextArray).text_storage(db)?,
            _ => array(*element),
        };
        let byte = Type::new(db, TypeKind::Primitive(Primitive::Integer(Integer::U8)));
        Some(Type::new(db, TypeKind::Tuple(vec![values, array(byte)])))
    }
}
