//! Dictionaries (decision 0035): the runtime's hash table, reached through calls, and its
//! entries, reached directly.
//!
//! A `dict[K, V]` value is one pointer to the runtime's table, null while the `dict` is empty.
//! The runtime keeps the entries in insertion order in one array, each entry the record
//! `(u64 hash, K, V)`; the table's first word points to that array. So an entry's `(K, V)` pair,
//! which an [`Index`](mir::Projection::Index) step on a `dict` names, is at
//! `entries + position * entry_size + 8`, laid out as the LLVM structure `{ K, V }`, and reading
//! or writing a value costs no call.
//!
//! The runtime knows nothing of the key type, so each `dict` type gets a layout constant,
//! `@baryon.dict.N`, that describes it: the entry's size, the key's size, and each part of the
//! key that hashes and compares, an integer of 1, 2, 4 or 8 bytes or a `str`. A `bool` is an
//! integer of 1 byte and an enum its tag; a tuple key is the parts of its items.
//!
//! | IR | Code |
//! | --- | --- |
//! | `Aggregate` of a `dict` | a null pointer: the empty `dict` |
//! | [`DictFind`](mir::Builtin::DictFind) | `baryon_rt_dict_find(table, layout, key)` |
//! | [`DictInsert`](mir::Builtin::DictInsert) | `baryon_rt_dict_insert(&table, layout, key, location)` |
//! | [`DictRemove`](mir::Builtin::DictRemove) | the value moved out, the key dropped, then `baryon_rt_dict_remove` |
//! | [`DictNext`](mir::Builtin::DictNext) | `baryon_rt_dict_next(table, layout, position)` |
//! | `Length` of a `dict` | `baryon_rt_dict_length(table)` |
//! | a drop | each key and value dropped when they own memory, then `baryon_rt_dict_free` |

use crate::Unsupported;
use crate::function::{Function, block_label};
use mir::{Argument, BlockId, Local, Operand, Place};
use types::{Layout, Primitive, Type, TypeKind};

/// What the backend needs to know about one `dict` type.
pub(crate) struct Shape<'db> {
    /// The type of an entry's `(K, V)` pair, `tuple[K, V]`.
    pub(crate) pair: Type<'db>,
    /// The type of the values.
    pub(crate) value: Type<'db>,
    /// The type of the keys.
    pub(crate) key: Type<'db>,
    /// The size of an entry in bytes, the record `(u64, K, V)`.
    pub(crate) entry_size: u64,
    /// The layout constant, `@baryon.dict.N`.
    pub(crate) layout: String,
}

impl<'db> Function<'_, '_, 'db> {
    /// The address of the `(K, V)` pair of the entry at the position in `indices` of the `dict`
    /// of type `ty` at `address`, and the pair's type, `tuple[K, V]`.
    pub(crate) fn dict_entry(
        &mut self,
        address: &str,
        ty: Type<'db>,
        indices: &[Local],
    ) -> Result<(String, Type<'db>), Unsupported> {
        let shape = self.dict_shape(ty)?;
        let &[position] = indices else {
            return Err(Unsupported::new(
                "a `dict` entry with more than one position",
            ));
        };
        let position = self.index(position)?;
        let table = self.emit(&format!("load ptr, ptr {address}"));
        let entries = self.emit(&format!("load ptr, ptr {table}"));
        let offset = self.emit(&format!("mul i64 {position}, {}", shape.entry_size));
        let offset = self.emit(&format!("add i64 {offset}, 8"));
        let pair = self.emit(&format!(
            "getelementptr inbounds i8, ptr {entries}, i64 {offset}"
        ));
        Ok((pair, shape.pair))
    }

    /// The number of keys of the `dict` at `address`, an `i64`.
    pub(crate) fn dict_length(&mut self, address: &str) -> String {
        self.declare(
            "baryon_rt_dict_length",
            "declare i64 @baryon_rt_dict_length(ptr) nounwind readonly",
        );
        let table = self.emit(&format!("load ptr, ptr {address}"));
        self.emit(&format!("call i64 @baryon_rt_dict_length(ptr {table})"))
    }

    /// Writes `DictFind(dict, key)`, both borrowed, into the `i64` at `destination`.
    pub(crate) fn dict_find(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [Argument::Borrow(dict), Argument::Borrow(key)] = arguments else {
            return Err(Unsupported::new("`dict` lookup with these arguments"));
        };
        let (dict, ty) = self.place(dict)?;
        let shape = self.dict_shape(ty)?;
        let (key, _) = self.place(key)?;
        self.declare(
            "baryon_rt_dict_find",
            "declare i64 @baryon_rt_dict_find(ptr, ptr, ptr) nounwind readonly",
        );
        let table = self.emit(&format!("load ptr, ptr {dict}"));
        let position = self.emit(&format!(
            "call i64 @baryon_rt_dict_find(ptr {table}, ptr {}, ptr {key})",
            shape.layout
        ));
        self.store_position(destination, &position, target)
    }

