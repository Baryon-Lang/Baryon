//! How a text array, `Array[str, 1]`, is stored (decision 0045).

use crate::{Integer, Primitive, Type, TypeKind};

impl<'db> Type<'db> {
    /// How a text array is stored: a tuple of two arrays of rank 1, as Arrow's `large_utf8`
    /// lays text out. The first is the bytes of every element in UTF-8, end to end, an
    /// `Array[u8, 1]`. The second is the offsets, an `Array[i64, 1]` one longer than the text
    /// array: element `i` is the bytes from `offsets[i]` up to `offsets[i + 1]`, and
    /// `offsets[0]` is 0. `None` when the type is not a text array.
    ///
    /// The whole array is two allocations whatever its length, where a `list[str]` makes one
    /// for each element. Lowering replaces a text array type by this one, so the mid-level IR
    /// and every stage after it store, move, borrow and free a text array as the tuple it is,
    /// with no code of their own for text arrays.
    ///
    /// # Example
    ///
    /// ```
    /// use types::{Integer, Primitive, Type, TypeKind};
    ///
    /// # #[salsa::db]
    /// # #[derive(Clone, Default)]
    /// # struct Database { storage: salsa::Storage<Self> }
    /// # #[salsa::db]
    /// # impl salsa::Database for Database {}
    /// let database = Database::default();
    /// let number = |integer| {
    ///     Type::new(&database, TypeKind::Primitive(Primitive::Integer(integer)))
    /// };
    /// let array = |element| Type::new(&database, TypeKind::Array { element, rank: 1 });
    /// let text = Type::new(&database, TypeKind::TextArray);
    ///
    /// let stored = vec![array(number(Integer::U8)), array(number(Integer::I64))];
    /// let tuple = Type::new(&database, TypeKind::Tuple(stored));
    /// assert_eq!(text.text_storage(&database), Some(tuple));
    /// assert_eq!(number(Integer::I64).text_storage(&database), None);
    /// ```
    #[must_use]
    pub fn text_storage(self, db: &'db dyn salsa::Database) -> Option<Type<'db>> {
        if !matches!(self.kind(db), TypeKind::TextArray) {
            return None;
        }
        let array = |integer| {
            let element = Type::new(db, TypeKind::Primitive(Primitive::Integer(integer)));
            Type::new(db, TypeKind::Array { element, rank: 1 })
        };
        let buffers = vec![array(Integer::U8), array(Integer::I64)];
        Some(Type::new(db, TypeKind::Tuple(buffers)))
    }
}
