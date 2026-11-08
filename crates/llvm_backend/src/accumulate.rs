//! Arrays a `prange` loop lists in `accumulate=` (decisions 0013 and 0024): each chunk adds into
//! private blocks the runtime makes on first touch, and the blocks combine into the array after
//! the loop in the reference's fixed tree.
//!
//! The function running the loop calls `baryon_rt_accumulator_new` for each accumulated array
//! before `baryon_rt_parallel_for`, and `baryon_rt_accumulator_finish` after it. It passes the
//! chunk function two context slots per array: the address of the array's header and the
//! accumulator.
//!
//! The chunk function builds a *view* per array on its stack, `{ header, ptr accumulator, i64
//! chunk }`, and passes its address where the outlined body expects the array. The header comes
//! first, so the body's bounds checks read the lengths through the parameter as for any array.
//! When the chunk's last iteration ends, it calls `baryon_rt_accumulator_chunk_done`.
//!
//! In the body, an [`Accumulate`](mir::StatementKind::Accumulate) finds the element's row-major
//! position from the indices and the lengths, asks `baryon_rt_accumulator_slot` for the address
//! of each of its scalars in the chunk's copy, and adds there with the arithmetic of `+=`. A
//! `Fixed` element is added lane by lane.

use crate::Unsupported;
use crate::function::Function;
use crate::ty::Number;
use mir::{AccumulateOperator, Operand, Place, Projection};
use std::fmt::Write as _;
use types::{Type, TypeKind};

/// The runtime functions an accumulated array calls, as the runtime exports them.
const NEW: &str = "declare ptr @baryon_rt_accumulator_new(i64, i64, i32, ptr)";

/// See [`NEW`].
const SLOT: &str = "declare ptr @baryon_rt_accumulator_slot(ptr, i64, i64) nounwind";

/// See [`NEW`].
const CHUNK_DONE: &str = "declare void @baryon_rt_accumulator_chunk_done(ptr, i64)";

/// See [`NEW`].
const FINISH: &str = "declare void @baryon_rt_accumulator_finish(ptr, ptr, i64, ptr, ptr, i64)";

/// One accumulated array of a loop, as the function running the loop writes it.
pub(crate) struct Accumulated {
    /// The address of the array's header.
    pub(crate) address: String,
    /// The header's LLVM type.
    pub(crate) header: String,
    /// The accumulator `baryon_rt_accumulator_new` returned.
    pub(crate) accumulator: String,
    /// The array's rank.
    rank: u32,
    /// Scalars per element.
    lanes: u64,
}

/// What an accumulated array's elements are made of.
struct Scalars {
    /// The runtime's code for the scalar kind (`baryon_rt_accumulator_new`).
    kind: i32,
    /// The scalar's kind of number.
    number: Number,
    /// The scalar's LLVM type.
    llvm: String,
    /// The lengths of a `Fixed` element, outermost first; empty for a number.
    shape: Vec<u64>,
}

impl Scalars {
    /// How many scalars one element holds.
    fn lanes(&self) -> u64 {
        self.shape.iter().product()
    }
}

impl<'db> Function<'_, '_, 'db> {
    /// What the elements of the array type `ty` are made of: numbers, or `Fixed` arrays of
    /// them.
    fn scalars(&self, ty: Type<'db>) -> Result<Scalars, Unsupported> {
        let (element, _) = self.array(ty)?;
        let (scalar, shape) = match element.kind(self.types.db) {
            TypeKind::Fixed { element, shape } => (*element, shape.lengths().to_vec()),
            _ => (element, Vec::new()),
        };
        let (number, llvm) = self.scalar(scalar)?;
        let kind = match number {
            Number::Integer { bits, signed } => {
                let width = match bits {
                    8 => 0,
                    16 => 1,
                    32 => 2,
                    64 => 3,
                    _ => return Err(self.types.unsupported(scalar)),
                };
                if signed { width } else { width + 4 }
            }
            Number::Float(32) => 8,
            Number::Float(_) => 9,
            Number::Bool => return Err(self.types.unsupported(scalar)),
        };
        Ok(Scalars {
            kind,
            number,
            llvm,
            shape,
        })
    }

    /// Starts an accumulator for each array in `accumulations`, for a loop of `iterations`
    /// iterations. Cost: one runtime call each; no private copy exists yet.
    pub(crate) fn start_accumulators(
        &mut self,
        accumulations: &[Place],
        iterations: &str,
    ) -> Result<Vec<Accumulated>, Unsupported> {
        let mut started = Vec::new();
        for place in accumulations {
            let (address, ty) = self.place(place)?;
            let (_, rank) = self.array(ty)?;
            let scalars = self.scalars(ty)?;
            let header = self.types.value(ty)?;
            let lanes = scalars.lanes();
            let mut count = lanes.to_string();
            for dimension in 0..rank {
                let field = self.emit(&format!(
                    "getelementptr inbounds {header}, ptr {address}, i32 0, i32 1, i32 {dimension}"
                ));
                let length = self.emit(&format!("load i64, ptr {field}"));
                count = self.emit(&format!("mul i64 {count}, {length}"));
            }
            self.declare("baryon_rt_accumulator_new", NEW);
            let location = self.constants.location(self.span);
            let accumulator = self.emit(&format!(
                "call ptr @baryon_rt_accumulator_new(i64 {iterations}, i64 {count}, i32 {}, \
                 ptr {location})",
                scalars.kind
            ));
            started.push(Accumulated {
                address,
                header,
                accumulator,
                rank,
                lanes,
            });
        }
        Ok(started)
    }