    /// Writes `DictInsert(dict, key)`, the `dict` `InOut` and the key moved in, into the `i64`
    /// at `destination`.
    pub(crate) fn dict_insert(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [
            Argument::InOut(dict),
            Argument::Owned(Operand::Move(key) | Operand::Copy(key)),
        ] = arguments
        else {
            return Err(Unsupported::new("`dict` insertion with these arguments"));
        };
        let (dict, ty) = self.place(dict)?;
        let shape = self.dict_shape(ty)?;
        let (key, _) = self.place(key)?;
        let location = self.constants.location(self.span);
        self.declare(
            "baryon_rt_dict_insert",
            "declare i64 @baryon_rt_dict_insert(ptr, ptr, ptr, ptr) nounwind",
        );
        let position = self.emit(&format!(
            "call i64 @baryon_rt_dict_insert(ptr {dict}, ptr {}, ptr {key}, ptr {location})",
            shape.layout
        ));
        self.store_position(destination, &position, target)
    }

    /// Writes `DictNext(dict, position)` into the `i64` at `destination`.
    pub(crate) fn dict_next(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [Argument::Borrow(dict), Argument::Owned(position)] = arguments else {
            return Err(Unsupported::new("`dict` iteration with these arguments"));
        };
        let (dict, ty) = self.place(dict)?;
        let shape = self.dict_shape(ty)?;
        let (position, _) = self.value(position)?;
        self.declare(
            "baryon_rt_dict_next",
            "declare i64 @baryon_rt_dict_next(ptr, ptr, i64) nounwind readonly",
        );
        let table = self.emit(&format!("load ptr, ptr {dict}"));
        let next = self.emit(&format!(
            "call i64 @baryon_rt_dict_next(ptr {table}, ptr {}, i64 {position})",
            shape.layout
        ));
        self.store_position(destination, &next, target)
    }

