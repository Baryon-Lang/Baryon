//! Leaving out the rows of a frame that repeat (decision 0045, roadmap 3.13.9):
//! `df.drop_duplicates()` and `df.drop_duplicates(subset=[...])`, a new frame with the
//! first of every set of rows that hold the same values in the columns compared, or as
//! `keep=` asks, the last of each or only the rows that do not repeat (roadmap 3.13.16).
//!
//! The work is Baryon code in the standard library's `<std>/frame_joins.byn` (decision
//! 0038). Each row gets the number of its key the way a join numbers them, `first_rows`
//! keeps the first row of each number, and `taken` copies every column at those rows. The
//! frame is borrowed. Cost for `n` rows: `n log n` comparisons for each column compared,
//! and one copy of the rows kept. A column of `str` is compared by the integers `text_codes`
//! gives for its text, which is one more sort of that column.

use crate::builder::Builder;
use crate::optional_arrays::{VALIDITY, VALUES};
use mir::{Constant, ConstantValue, Operand, Place, Projection, Rvalue};
use syntax::{NodeId, SyntaxKind};
use types::{Primitive, Type, TypeKind};

/// A column the library compares rows by, and the type of its elements.
pub(crate) type Key<'db> = (Place, Type<'db>);

impl<'db> Builder<'db> {
    /// What the call `node` of `drop_duplicates` or `value_counts` on the frame `frame`
    /// compares: the frame borrowed, the positions of the columns `subset` names, by
    /// position or keyword, in the order written, or of every column, and each of those
    /// columns as a key the library compares with its element type, a `bool` column as the
    /// integers 0 and 1, a column of `str` as the integers for its text, and a column that
    /// may hold `None` as two keys ([`Builder::optional_keys`]). `None` after a report. The
    /// checker has made sure of the columns.
    pub(crate) fn compared_columns(
        &mut self,
        frame: NodeId,
        node: NodeId,
    ) -> Option<(Place, Vec<u32>, Vec<Key<'db>>)> {
        let columns = self.frame_columns(frame)?;
        let subset = self
            .written(node)
            .into_iter()
            .find(|&argument| self.tree.kind(argument) != SyntaxKind::Keyword)
            .or_else(|| self.keyword_of(node, "subset"));
        let mut compared: Vec<u32> = Vec::new();
        match subset {
            Some(subset) => {
                for name in self.listed(subset) {
                    let Some((position, _)) = self.named_column(columns, name) else {
                        self.unsupported(name, "this column to compare");
                        return None;
                    };
                    compared.push(position);
                }
            }
            None => compared.extend((0_u32..).zip(columns).map(|(position, _)| position)),
        }
        let source = self.borrow(frame);
        let int = self.int_type();
        let mut keys: Vec<Key<'db>> = Vec::new();
        for (position, column) in (0_u32..).zip(columns) {
            if !compared.contains(&position) {
                continue;
            }
            let values = source.clone().project(Projection::Field(position));
            keys.push(match column.element.kind(self.db) {
                &TypeKind::Optional(element) => {
                    keys.extend(self.optional_keys(&values, element, node)?);
                    continue;
                }
                TypeKind::Str => (self.text_codes(values, node)?, int),
                TypeKind::Primitive(Primitive::Bool) => {
                    (self.converted(values, column.element, int, node), int)
                }
                _ => (values, column.element),
            });
        }
        if keys.is_empty() {
            self.unsupported(node, "comparing the rows of a frame with no columns");
            return None;
        }
        Some((source, compared, keys))
    }

    /// The call `node` of `drop_duplicates` on the frame `frame`, into `place`. The checker
    /// has made sure of the columns `subset` names and of `keep`, which chooses the library
    /// function that picks the rows: the first of each set of rows that repeat, the last
    /// with `keep="last"`, or with `keep=False` only the rows that do not repeat.
    pub(crate) fn frame_distinct(&mut self, place: Place, frame: NodeId, node: NodeId) {
        let Some((source, _, keys)) = self.compared_columns(frame, node) else {
            return;
        };
        let Some(ids) = self.key_ids(&keys, node) else {
            return;
        };
        let kept = match self.keyword_of(node, "keep") {
            Some(keep) if self.tree.kind(keep) == SyntaxKind::False => "single_rows",
            Some(keep) if self.written_text(keep) == "last" => "last_rows",
            _ => "first_rows",
        };
        let positions = self.positions_type();
        let arguments = vec![Place::from(ids)];
        let Some(rows) = self.frame_call(kept, Vec::new(), arguments, positions, node) else {
            return;
        };
        self.drop_later(rows);
        self.each_column(place, frame, &source, ("taken", &Place::from(rows)), node);
    }

