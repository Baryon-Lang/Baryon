//! Calls of a value rather than a named function: a `Callable` parameter or a local holding a
//! function (decision 0026), and a struct with `__call__` (decision 0058), whether the value is
//! a name, a field or the result of another call. Anything else called is an error that names
//! the expression and its type.

use super::calls::Arguments;
use super::{Body, relations};
use syntax::NodeId;
use types::{Callee, Mode, Parameter, Signature, Type, TypeKind};

impl<'db> Body<'db> {
    /// A call to a value, such as a `Callable` parameter or a struct with `__call__`. `node` is
    /// the call and `function` the expression called.
    pub(super) fn call_value(
        &mut self,
        (node, function): (NodeId, NodeId),
        callee: &str,
        ty: Type<'db>,
        arguments: &Arguments,
    ) -> Type<'db> {
        if let TypeKind::Function { callable, .. } = ty.kind(self.db) {
            return self.call_value((node, function), callee, *callable, arguments);
        }
        match ty.kind(self.db) {
            TypeKind::Callable {
                parameters,
                returns,
            } => {
                let signature = Signature {
                    type_parameters: Vec::new(),
                    parameters: parameters
                        .iter()
                        .enumerate()
                        .map(|(index, &ty)| Parameter {
                            name: format!("argument {}", index + 1),
                            ty,
                            mode: Mode::Borrow,
                            default: false,
                        })
                        .collect(),
                    returns: *returns,
                };
                self.check_call(node, callee, &signature, None, Callee::Value, arguments)
            }
            TypeKind::Struct(_) => self.called_struct((node, function), callee, ty, arguments),
            _ if relations::is_open(self.db, ty) || !self.is_value_scalar(ty) => {
                self.untyped(arguments)
            }
            _ => {
                self.untyped(arguments);
                let message = format!(
                    "`{callee}` has type `{}`, which cannot be called",
                    self.show(ty)
                );
                self.lowerer.error(node, &message, "not a function", None)
            }
        }
    }

    /// Whether `ty` is a number, `bool`, `str` or `None`.
    fn is_value_scalar(&self, ty: Type<'db>) -> bool {
        matches!(
            ty.kind(self.db),
            TypeKind::Primitive(_) | TypeKind::Str | TypeKind::None
        )
    }
}
