//! Text files (decision 0036, parts 3 and 4): `open` and the `File` methods, each a call of the
//! runtime, and the drop that closes a `File`.
//!
//! A `File` is a `ptr` to the runtime's open file. Every file call can fail, and the runtime
//! raises the way a compiled function does (the `call` module): its last parameter is the address
//! of the error edge's local, a `Raises[None, OSError]`, and it returns an `i1` that is true when
//! it failed. A result goes through an `out` pointer, the destination's address, which the runtime
//! writes only on success, so a caught error leaves the destination as it was.
//!
//! | Method | Runtime call |
//! | --- | --- |
//! | [`Open`](mir::FileMethod::Open) | `baryon_rt_file_open(path, length, mode, out, error)` |
//! | [`Read`](mir::FileMethod::Read) | `baryon_rt_file_read(file, out, error)` |
//! | [`ReadLine`](mir::FileMethod::ReadLine) | `baryon_rt_file_read_line(file, out, error)` |
//! | [`ReadLines`](mir::FileMethod::ReadLines) | `baryon_rt_file_read_lines(file, out, error)` |
//! | [`Write`](mir::FileMethod::Write) | `baryon_rt_file_write(file, text, length, out, error)` |
//! | [`Flush`](mir::FileMethod::Flush) | `baryon_rt_file_flush(file, error)` |
//! | [`Close`](mir::FileMethod::Close) | `baryon_rt_file_close(file, error)` |
//! | a drop | `baryon_rt_file_drop(file)`, which panics if the last write fails |

use crate::Unsupported;
use crate::function::{Function, block_label};
use mir::{Argument, BlockId, ErrorEdge, FileMethod, Place};

impl<'db> Function<'_, '_, 'db> {
    /// Writes a call of the file operation `method`, storing its result at `destination` and
    /// going to `target`, or to the error edge's block with the `OSError` in its local.
    pub(crate) fn file_call(
        &mut self,
        method: FileMethod,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
        error: Option<&ErrorEdge>,
    ) -> Result<(), Unsupported> {
        let name = method.name();
        let edge = error.ok_or_else(|| Unsupported::new(format!("`{name}` with no error edge")))?;
        let mut parameters = Vec::new();
        match (method, arguments) {
            (FileMethod::Open, [path, Argument::Owned(mode)]) => {
                let (bytes, length) = self.text_argument(path)?;
                let (mode, _) = self.value(mode)?;
                let mode = self.emit(&format!("trunc i64 {mode} to i8"));
                parameters.extend([
                    format!("ptr {bytes}"),
                    format!("i64 {length}"),
                    format!("i8 zeroext {mode}"),
                ]);
            }
            (FileMethod::Write, [file, text]) => {
                let file = self.file_argument(file)?;
                let (bytes, length) = self.text_argument(text)?;
                parameters.extend([
                    format!("ptr {file}"),
                    format!("ptr {bytes}"),
                    format!("i64 {length}"),
                ]);
            }
            (
                FileMethod::Read
                | FileMethod::ReadLine
                | FileMethod::ReadLines
                | FileMethod::Flush
                | FileMethod::Close,
                [file],
            ) => {
                let file = self.file_argument(file)?;
                parameters.push(format!("ptr {file}"));
            }
            _ => return Err(Unsupported::new(format!("`{name}` with these arguments"))),
        }
        if !matches!(method, FileMethod::Flush | FileMethod::Close) {
            let (out, _) = self.place(destination)?;
            parameters.push(format!("ptr {out}"));
        }
        let (address, _) = self.place(&edge.error.into())?;
        parameters.push(format!("ptr {address}"));
        let symbol = symbol(method);
        let types: Vec<&str> = parameters
            .iter()
            .map(|parameter| parameter.rsplit_once(' ').map_or("", |(ty, _)| ty))
            .collect();
        self.declare(
            symbol,
            &format!(
                "declare zeroext i1 @{symbol}({}) nounwind",
                types.join(", ")
            ),
        );
        let failed = self.emit(&format!(
            "call zeroext i1 @{symbol}({})",
            parameters.join(", ")
        ));
        self.line(&format!(
            "br i1 {failed}, label %{}, label %{}",
            block_label(edge.target),
            block_label(target)
        ));
        Ok(())
    }

    /// Closes the `File` at `address`, writing out its buffer, as its drop.
    pub(crate) fn drop_file(&mut self, address: &str) {
        let file = self.emit(&format!("load ptr, ptr {address}"));
        self.declare(
            "baryon_rt_file_drop",
            "declare void @baryon_rt_file_drop(ptr) nounwind",
        );
        self.line(&format!("call void @baryon_rt_file_drop(ptr {file})"));
    }

    /// The pointer a `File` argument holds: loaded from its place, or the operand passed `Owned`.
    fn file_argument(&mut self, argument: &Argument<'db>) -> Result<String, Unsupported> {
        match argument {
            Argument::Owned(operand) => Ok(self.value(operand)?.0),
            Argument::Borrow(place) | Argument::InOut(place) => {
                let (address, _) = self.place(place)?;
                Ok(self.emit(&format!("load ptr, ptr {address}")))
            }
        }
    }
}

/// The runtime function that performs `method`.
fn symbol(method: FileMethod) -> &'static str {
    match method {
        FileMethod::Open => "baryon_rt_file_open",
        FileMethod::Read => "baryon_rt_file_read",
        FileMethod::ReadLine => "baryon_rt_file_read_line",
        FileMethod::ReadLines => "baryon_rt_file_read_lines",
        FileMethod::Write => "baryon_rt_file_write",
        FileMethod::Flush => "baryon_rt_file_flush",
        FileMethod::Close => "baryon_rt_file_close",
        FileMethod::Print => "baryon_rt_file_print",
    }
}