    /// A method of the rank-1 array `array` that compares its elements, into `place`:
    /// `argsort` ([`Builder::array_argsort`]), `min` or `max` of a text array
    /// ([`Builder::text_extreme`]), or `unique` or `nunique` (decision 0045), the
    /// distinct elements in the order they first appear, or how many there are, a NaN not
    /// counted, as in pandas. An array is one column compared the
    /// way `drop_duplicates` compares a frame's: its elements are numbered as keys,
    /// `first_rows` finds where each number first stands, and `taken` or `counted_values`
    /// gives the result. Cost for `n` elements: `n log n` comparisons and four arrays of `n`
    /// integers, dropped when the statement ends. A text array is compared by the integers
    /// `text_codes` gives for its text, one more sort, and copied with `text_taken`.
    pub(crate) fn array_compared(
        &mut self,
        place: Place,
        method: &str,
        array: NodeId,
        node: NodeId,
    ) {
        if method == "argsort" {
            return self.array_argsort(place, array, node);
        }
        let source = self.borrow(array);
        if let Some(element) = self.optional_array_element(array) {
            return self.optional_compared(place, method == "unique", (source, element), node);
        }
        if matches!(method, "min" | "max") {
            return self.text_extreme(place, method == "max", (array, source), node);
        }
        let int = self.int_type();
        // The elements as numbers the library compares: a text array as the integers for
        // its text, `bool` as 0 and 1, and any other number as it is.
        let (element, key, compared) = if self.is_text_array(array) {
            let Some(codes) = self.text_codes(source.clone(), node) else {
                return;
            };
            (None, codes, int)
        } else if let &TypeKind::Array { element, .. } = self.ty(array).kind(self.db) {
            let flag = matches!(element.kind(self.db), TypeKind::Primitive(Primitive::Bool));
            let compared = if flag { int } else { element };
            let key = self.converted(source.clone(), element, compared, node);
            (Some(element), key, compared)
        } else {
            return self.unsupported(node, "the distinct elements of this value");
        };
        let Some(ids) = self.key_ids(&[(key.clone(), compared)], node) else {
            return;
        };
        let positions = self.positions_type();
        let arguments = vec![Place::from(ids)];
        let Some(rows) = self.frame_call("first_rows", Vec::new(), arguments, positions, node)
        else {
            return;
        };
        self.drop_later(rows);
        let rows = Place::from(rows);
        let returns = self.ty(node);
        let (function, types, arguments, returns) = match (method, element) {
            ("unique", Some(element)) => ("taken", vec![element], vec![source, rows], returns),
            ("unique", None) => ("text_taken", Vec::new(), vec![source, rows], returns),
            _ => ("counted_values", vec![compared], vec![key, rows], int),
        };
        let Some(result) = self.frame_call(function, types, arguments, returns, node) else {
            return;
        };
        self.assign(place, Rvalue::Use(Operand::Move(Place::from(result))), node);
    }

    /// `values.unique()`, when `distinct`, or `values.nunique()` of the array with missing
    /// values of `element`s borrowed at `source`, into `place`. The elements are compared
    /// as `drop_duplicates` compares a column that may hold `None`
    /// ([`Builder::optional_keys`]), so a `None` equals a `None`. `unique` keeps the first
    /// of each distinct element, one `None` among them where it first stands, as pandas
    /// does; `nunique` counts the distinct elements that are there, a `None` and a NaN not
    /// counted. Cost: that of a plain array, and the two key arrays.
    fn optional_compared(
        &mut self,
        place: Place,
        distinct: bool,
        (source, element): (Place, Type<'db>),
        node: NodeId,
    ) {
        let Some(keys) = self.optional_keys(&source, element, node) else {
            return;
        };
        let Some(ids) = self.key_ids(&keys, node) else {
            return;
        };
        let positions = self.positions_type();
        let arguments = vec![Place::from(ids)];
        let Some(rows) = self.frame_call("first_rows", Vec::new(), arguments, positions, node)
        else {
            return;
        };
        self.drop_later(rows);
        let rows = Place::from(rows);
        let bits = source.clone().project(Projection::Field(VALIDITY));
        if distinct {
            let values = source.project(Projection::Field(VALUES));
            let Some(values) = self.values_call("taken", element, vec![values, rows.clone()], node)
            else {
                return;
            };
            let bitmap = self.bitmap_type();
            let arguments = vec![bits, rows];
            let Some(bits) = self.frame_call("taken_bits", Vec::new(), arguments, bitmap, node)
            else {
                return;
            };
            let whole = self.with_bits(element, values, bits, node);
            return self.assign(place, Rvalue::Use(whole), node);
        }
        let boolean = self.boolean_type();
        let wanted = Operand::Constant(Constant {
            ty: boolean,
            value: ConstantValue::Bool(true),
        });
        let wanted = Place::from(self.held(boolean, wanted, node));
        let arguments = vec![bits, rows, wanted];
        let Some(there) =
            self.frame_call("valid_positions", Vec::new(), arguments, positions, node)
        else {
            return;
        };
        self.drop_later(there);
        let [_, (values, compared)] = keys;
        let (int, arguments) = (self.int_type(), vec![values, Place::from(there)]);
        let Some(count) = self.frame_call("counted_values", vec![compared], arguments, int, node)
        else {
            return;
        };
        self.assign(place, Rvalue::Use(Operand::Move(Place::from(count))), node);
    }

    /// `names.min()` or `names.max()` of the text array `array`, borrowed at `source`, into
    /// `place`: a new `str`, a copy of the element that comes first, or last when `largest`,
    /// by Python's `<` on `str`, found by the library's `text_extreme` in one pass. An empty
    /// array panics, as an array of numbers does, since it has no such element.
    fn text_extreme(
        &mut self,
        place: Place,
        largest: bool,
        (array, source): (NodeId, Place),
        node: NodeId,
    ) {
        let length = self.text_array_length(&source, array);
        self.not_empty(&[length], false, node);
        let boolean = self.boolean_type();
        let largest = Operand::Constant(Constant {
            ty: boolean,
            value: ConstantValue::Bool(largest),
        });
        let largest = Place::from(self.held(boolean, largest, node));
        let text = Type::new(self.db, TypeKind::Str);
        let arguments = vec![source, largest];
        let Some(result) = self.frame_call("text_extreme", Vec::new(), arguments, text, node)
        else {
            return;
        };
        self.assign(place, Rvalue::Use(Operand::Move(Place::from(result))), node);
    }
}
