//! Joining two frames (decision 0045, roadmap 3.13.8): `left.merge(right, on=...)`, a new
//! frame with one row for each pair of rows whose key columns hold the same values.
//!
//! The work is Baryon code in the standard library's `<std>/frame_joins.byn` (decision
//! 0038), whose documentation describes the method: the key columns of the two frames are
//! put end to end and sorted together, each row gets the number of its key, and the join is
//! then one of two columns of integers. Lowering calls the generic functions once for each
//! key column and copies each column of the result with `taken`. Both frames are borrowed.
//! A key of `str` is joined on the integers the library's `text_codes` gives for the text of
//! the two columns end to end, and a column of `str` is copied with `text_taken`.

use crate::builder::Builder;
use crate::frames::erase;
use mir::{Constant, ConstantValue, Local, Operand, Place, Projection, Rvalue};
use syntax::{NodeId, SyntaxKind};
use types::{Primitive, Type, TypeKind};

impl<'db> Builder<'db> {
    /// The call `node`, `left.merge(right, on=...)` with `rest` the arguments after `left`
    /// as the checker recorded them, into `place`. The checker has made sure of the keys.
    pub(crate) fn frame_merge(
        &mut self,
        place: Place,
        left: NodeId,
        rest: &[NodeId],
        node: NodeId,
    ) {
        let ty = self.ty(node);
        let on = self
            .written(node)
            .into_iter()
            .filter(|&argument| self.tree.kind(argument) == SyntaxKind::Keyword)
            .find(|&argument| self.keyword_name(argument) == Some("on"))
            .map(|argument| self.keyword_value(argument));
        let (Some(&right), Some(on)) = (rest.first(), on) else {
            return self.unsupported(node, "this `merge`");
        };
        let (Some(left_columns), Some(right_columns)) =
            (self.frame_columns(left), self.frame_columns(right))
        else {
            return self.unsupported(node, "this `merge`");
        };
        let left_source = self.borrow(left);
        let right_source = self.borrow(right);
        let int = self.int_type();
        let positions = self.positions_type();
        // Each key column of the two frames end to end, a `bool` key as the integers 0 and 1.
        let mut keys: Vec<(Place, Type<'db>)> = Vec::new();
        let mut names = Vec::new();
        for key in self.listed(on) {
            let (Some((left_at, column)), Some((right_at, _))) = (
                self.named_column(left_columns, key),
                self.named_column(right_columns, key),
            ) else {
                return self.unsupported(key, "this column to join on");
            };
            names.push(column.name.as_str());
            let sides = [
                left_source.clone().project(Projection::Field(left_at)),
                right_source.clone().project(Projection::Field(right_at)),
            ];
            let Some(both) = self.joined_key(sides, column.element, key) else {
                return;
            };
            keys.push(both);
        }
        let count = self.counted_rows(left, &left_source, node);
        let Some(pairs) = self.join_pairs(&keys, count, node) else {
            return;
        };
        let call = |builder: &mut Self, arguments: Vec<Place>| {
            let rows = builder.frame_call("pair_side", Vec::new(), arguments, positions, node)?;
            builder.drop_later(rows);
            Some(rows)
        };
        let mut made = Vec::new();
        for (side, source, columns) in [
            (0, &left_source, left_columns),
            (1, &right_source, right_columns),
        ] {
            let which = Operand::Constant(Constant::integer(int, side));
            let which = self.held(int, which, node);
            let arguments = vec![Place::from(pairs), Place::from(which)];
            let Some(rows) = call(self, arguments) else {
                return;
            };
            for (position, column) in (0_u32..).zip(columns) {
                // The keys come from the left frame; the right frame's would repeat them.
                if side == 1 && names.contains(&column.name.as_str()) {
                    continue;
                }
                // A column of `str` has its own function, `text_taken`.
                let (function, types) = match column.element.kind(self.db) {
                    TypeKind::Str => ("text_taken", Vec::new()),
                    _ => ("taken", vec![column.element]),
                };
                let array = erase(self.db, column.array(self.db));
                let from = source.clone().project(Projection::Field(position));
                let arguments = vec![from, Place::from(rows)];
                let Some(taken) = self.frame_call(function, types, arguments, array, node) else {
                    return;
                };
                made.push(Operand::Move(Place::from(taken)));
            }
        }
        self.assign(place, Rvalue::Aggregate(ty, made), node);
    }

    /// The key column of the two frames end to end, `sides` with the left frame's first,
    /// whose elements are `element`, as a column the library's sort takes, and its element
    /// type: a key of `str` as the integers for its text (`text_joined`, then `text_codes`),
    /// a key of `bool` as the integers 0 and 1, and any other as it is. The new arrays are
    /// dropped when the statement ends. `None` after an error is reported.
    fn joined_key(
        &mut self,
        sides: [Place; 2],
        element: Type<'db>,
        key: NodeId,
    ) -> Option<(Place, Type<'db>)> {
        let int = self.int_type();
        if matches!(element.kind(self.db), TypeKind::Str) {
            let text = erase(self.db, Type::new(self.db, TypeKind::TextArray));
            let both = self.frame_call("text_joined", Vec::new(), sides.to_vec(), text, key)?;
            self.drop_later(both);
            let codes = self.text_codes(Place::from(both), key)?;
            return Some((codes, int));
        }
        let flag = matches!(element.kind(self.db), TypeKind::Primitive(Primitive::Bool));
        let compared = if flag { int } else { element };
        let sides = sides.map(|side| self.converted(side, element, compared, key));
        let array = self.array_of(compared);
        let both = self.frame_call("joined", vec![compared], sides.to_vec(), array, key)?;
        self.drop_later(both);
        Some((Place::from(both), compared))
    }

    /// The pairs of rows an inner join on `keys` gives, as the library's `join_pairs` lists
    /// them: each of `keys` is a key column of the two frames end to end, the left frame's
    /// `left_count` rows first, and its element type. `None` after an error is reported.
    fn join_pairs(
        &mut self,
        keys: &[(Place, Type<'db>)],
        left_count: Local,
        node: NodeId,
    ) -> Option<Local> {
        let positions = self.positions_type();
        let ids = self.key_ids(keys, node)?;
        let arguments = vec![Place::from(ids), Place::from(left_count)];
        let pairs = self.frame_call("join_pairs", Vec::new(), arguments, positions, node)?;
        self.drop_later(pairs);
        Some(pairs)
    }

    /// A number for each row of `keys`, columns of one length each with its element type:
    /// the same number for two rows exactly when every column holds the same value in both,
    /// two NaN counting as the same. The rows are sorted by the columns, the last first,
    /// and numbered where the sorted values change. `None` after an error is reported.
    pub(crate) fn key_ids(&mut self, keys: &[(Place, Type<'db>)], node: NodeId) -> Option<Local> {
        let (first, _) = keys.first()?;
        let int = self.int_type();
        let boolean = self.boolean_type();
        let positions = self.positions_type();
        let total = self.temporary(int, node);
        self.assign(Place::from(total), Rvalue::Length(first.clone(), 0), node);
        let rising = Operand::Constant(Constant {
            ty: boolean,
            value: ConstantValue::Bool(true),
        });
        let rising = self.held(boolean, rising, node);
        let call = |builder: &mut Self,
                    function: &str,
                    element: Option<Type<'db>>,
                    arguments: Vec<Place>,
                    returns: Type<'db>| {
            let type_arguments = element.into_iter().collect();
            let result = builder.frame_call(function, type_arguments, arguments, returns, node)?;
            builder.drop_later(result);
            Some(result)
        };
        let all = vec![Place::from(total)];
        let mut order = call(self, "row_positions", None, all, positions)?;
        for (key, element) in keys.iter().rev() {
            let arguments = vec![key.clone(), Place::from(order), Place::from(rising)];
            let next = call(self, "ordered", Some(*element), arguments, positions)?;
            order = next;
        }
        let flags = self.array_of(boolean);
        let mut marks = call(self, "no_marks", None, vec![Place::from(order)], flags)?;
        for (key, element) in keys {
            let arguments = vec![key.clone(), Place::from(order), Place::from(marks)];
            let next = call(self, "key_marks", Some(*element), arguments, flags)?;
            marks = next;
        }
        let arguments = vec![Place::from(order), Place::from(marks)];
        call(self, "group_ids", None, arguments, positions)
    }

    /// The name of the keyword argument `argument`, `on` in `on="symbol"`.
    fn keyword_name(&self, argument: NodeId) -> Option<&'db str> {
        self.tree
            .child(argument, syntax::Role::Name)
            .map(|name| self.text_of(name))
    }
}
