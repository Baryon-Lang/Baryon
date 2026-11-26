//! `method=` of `integrate.solve_ivp`, chosen when compiling (decision 0065 point 2).
//!
//! A method written as a literal, or left out, sends the call to the integrator for that method,
//! so only that integrator is compiled for each right-hand side. An unknown literal, such as
//! `"RK54"`, is a compile error that lists the methods built. A method held in a variable is
//! picked at run time by `_solve_ivp_named`, which raises `ValueError` for an unknown name, so
//! only that form adds `Raises[..., ValueError]` to the call.

use super::Body;
use super::calls::Arguments;
use database::File;
use syntax::NodeId;

/// The methods `solve_ivp` builds, in `SciPy`'s spelling, and the library function of
/// `<std>/integrate_ivp.byn` each one is checked as. RK45 is `solve_ivp` itself.
const METHODS: [(&str, &str); 1] = [("RK45", "solve_ivp")];

impl Body<'_> {
    /// The function a call of the library function `name` of `file` is checked as: for
    /// `solve_ivp`, the integrator its `method=` names when that is a literal, or
    /// `_solve_ivp_named` when it is held in a variable, after reporting an unknown literal; the
    /// function itself for any other call. The method is the fourth argument, by position or as
    /// `method=`, as in `SciPy`.
    pub(super) fn ode_method(
        &mut self,
        file: File,
        name: NodeId,
        arguments: &Arguments,
    ) -> (File, NodeId) {
        if self.library_function("integrate_ivp", "solve_ivp") != Some((file, name)) {
            return (file, name);
        }
        let method = arguments.positional.get(3).copied().or_else(|| {
            arguments
                .keywords
                .iter()
                .find(|&&(keyword, _)| self.tree.text(keyword, self.text) == "method")
                .map(|&(_, value)| value)
        });
        let Some(method) = method else {
            return (file, name);
        };
        let routed = match super::files::literal(self.tree, self.text, method) {
            Some(written) => {
                let found = METHODS.iter().find(|&&(method, _)| method == written);
                if found.is_none() {
                    let names: Vec<String> = METHODS
                        .iter()
                        .map(|(method, _)| format!("\"{method}\""))
                        .collect();
                    let message = format!("`{written}` is not a method of `solve_ivp`");
                    let note = format!("the methods built are {}", names.join(", "));
                    self.lowerer
                        .error(method, &message, "unknown method", Some(&note));
                }
                found.map_or("solve_ivp", |&(_, function)| function)
            }
            None => "_solve_ivp_named",
        };
        self.library_function("integrate_ivp", routed)
            .unwrap_or((file, name))
    }
}
