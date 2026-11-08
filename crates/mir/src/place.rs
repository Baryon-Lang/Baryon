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
    /// A field of a struct or an item of a tuple, by its position in declaration order.
    Field(u32),
    /// One element of an `Array` or `Fixed`, at one index per dimension, each held in a local
    /// of an integer type. The step itself is unchecked: lowering puts a bounds
    /// [`Check`](crate::Check) before it unless the program wrote `a.unchecked[i]`
    /// (decision 0010).
    Index(Vec<Local>),
    /// The value inside a `T | None`. Valid only where the value is known to be present.
    Value,
    /// The error inside an error value received at an [`ErrorEdge`](crate::ErrorEdge), as the
    /// error type at this position in the callee's `Raises` list. Valid only where
    /// [`ErrorIndex`](crate::Rvalue::ErrorIndex) said it is that error.
    Error(u32),
}