    /// Writes `DictRemove(dict, position)`: moves the entry's value to `destination`, drops its
    /// key when the key owns memory, then has the runtime mark the entry a hole.
    pub(crate) fn dict_remove(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [
            Argument::InOut(dict),
            Argument::Owned(Operand::Copy(position) | Operand::Move(position)),
        ] = arguments
        else {
            return Err(Unsupported::new("`dict` removal with these arguments"));
        };
        let &[] = position.projections.as_slice() else {
            return Err(Unsupported::new("a `dict` position that is not a local"));
        };
        let (address, ty) = self.place(dict)?;
        let shape = self.dict_shape(ty)?;
        let (pair, _) = self.dict_entry(&address, ty, &[position.local])?;
        let llvm = self.types.value(shape.pair)?;
        let value_at = self.emit(&format!(
            "getelementptr inbounds {llvm}, ptr {pair}, i32 0, i32 1"
        ));
        if let Some(value) = self.load(&value_at, shape.value)? {
            let (out, _) = self.place(destination)?;
            let value_type = self.types.value(shape.value)?;
            self.line(&format!("store {value_type} {value}, ptr {out}"));
        }
        if self.owns_memory(shape.key) {
            let key_at = self.emit(&format!(
                "getelementptr inbounds {llvm}, ptr {pair}, i32 0, i32 0"
            ));
            self.drop_value(&key_at, shape.key)?;
        }
        let position = self.index(position.local)?;
        self.declare(
            "baryon_rt_dict_remove",
            "declare void @baryon_rt_dict_remove(ptr, ptr, i64) nounwind",
        );
        let table = self.emit(&format!("load ptr, ptr {address}"));
        self.line(&format!(
            "call void @baryon_rt_dict_remove(ptr {table}, ptr {}, i64 {position})",
            shape.layout
        ));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// Frees the `dict` of type `ty` at `address`: first what each key and value owns, entry by
    /// entry in order, when they own memory, then the table.
    ///
    /// Cost: one pass over the entries when the keys or values own memory, none otherwise.
    pub(crate) fn free_dict(&mut self, address: &str, ty: Type<'db>) -> Result<(), Unsupported> {
        let shape = self.dict_shape(ty)?;
        let table = self.emit(&format!("load ptr, ptr {address}"));
        let (key_owns, value_owns) = (self.owns_memory(shape.key), self.owns_memory(shape.value));
        if key_owns || value_owns {
            self.declare(
                "baryon_rt_dict_next",
                "declare i64 @baryon_rt_dict_next(ptr, ptr, i64) nounwind readonly",
            );
            let llvm = self.types.value(shape.pair)?;
            let [enter, test, body, done] = [(); 4].map(|()| self.fresh_block());
            let (from, position, after) = (self.fresh(), self.fresh(), self.fresh());
            self.line(&format!("br label %{enter}"));
            self.line_label(&enter);
            self.line(&format!("br label %{test}"));
            self.line_label(&test);
            self.line(&format!(
                "{from} = phi i64 [ 0, %{enter} ], [ {after}, %{body}_end ]"
            ));
            self.line(&format!(
                "{position} = call i64 @baryon_rt_dict_next(ptr {table}, ptr {}, i64 {from})",
                shape.layout
            ));
            let more = self.emit(&format!("icmp sge i64 {position}, 0"));
            self.line(&format!("br i1 {more}, label %{body}, label %{done}"));
            self.line_label(&body);
            let entries = self.emit(&format!("load ptr, ptr {table}"));
            let offset = self.emit(&format!("mul i64 {position}, {}", shape.entry_size));
            let offset = self.emit(&format!("add i64 {offset}, 8"));
            let pair = self.emit(&format!(
                "getelementptr inbounds i8, ptr {entries}, i64 {offset}"
            ));
            for (field, owns, part) in [(0, key_owns, shape.key), (1, value_owns, shape.value)] {
                if owns {
                    let at = self.emit(&format!(
                        "getelementptr inbounds {llvm}, ptr {pair}, i32 0, i32 {field}"
                    ));
                    self.drop_value(&at, part)?;
                }
            }
            self.line(&format!("br label %{body}_end"));
            self.line_label(&format!("{body}_end"));
            self.line(&format!("{after} = add i64 {position}, 1"));
            self.line(&format!("br label %{test}"));
            self.line_label(&done);
        }
        self.declare(
            "baryon_rt_dict_free",
            "declare void @baryon_rt_dict_free(ptr) nounwind",
        );
        self.line(&format!("call void @baryon_rt_dict_free(ptr {table})"));
        Ok(())
    }

    /// Stores the `i64` `position` at `destination` and goes to `target`.
    fn store_position(
        &mut self,
        destination: &Place,
        position: &str,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let (address, _) = self.place(destination)?;
        self.line(&format!("store i64 {position}, ptr {address}"));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// What the backend needs to know about the `dict` type `ty`, with its layout constant made
    /// the first time it is asked for.
    pub(crate) fn dict_shape(&mut self, ty: Type<'db>) -> Result<Shape<'db>, Unsupported> {
        let db = self.types.db;
        let &TypeKind::Dict { key, value } = ty.kind(db) else {
            return Err(self.types.unsupported(ty));
        };
        let pair = Type::new(db, TypeKind::Tuple(vec![key, value]));
        let layout_of = |ty: Type<'db>| {
            types::layout(db, ty, self.types.declared).map_err(|_| self.types.unsupported(ty))
        };
        let word = Layout::new(8, 8).ok_or_else(|| Unsupported::new("a 64-bit word"))?;
        let entry =
            Layout::record([word, layout_of(pair)?]).ok_or_else(|| self.types.unsupported(ty))?;
        if entry.offsets().get(1) != Some(&8) {
            return Err(self.types.unsupported(ty));
        }
        let key_size = layout_of(key)?.size();
        let mut parts = Vec::new();
        self.key_parts(key, 0, &mut parts)?;
        let entry_size = entry.layout().size();
        let layout = self.constants.dict_layout(entry_size, key_size, &parts);
        Ok(Shape {
            pair,
            value,
            key,
            entry_size,
            layout,
        })
    }

    /// Appends the parts of a key of type `ty` at `offset` bytes into the key, as the runtime's
    /// `KeyPart`s: `(offset, size)` for an integer, `bool` or enum tag, `(offset, 0)` for a `str`.
    fn key_parts(
        &self,
        ty: Type<'db>,
        offset: u64,
        parts: &mut Vec<(u64, u64)>,
    ) -> Result<(), Unsupported> {
        let db = self.types.db;
        match ty.kind(db) {
            TypeKind::Str => parts.push((offset, 0)),
            TypeKind::Primitive(Primitive::Bool | Primitive::Integer(_)) | TypeKind::Enum(_) => {
                let size = types::layout(db, ty, self.types.declared)
                    .map_err(|_| self.types.unsupported(ty))?
                    .size();
                parts.push((offset, size));
            }
            TypeKind::Tuple(items) => {
                let layouts = items
                    .iter()
                    .map(|&item| {
                        types::layout(db, item, self.types.declared)
                            .map_err(|_| self.types.unsupported(item))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let record = Layout::record(layouts).ok_or_else(|| self.types.unsupported(ty))?;
                for (&item, &at) in items.iter().zip(record.offsets()) {
                    self.key_parts(item, offset + at, parts)?;
                }
            }
            _ => {
                return Err(Unsupported::new(format!(
                    "`dict` keys of type `{}`",
                    ty.display(db)
                )));
            }
        }
        Ok(())
    }
}
