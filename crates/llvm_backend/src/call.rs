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
use crate::strings::{COMPARE, CONTAINS, LENGTH};
use crate::ty::Repr;
use mir::{Argument, BlockId, Builtin, Callee, ErrorEdge, Place, TextMethod};

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
            Callee::Builtin(
                builtin @ (Builtin::File(_)
                | Builtin::ArrayFile(_)
                | Builtin::Path(_)
                | Builtin::Run
                | Builtin::Text(TextMethod::ParseInt | TextMethod::ParseFloat)),
            ) => {
                return self.raising_builtin(*builtin, arguments, destination, target, error);
            }
            Callee::Builtin(builtin) => {
                return self.builtin(*builtin, arguments, destination, target);
            }
            Callee::Value(_) => return Err(Unsupported::new("calls of `Callable` values")),
        };
        let symbol = (self.symbol)(item, type_arguments);
        let (mut written, mut parameters) = self.function_arguments(arguments)?;
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

    /// Writes a call of `builtin`, one of those that can fail: a file operation, an array file,
    /// a path or directory operation, `subprocess.run`, or `int(text)` and `float(text)`. Each
    /// is a call of the runtime that raises through `error`, the call's error edge; a path
    /// operation that cannot fail, such as `path.name`, has no edge.
    fn raising_builtin(
        &mut self,
        builtin: Builtin,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
        error: Option<&ErrorEdge>,
    ) -> Result<(), Unsupported> {
        match builtin {
            Builtin::File(method) => self.file_call(method, arguments, destination, target, error),
            Builtin::ArrayFile(method) => {
                self.array_file_call(method, arguments, destination, target, error)
            }
            Builtin::Path(method) => self.path_call(method, arguments, destination, target, error),
            Builtin::Run => self.subprocess_run(arguments, destination, target, error),
            Builtin::Text(method) => {
                self.parse_number(method, arguments, destination, target, error)
            }
            _ => Err(Unsupported::new(builtin.name())),
        }
    }

    /// The arguments of a call of a compiled function, each as LLVM writes it, `double %3`,
    /// and the parameter types they give its declaration. An argument of a type that takes no
    /// bytes is left out of both.
    fn function_arguments(
        &mut self,
        arguments: &[Argument<'db>],
    ) -> Result<(Vec<String>, Vec<String>), Unsupported> {
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
        Ok((written, parameters))
    }

    /// Writes a call of the built-in function `builtin`, each of which the backend expands in
    /// place or turns into a call of the runtime, rather than a call of a compiled function.
    fn builtin(
        &mut self,
        builtin: Builtin,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        match builtin {
            Builtin::Print => self.print(arguments, target),
            Builtin::Math(function) => self.math(function, arguments, destination, target),
            Builtin::Append => self.list_append(arguments, target),
            Builtin::Pop => self.list_pop(arguments, destination, target),
            Builtin::Sum => self.sum(arguments, destination, target),
            Builtin::Dot => self.dot(arguments, destination, target),
            Builtin::PairwiseLeaves | Builtin::PairwiseLeafEnd => {
                self.pairwise_leaf(arguments, destination, target)
            }
            Builtin::PairwiseCombine => self.pairwise_combine(arguments, destination, target),
            Builtin::Philox => self.philox(arguments, destination, target),
            Builtin::Uniform => self.uniform(arguments, destination, target),
            Builtin::CompareStr => self.text_builtin(&COMPARE, arguments, destination, target),
            Builtin::ContainsStr => self.text_builtin(&CONTAINS, arguments, destination, target),
            Builtin::LengthStr => self.text_builtin(&LENGTH, arguments, destination, target),
            Builtin::CharacterStr => self.text_character(arguments, destination, target),
            Builtin::Text(method) => self.text_method(method, arguments, destination, target),
            Builtin::DictFind => self.dict_find(arguments, destination, target),
            Builtin::DictInsert => self.dict_insert(arguments, destination, target),
            Builtin::DictRemove => self.dict_remove(arguments, destination, target),
            Builtin::DictNext => self.dict_next(arguments, destination, target),
            Builtin::ArgumentCount => self.argument_count(arguments, destination, target),
            Builtin::Argument => self.argument(arguments, destination, target),
            Builtin::EnvironmentGet => self.environment_get(arguments, destination, target),
            Builtin::EnvironmentEntries => self.environment_entries(arguments, destination, target),
            Builtin::Exit => self.system_exit(arguments),
            Builtin::Sort => self.list_sort(arguments, target),
            Builtin::RoundDigits => self.round_digits(arguments, destination, target),
            // `File`, `ArrayFile`, `Path` and `Run` can fail, so `call` sends them to
            // `raising_builtin` with their error edge. The standard streams (decision 0036, part
            // 6) are in `mir` before they are here.
            Builtin::File(_)
            | Builtin::ArrayFile(_)
            | Builtin::Run
            | Builtin::Path(_)
            | Builtin::PrintError
            | Builtin::FlushOutput
            | Builtin::StandardStream
            | Builtin::Input => Err(Unsupported::new(builtin.name())),
        }
    }
}