    /// Adds each accumulator's combined copies into its array, after the loop.
    pub(crate) fn finish_accumulators(&mut self, accumulated: &[Accumulated]) {
        for array in accumulated {
            let Accumulated {
                address,
                header,
                accumulator,
                rank,
                lanes,
            } = array;
            self.declare("baryon_rt_accumulator_finish", FINISH);
            let data = self.emit(&format!("load ptr, ptr {address}"));
            let lengths = self.emit(&format!(
                "getelementptr inbounds {header}, ptr {address}, i32 0, i32 1, i32 0"
            ));
            let strides = self.emit(&format!(
                "getelementptr inbounds {header}, ptr {address}, i32 0, i32 2, i32 0"
            ));
            self.line(&format!(
                "call void @baryon_rt_accumulator_finish(ptr {accumulator}, ptr {data}, \
                 i64 {rank}, ptr {lengths}, ptr {strides}, i64 {lanes})"
            ));
        }
    }

    /// Writes an `Accumulate`: adds `value` into the element at `place`, an accumulated
    /// parameter indexed and possibly then indexed into its `Fixed` element, in the running
    /// chunk's copy.
    pub(crate) fn accumulate(
        &mut self,
        place: &Place,
        operator: AccumulateOperator,
        value: &Operand<'db>,
    ) -> Result<(), Unsupported> {
        let (view, ty) = self.place(&Place::from(place.local))?;
        let header = self.types.value(ty)?;
        let scalars = self.scalars(ty)?;
        let (indices, inner) = match place.projections.as_slice() {
            [Projection::Index(indices)] => (indices, None),
            [Projection::Index(indices), Projection::Index(inner)] => (indices, Some(inner)),
            _ => {
                return Err(Unsupported::new(
                    "this target of an add into an accumulated array",
                ));
            }
        };
        let mut position = "0".to_owned();
        for (dimension, &index) in indices.iter().enumerate() {
            let index = self.index(index)?;
            let field = self.emit(&format!(
                "getelementptr inbounds {header}, ptr {view}, i32 0, i32 1, i32 {dimension}"
            ));
            let length = self.emit(&format!("load i64, ptr {field}"));
            let scaled = self.emit(&format!("mul i64 {position}, {length}"));
            position = self.emit(&format!("add i64 {scaled}, {index}"));
        }
        let lanes = scalars.lanes();
        let first = self.emit(&format!("mul i64 {position}, {lanes}"));
        let (value, _) = self.value(value)?;
        let mut lane_values = Vec::new();
        if let Some(inner) = inner {
            let mut lane = "0".to_owned();
            for (&index, length) in inner.iter().zip(&scalars.shape) {
                let index = self.index(index)?;
                let scaled = self.emit(&format!("mul i64 {lane}, {length}"));
                lane = self.emit(&format!("add i64 {scaled}, {index}"));
            }
            lane_values.push((lane, value));
        } else {
            let element = self.types.value(self.array(ty)?.0)?;
            for (lane, path) in lane_paths(&scalars.shape).into_iter().enumerate() {
                let scalar = if path.is_empty() {
                    value.clone()
                } else {
                    self.emit(&format!("extractvalue {element} {value}, {path}"))
                };
                lane_values.push((lane.to_string(), scalar));
            }
        }
        self.declare("baryon_rt_accumulator_slot", SLOT);
        let view_type = format!("{{ {header}, ptr, i64 }}");
        let accumulator_field = self.emit(&format!(
            "getelementptr inbounds {view_type}, ptr {view}, i32 0, i32 1"
        ));
        let accumulator = self.emit(&format!("load ptr, ptr {accumulator_field}"));
        let chunk_field = self.emit(&format!(
            "getelementptr inbounds {view_type}, ptr {view}, i32 0, i32 2"
        ));
        let chunk = self.emit(&format!("load i64, ptr {chunk_field}"));
        let llvm = &scalars.llvm;
        let instruction = match (scalars.number, operator) {
            (Number::Float(_), AccumulateOperator::Add) => "fadd",
            (Number::Float(_), AccumulateOperator::Subtract) => "fsub",
            (_, AccumulateOperator::Add) => "add",
            (_, AccumulateOperator::Subtract) => "sub",
        };
        for (lane, scalar) in lane_values {
            let index = self.emit(&format!("add i64 {first}, {lane}"));
            let slot = self.emit(&format!(
                "call ptr @baryon_rt_accumulator_slot(ptr {accumulator}, i64 {chunk}, i64 {index})"
            ));
            let current = self.emit(&format!("load {llvm}, ptr {slot}"));
            let sum = self.emit(&format!("{instruction} {llvm} {current}, {scalar}"));
            self.line(&format!("store {llvm} {sum}, ptr {slot}"));
        }
        Ok(())
    }
}

