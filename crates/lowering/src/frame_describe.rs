//! Summing up the columns of a frame (decision 0045, roadmap 3.13.9): `df.describe()`, a
//! new frame of eight rows: the column `statistic` with the names `count`, `mean`, `std`,
//! `min`, `25%`, `50%`, `75%` and `max`, then one `f64` column for each column of numbers.
//!
//! The work is Baryon code in the standard library's `<std>/frame_joins.byn` (decision
//! 0038): `statistic_names` makes the first column, and `described` the eight numbers of one
//! column. A column that may hold `None` is described by the values that are there. Cost for
//! `n` rows: a copy and a sort of each column of numbers, and one more copy
//! of a column that is not `f64`.

use crate::builder::Builder;
use crate::frames::erase;
use crate::optional_arrays::{VALIDITY, VALUES};
use mir::{Operand, Place, Projection, Rvalue};
use syntax::NodeId;
use types::{Float, Primitive, Type, TypeKind};

impl Builder<'_> {
    /// The call `node` of `describe` on the frame `frame`, into `place`. The checker has
    /// chosen the columns: those of whole and real numbers, which the result's type lists
    /// after `statistic`.
    pub(crate) fn frame_describe(&mut self, place: Place, frame: NodeId, node: NodeId) {
        let ty = self.ty(node);
        let Some(columns) = self.frame_columns(frame) else {
            return self.unsupported(node, "this `describe`");
        };
        let source = self.borrow(frame);
        let float = Type::new(self.db, TypeKind::Primitive(Primitive::Float(Float::F64)));
        let numbers = self.array_of(float);
        let text = erase(self.db, Type::new(self.db, TypeKind::TextArray));
        let Some(names) = self.frame_call("statistic_names", Vec::new(), Vec::new(), text, node)
        else {
            return;
        };
        let mut made = vec![Operand::Move(Place::from(names))];
        for (position, column) in (0_u32..).zip(columns) {
            let stored = source.clone().project(Projection::Field(position));
            // A column that may hold `None` is described by the values that are there, which
            // the library's `present` gathers into an array dropped with the statement.
            let (values, element) = match column.element.kind(self.db) {
                &TypeKind::Optional(element) => {
                    let arguments = vec![
                        stored.clone().project(Projection::Field(VALUES)),
                        stored.project(Projection::Field(VALIDITY)),
                    ];
                    let gathered = self.array_of(element);
                    let Some(there) =
                        self.frame_call("present", vec![element], arguments, gathered, node)
                    else {
                        return;
                    };
                    self.drop_later(there);
                    (Place::from(there), element)
                }
                _ => (stored, column.element),
            };
            if !matches!(
                element.kind(self.db),
                TypeKind::Primitive(Primitive::Integer(_) | Primitive::Float(_))
            ) {
                continue;
            }
            let values = self.converted(values, element, float, node);
            let Some(described) =
                self.frame_call("described", Vec::new(), vec![values], numbers, node)
            else {
                return;
            };
            made.push(Operand::Move(Place::from(described)));
        }
        self.assign(place, Rvalue::Aggregate(ty, made), node);
    }
}
