//! What a program reads from, and gives back to, the process that runs it (decision 0036): the
//! command-line arguments, an environment variable and the exit status, each a call of the
//! runtime.
//!
//! | Built-in | Runtime call |
//! | --- | --- |
//! | [`ArgumentCount`](mir::Builtin::ArgumentCount) | `baryon_rt_argument_count()` |
//! | [`Argument`](mir::Builtin::Argument) | `baryon_rt_argument(index)`, a new owned `str` |
//! | [`EnvironmentGet`](mir::Builtin::EnvironmentGet) | `baryon_rt_environment_get(name, length, found)`, a new owned `str` |
//! | [`Exit`](mir::Builtin::Exit) | `baryon_rt_exit(status, location)`, which never returns |

use crate::Unsupported;
use crate::function::{Function, block_label};
use crate::text::TEXT;
use mir::{Argument, BlockId, Place};

impl<'db> Function<'_, '_, 'db> {
    /// Writes `destination = len(sys.argv)` and goes to `target`.
    pub(crate) fn argument_count(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        if !arguments.is_empty() {
            return Err(Unsupported::new("`len(sys.argv)` with arguments"));
        }
        self.declare(
            "baryon_rt_argument_count",
            "declare i64 @baryon_rt_argument_count() nounwind",
        );
        let count = self.emit("call i64 @baryon_rt_argument_count()");
        let (address, _) = self.place(destination)?;
        self.line(&format!("store i64 {count}, ptr {address}"));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// Writes `destination = sys.argv[index]`, a new owned `str`, and goes to `target`.
    pub(crate) fn argument(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [Argument::Owned(index)] = arguments else {
            return Err(Unsupported::new("`sys.argv` item with these arguments"));
        };
        let (index, _) = self.value(index)?;
        self.declare(
            "baryon_rt_argument",
            &format!("declare {TEXT} @baryon_rt_argument(i64) nounwind"),
        );
        let text = self.emit(&format!("call {TEXT} @baryon_rt_argument(i64 {index})"));
        let (address, _) = self.place(destination)?;
        self.line(&format!("store {TEXT} {text}, ptr {address}"));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// Writes `destination = os.environ.get(name)` as the value, the empty string when the
    /// variable is not set, and stores whether it is set in the `bool` the second argument
    /// names, then goes to `target`.
    pub(crate) fn environment_get(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [name, Argument::InOut(found)] = arguments else {
            return Err(Unsupported::new("`os.environ.get` with these arguments"));
        };
        let name = match name {
            Argument::Owned(operand) => self.value(operand)?.0,
            Argument::Borrow(place) | Argument::InOut(place) => {
                let (address, ty) = self.place(place)?;
                self.load(&address, ty)?
                    .ok_or_else(|| Unsupported::new("`None` where a `str` is needed"))?
            }
        };
        let (bytes, length) = self.text_parts(&name);
        let (found, _) = self.place(found)?;
        self.declare(
            "baryon_rt_environment_get",
            &format!("declare {TEXT} @baryon_rt_environment_get(ptr, i64, ptr) nounwind"),
        );
        let text = self.emit(&format!(
            "call {TEXT} @baryon_rt_environment_get(ptr {bytes}, i64 {length}, ptr {found})"
        ));
        let (address, _) = self.place(destination)?;
        self.line(&format!("store {TEXT} {text}, ptr {address}"));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// Writes `sys.exit(status)`. The runtime never returns, so the block ends with
    /// `unreachable` and `target` is never reached from here.
    pub(crate) fn system_exit(&mut self, arguments: &[Argument<'db>]) -> Result<(), Unsupported> {
        let [Argument::Owned(status)] = arguments else {
            return Err(Unsupported::new("`sys.exit` with these arguments"));
        };
        let (status, _) = self.value(status)?;
        let location = self.constants.location(self.span);
        self.declare(
            "baryon_rt_exit",
            "declare void @baryon_rt_exit(i64, ptr) noreturn nounwind",
        );
        self.line(&format!(
            "call void @baryon_rt_exit(i64 {status}, ptr {location})"
        ));
        self.line("unreachable");
        Ok(())
    }
}