/// The `extractvalue` index path of each lane of a `Fixed` element of `shape`, in row-major
/// order: `0, 0` then `0, 1` and so on. One empty path for a number.
fn lane_paths(shape: &[u64]) -> Vec<String> {
    let mut paths = vec![Vec::new()];
    for &length in shape {
        paths = paths
            .into_iter()
            .flat_map(|path: Vec<u64>| {
                (0..length).map(move |index| {
                    let mut longer = path.clone();
                    longer.push(index);
                    longer
                })
            })
            .collect();
    }
    paths
        .into_iter()
        .map(|path| {
            path.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .collect()
}

/// The chunk function's part for the accumulated arrays whose headers have the LLVM types
/// `headers`, whose context slots start at `first`: the entry code that builds each view, the
/// arguments that pass the views to the body, and the code that tells the runtime the chunk is
/// done. `slots` is the context's size.
pub(crate) fn chunk_views(headers: &[String], first: usize, slots: usize) -> [String; 3] {
    let mut entry = String::new();
    let mut arguments = Vec::new();
    let mut done = String::new();
    for (index, header) in headers.iter().enumerate() {
        let (array, accumulator) = (first + 2 * index, first + 2 * index + 1);
        let view = format!("{{ {header}, ptr, i64 }}");
        let _ = writeln!(
            entry,
            "  %ha{index} = getelementptr inbounds [{slots} x ptr], ptr %context, i64 0, i64 {array}\n  \
             %h{index} = load ptr, ptr %ha{index}\n  \
             %aa{index} = getelementptr inbounds [{slots} x ptr], ptr %context, i64 0, i64 {accumulator}\n  \
             %a{index} = load ptr, ptr %aa{index}\n  \
             %w{index} = alloca {view}\n  \
             %hv{index} = load {header}, ptr %h{index}\n  \
             store {header} %hv{index}, ptr %w{index}\n  \
             %wa{index} = getelementptr inbounds {view}, ptr %w{index}, i32 0, i32 1\n  \
             store ptr %a{index}, ptr %wa{index}\n  \
             %wc{index} = getelementptr inbounds {view}, ptr %w{index}, i32 0, i32 2\n  \
             store i64 %chunk, ptr %wc{index}"
        );
        arguments.push(format!("ptr %w{index}"));
        let _ = writeln!(
            done,
            "  call void @baryon_rt_accumulator_chunk_done(ptr %a{index}, i64 %chunk)"
        );
    }
    [entry, arguments.join(", "), done]
}

/// Declares `baryon_rt_accumulator_chunk_done` for a chunk function that calls it.
pub(crate) fn declare_chunk_done(function: &mut Function<'_, '_, '_>) {
    function.declare("baryon_rt_accumulator_chunk_done", CHUNK_DONE);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A number has one lane with an empty path; a `Fixed` element's lanes are its elements in
    /// row-major order.
    #[test]
    fn lanes_are_row_major() {
        assert_eq!(lane_paths(&[]), [""]);
        assert_eq!(lane_paths(&[3]), ["0", "1", "2"]);
        assert_eq!(lane_paths(&[2, 2]), ["0, 0", "0, 1", "1, 0", "1, 1"]);
    }

    /// The chunk function builds one view per accumulated array from its two context slots,
    /// passes the view, and reports the chunk done.
    #[test]
    fn a_chunk_builds_a_view_per_array() {
        let header = "{ ptr, [1 x i64], [1 x i64] }".to_owned();
        let [entry, arguments, done] = chunk_views(&[header], 1, 3);
        assert_eq!(
            entry,
            "  %ha0 = getelementptr inbounds [3 x ptr], ptr %context, i64 0, i64 1
  %h0 = load ptr, ptr %ha0
  %aa0 = getelementptr inbounds [3 x ptr], ptr %context, i64 0, i64 2
  %a0 = load ptr, ptr %aa0
  %w0 = alloca { { ptr, [1 x i64], [1 x i64] }, ptr, i64 }
  %hv0 = load { ptr, [1 x i64], [1 x i64] }, ptr %h0
  store { ptr, [1 x i64], [1 x i64] } %hv0, ptr %w0
  %wa0 = getelementptr inbounds { { ptr, [1 x i64], [1 x i64] }, ptr, i64 }, ptr %w0, i32 0, i32 1
  store ptr %a0, ptr %wa0
  %wc0 = getelementptr inbounds { { ptr, [1 x i64], [1 x i64] }, ptr, i64 }, ptr %w0, i32 0, i32 2
  store i64 %chunk, ptr %wc0
"
        );
        assert_eq!(arguments, "ptr %w0");
        assert_eq!(
            done,
            "  call void @baryon_rt_accumulator_chunk_done(ptr %a0, i64 %chunk)\n"
        );
    }
}
