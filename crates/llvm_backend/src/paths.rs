//! Paths and directories (decision 0036, parts 1 and 7): each [`PathMethod`] is a call of the
//! runtime.
//!
//! A path is passed as its text, `ptr bytes, i64 count`. A method that makes text stores the
//! new `str` the runtime returns; a question stores the `i1` it returns. A method that can fail
//! raises as a file call does (the `files` module): its last parameter is the address of the
//! error edge's local, a `Raises[None, OSError]`, it returns an `i1` that is true when it
//! failed, and a result goes through an `out` pointer, the destination's address.
//!
//! | Method | Runtime call |
//! | --- | --- |
//! | [`New`](PathMethod::New), [`Name`](PathMethod::Name), [`Stem`](PathMethod::Stem), [`Suffix`](PathMethod::Suffix), [`Parent`](PathMethod::Parent) | `baryon_rt_path_part(path, length, part)`, `part` 0 to 4 in that order |
//! | [`Join`](PathMethod::Join) | `baryon_rt_path_join(path, length, other, other_length)` |
//! | [`Exists`](PathMethod::Exists), [`IsFile`](PathMethod::IsFile), [`IsDirectory`](PathMethod::IsDirectory) | `baryon_rt_path_is(path, length, what)`, `what` 0 to 2 in that order |
//! | [`ListDirectory`](PathMethod::ListDirectory), [`IterateDirectory`](PathMethod::IterateDirectory) | `baryon_rt_path_list(path, length, joined, out, error)`, `joined` for `iterdir` |
//! | [`Remove`](PathMethod::Remove) | `baryon_rt_path_remove(path, length, error)` |
//! | [`Rename`](PathMethod::Rename) | `baryon_rt_path_rename(source, length, destination, destination_length, error)` |
//! | [`MakeDirectory`](PathMethod::MakeDirectory), [`MakeDirectories`](PathMethod::MakeDirectories) | `baryon_rt_path_make_directory(path, length, parents, exist_ok, error)`, `parents` for `makedirs` |
//! | [`CurrentDirectory`](PathMethod::CurrentDirectory) | `baryon_rt_current_directory(out, error)` |

use crate::Unsupported;
use crate::function::{Function, block_label};
use crate::text::TEXT;
use mir::{Argument, BlockId, ErrorEdge, PathMethod, Place};

impl<'db> Function<'_, '_, 'db> {
    /// Writes a call of the path operation `method`, storing its result at `destination` and
    /// going to `target`, or, for a method that can fail, to the error edge's block with the
    /// `OSError` in its local.
    pub(crate) fn path_call(
        &mut self,
        method: PathMethod,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
        error: Option<&ErrorEdge>,
    ) -> Result<(), Unsupported> {
        let name = method.name();
        let wrong = || Unsupported::new(format!("`{name}` with these arguments"));
        // The texts every method starts with: two for a join and a rename, none for the
        // current directory, one for the rest.
        let texts = match method {
            PathMethod::Join | PathMethod::Rename => 2,
            PathMethod::CurrentDirectory => 0,
            _ => 1,
        };
        let flags = usize::from(method == PathMethod::MakeDirectories);
        if arguments.len() != texts + flags {
            return Err(wrong());
        }
        let mut parameters = Vec::new();
        for text in &arguments[..texts] {
            let (bytes, length) = self.text_argument(text)?;
            parameters.extend([format!("ptr {bytes}"), format!("i64 {length}")]);
        }
        let (symbol, returns) = match method {
            PathMethod::New
            | PathMethod::Name
            | PathMethod::Stem
            | PathMethod::Suffix
            | PathMethod::Parent => {
                let part = match method {
                    PathMethod::New => 0,
                    PathMethod::Name => 1,
                    PathMethod::Stem => 2,
                    PathMethod::Suffix => 3,
                    _ => 4,
                };
                parameters.push(format!("i8 zeroext {part}"));
                ("baryon_rt_path_part", Some(TEXT))
            }
            PathMethod::Join => ("baryon_rt_path_join", Some(TEXT)),
            PathMethod::Exists | PathMethod::IsFile | PathMethod::IsDirectory => {
                let what = match method {
                    PathMethod::Exists => 0,
                    PathMethod::IsFile => 1,
                    _ => 2,
                };
                parameters.push(format!("i8 zeroext {what}"));
                ("baryon_rt_path_is", Some("i1"))
            }
            PathMethod::ListDirectory | PathMethod::IterateDirectory => {
                let joined = method == PathMethod::IterateDirectory;
                let (out, _) = self.place(destination)?;
                parameters.extend([format!("i1 zeroext {joined}"), format!("ptr {out}")]);
                ("baryon_rt_path_list", None)
            }
            PathMethod::Remove => ("baryon_rt_path_remove", None),
            PathMethod::Rename => ("baryon_rt_path_rename", None),
            PathMethod::MakeDirectory => {
                parameters.extend(["i1 zeroext false".to_owned(), "i1 zeroext false".to_owned()]);
                ("baryon_rt_path_make_directory", None)
            }
            PathMethod::MakeDirectories => {
                let Argument::Owned(exist_ok) = &arguments[texts] else {
                    return Err(wrong());
                };
                let (exist_ok, _) = self.value(exist_ok)?;
                parameters.extend([
                    "i1 zeroext true".to_owned(),
                    format!("i1 zeroext {exist_ok}"),
                ]);
                ("baryon_rt_path_make_directory", None)
            }
            PathMethod::CurrentDirectory => {
                let (out, _) = self.place(destination)?;
                parameters.push(format!("ptr {out}"));
                ("baryon_rt_current_directory", None)
            }
        };
        let Some(stored) = returns else {
            return self.raising_path_call(symbol, parameters, target, error, name);
        };
        let declared = if stored == "i1" { "zeroext i1" } else { stored };
        self.declare(
            symbol,
            &format!(
                "declare {declared} @{symbol}({}) nounwind",
                parameter_types(&parameters)
            ),
        );
        let result = self.emit(&format!(
            "call {declared} @{symbol}({})",
            parameters.join(", ")
        ));
        let (address, _) = self.place(destination)?;
        self.line(&format!("store {stored} {result}, ptr {address}"));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// Writes the call of the runtime function `symbol`, which can fail, with `parameters` and
    /// then the address of the error edge's local, and branches on the flag it returns: to the
    /// edge's block when it failed, to `target` otherwise. `name` names the method in the error
    /// for a call that has no edge.
    fn raising_path_call(
        &mut self,
        symbol: &str,
        mut parameters: Vec<String>,
        target: BlockId,
        error: Option<&ErrorEdge>,
        name: &str,
    ) -> Result<(), Unsupported> {
        let edge = error.ok_or_else(|| Unsupported::new(format!("`{name}` with no error edge")))?;
        let (address, _) = self.place(&edge.error.into())?;
        parameters.push(format!("ptr {address}"));
        self.declare(
            symbol,
            &format!(
                "declare zeroext i1 @{symbol}({}) nounwind",
                parameter_types(&parameters)
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
}

/// The types of `parameters`, each written `type operand`, for a declaration: `ptr, i64`.
fn parameter_types(parameters: &[String]) -> String {
    let types: Vec<&str> = parameters
        .iter()
        .map(|parameter| parameter.rsplit_once(' ').map_or("", |(ty, _)| ty))
        .collect();
    types.join(", ")
}
