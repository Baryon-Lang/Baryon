//! Places: the memory a statement reads or writes, named as an access path.

use crate::Local;

/// A location in memory: a local, then zero or more steps into it. `particles[i].velocity` is
/// the local `particles`, then [`Index`](Projection::Index) by the local holding `i`, then
/// [`Field`](Projection::Field) `velocity`.
///
/// Places are the access paths the reference rules are stated over (decision 0006): two places
/// overlap when one's steps are a prefix of the other's, or when they index the same array.
/// That is why the IR keeps them instead of address arithmetic (decision 0018).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Place {
    /// The local the path starts at.
    pub local: Local,
    /// The steps, outermost first.
    pub projections: Vec<Projection>,
}

impl Place {
    /// This place, one step further in.
    ///
    /// ```
    /// use mir::{Local, Place, Projection};
    ///
    /// // `particles[i].velocity`, with `particles` in local 1 and `i` in local 2.
    /// let velocity = Place::from(Local::new(1))
    ///     .project(Projection::Index(vec![Local::new(2)]))
    ///     .project(Projection::Field(1));
    /// assert_eq!(velocity.local, Local::new(1));
    /// assert_eq!(velocity.projections.len(), 2);
    /// ```
    #[must_use]
    pub fn project(mut self, projection: Projection) -> Self {
        self.projections.push(projection);
        self
    }

    /// Whether the place is a view: its last step is a [`Slice`](Projection::Slice). A view
    /// is a borrow of an array's memory, used only where a borrow can be.
    #[must_use]
    pub fn is_view(&self) -> bool {
        matches!(self.projections.last(), Some(Projection::Slice(_)))
    }
}

impl From<Local> for Place {
    /// The whole of `local`, with no steps.
    fn from(local: Local) -> Self {
        Self {
            local,
            projections: Vec::new(),
        }
    }
}

/// One step from a value into a part of it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Projection {
    /// A field of a struct or an item of a tuple, by its position in declaration order; or a
    /// part of a complex number, a float of its parts' width: 0 is the real part and 1 the
    /// imaginary part, the order `types::layout` stores them in.
    Field(u32),
    /// One element of an `Array` or `Fixed`, at one index per dimension, or of a `list`, at
    /// one index, each held in a local of an integer type. The step itself is unchecked:
    /// lowering puts a bounds [`Check`](crate::Check) before it unless the program wrote
    /// `a.unchecked[i]` (decision 0010).
    Index(Vec<Local>),
    /// A view of part of an `Array` or `Fixed`: one [`SliceIndex`] per dimension, at least one
    /// of them a range (language reference, "Arrays", "Slicing"). The view is an `Array` whose
    /// rank is the number of ranges, of the base's element type. It refers to the base's memory
    /// and owns none: element `k` of a range dimension is element `start + k * step` of the
    /// base's dimension, and an index dimension is fixed at its index and dropped.
    ///
    /// A view is a borrow, so a place that ends in this step is used only where a borrow can
    /// be: as a `Borrow` or `InOut` argument, read by [`Length`](crate::Rvalue::Length), as a
    /// leaf of a whole-array expression, or as the destination of one, which writes the
    /// elements in place. It is never moved, copied whole, dropped or assigned whole, which
    /// [`Body::validate`](crate::Body::validate) checks. A further
    /// [`Index`](Projection::Index) step names one of its elements.
    ///
    /// The step itself is unchecked, like [`Index`](Projection::Index): lowering puts a
    /// [`SliceInBounds`](crate::Check::SliceInBounds) check and, for a step that is not a
    /// constant, a [`NonZeroStep`](crate::Check::NonZeroStep) check with `slice: true` before it.
    Slice(Vec<SliceIndex>),
    /// The value inside a `T | None`. Valid only where the value is known to be present.
    Value,
    /// The error inside an error value received at an [`ErrorEdge`](crate::ErrorEdge), as the
    /// error type at this position in the callee's `Raises` list. Valid only where
    /// [`ErrorIndex`](crate::Rvalue::ErrorIndex) said it is that error.
    Error(u32),
}

/// One dimension of a [`Slice`](Projection::Slice).
///
/// A range is stored as its first index, its number of elements and its step rather than
/// Python's `start:stop:step`, so a backend builds the view's header without dividing: the
/// length is the dimension's new length, and the step multiplies the stride. Lowering works
/// out the length from `stop` once, where it checks the range.
///
/// ```
/// use mir::{Local, Place, Projection, SliceIndex};
///
/// // `grid[:, k]`, with `grid` in local 1: the range in locals 2 to 4, `k` in local 5.
/// let column = Place::from(Local::new(1)).project(Projection::Slice(vec![
///     SliceIndex::Range { start: Local::new(2), length: Local::new(3), step: Local::new(4) },
///     SliceIndex::Index(Local::new(5)),
/// ]));
/// assert!(column.is_view());
/// assert!(!column.project(Projection::Index(vec![Local::new(6)])).is_view());
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SliceIndex {
    /// One index, held in an `i64` local: the dimension is fixed there and leaves the view.
    Index(Local),
    /// A range of the dimension, which the view keeps. Each local holds an `i64`.
    Range {
        /// The index of the first element taken.
        start: Local,
        /// How many elements are taken, 0 or more.
        length: Local,
        /// The distance from one element taken to the next, not 0; negative walks backwards.
        step: Local,
    },
}

impl SliceIndex {
    /// The locals this dimension reads, in the order the text form writes them.
    ///
    /// ```
    /// use mir::{Local, SliceIndex};
    ///
    /// assert_eq!(SliceIndex::Index(Local::new(3)).locals(), [Local::new(3)]);
    /// ```
    #[must_use]
    pub fn locals(&self) -> Vec<Local> {
        match self {
            Self::Index(index) => vec![*index],
            Self::Range {
                start,
                length,
                step,
            } => vec![*start, *length, *step],
        }
    }
}
