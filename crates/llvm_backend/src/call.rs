//! Calls: arguments in their modes, the result stored at the destination, and for a callee that
//! can fail, its error edge.
//!
//! A function that can fail (decision 0005) takes one more parameter, last: a pointer to where
//! it writes its error. It returns `{ T, i1 }`, the value and whether it failed, or only the
//! `i1` when `T` is `None`. The caller passes the address of its error edge's local and
//! branches on the flag; the value is stored at the destination only on success, so a caught
//! error leaves the destination as it was.

use crate::Unsupported;
use crate::function::{Function, block_label};
use crate::literal::global;
use crate::ty::Repr;
use mir::{Argument, BlockId, Builtin, Callee, ErrorEdge, Place};

impl<'db> Function<'_, '_, 'db> {
    /// Writes a call of `callee` with `arguments`, storing its result at `destination` and going
    /// to `target`; when `error` is present, the callee can fail and a failure goes to the edge's
    /// block with the error in the edge's local.
    pub(crate) fn call(
        &mut self,
        callee: &Callee<'db>,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
        error: Option<&ErrorEdge>,
    ) -> Result<(), Unsupported> {
        let (item, type_arguments) = match callee {
            Callee::Function {
                item,
                type_arguments,
            } => (item, type_arguments),
            Callee::Builtin(Builtin::Print) => return self.print(arguments, target),
            Callee::Builtin(Builtin::Math(function)) => {
                return self.math(*function, arguments, destination, target);
            }
            Callee::Builtin(Builtin::Append) => return self.list_append(arguments, target),
            Callee::Builtin(Builtin::Sum) => return self.sum(arguments, destination, target),
            Callee::Value(_) => return Err(Unsupported::new("calls of `Callable` values")),
        };
        let symbol = (self.symbol)(item, type_arguments);
        let mut written = Vec::new();
        let mut parameters = Vec::new();
        for argument in arguments {
            let (ty, value) = match argument {
                Argument::Borrow(place) => {
                    let (address, ty) = self.place(place)?;
                    if self.types.by_value(ty) {
                        let Some(value) = self.load(&address, ty)? else {
                            continue;
                        };
                        (self.types.value(ty)?, value)
                    } else {
                        ("ptr".to_owned(), address)
                    }
                }
                Argument::InOut(place) => ("ptr".to_owned(), self.place(place)?.0),
                Argument::Owned(operand) => {
                    let Some((value, ty)) = self.operand(operand)? else {
                        continue;
                    };
                    (self.types.value(ty)?, value)
                }
            };
            written.push(format!("{ty} {value}"));
            parameters.push(ty);
        }
        if let Some(edge) = error {
            let (address, _) = self.place(&edge.error.into())?;
            written.push(format!("ptr {address}"));
            parameters.push("ptr".to_owned());
        }
        let (address, ty) = self.place(destination)?;
        let value = match self.types.repr(ty)? {
            Repr::Empty => None,
            Repr::Value(llvm) => Some(llvm),
        };
        let returns = match (&value, error) {
            (None, None) => "void".to_owned(),
            (Some(llvm), None) => llvm.clone(),
            (None, Some(_)) => "i1".to_owned(),
            (Some(llvm), Some(_)) => format!("{{ {llvm}, i1 }}"),
        };
        let call = format!("call {returns} {}({})", global(&symbol), written.join(", "));
        self.declare(
            &symbol,
            &format!(
                "declare {returns} {}({})",
                global(&symbol),
                parameters.join(", ")
            ),
        );
        let target = block_label(target);
        match (value, error) {
            (None, None) => self.line(&call),
            (Some(llvm), None) => {
                let result = self.emit(&call);
                self.line(&format!("store {llvm} {result}, ptr {address}"));
            }
            (None, Some(edge)) => {
                let failed = self.emit(&call);
                self.line(&format!(
                    "br i1 {failed}, label %{}, label %{target}",
                    block_label(edge.target)
                ));
                return Ok(());
            }
            (Some(llvm), Some(edge)) => {
                let result = self.emit(&call);
                let failed = self.emit(&format!("extractvalue {returns} {result}, 1"));
                let succeeded = self.fresh_block();
                self.line(&format!(
                    "br i1 {failed}, label %{}, label %{succeeded}",
                    block_label(edge.target)
                ));
                self.line_label(&succeeded);
                let value = self.emit(&format!("extractvalue {returns} {result}, 0"));
                self.line(&format!("store {llvm} {value}, ptr {address}"));
            }
        }
        self.line(&format!("br label %{target}"));
        Ok(())
    }
}
