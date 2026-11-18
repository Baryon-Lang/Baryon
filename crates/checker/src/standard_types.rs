//! A standard module's member written as a type (roadmap 1.9.56). `random.Philox`, the
//! generator the compiler builds in (decision 0028), is the only one: written as a parameter's
//! type, `g: InOut[Philox]`, it is the same type `Philox(seed)` gives. Any other member, or the
//! module itself, is an error where it is written, rather than an unknown type that the backend
//! stops on later:
//!
//! ```text
//! error: `zlib.error` is not a type in Baryon
//! ```

use crate::lower::Lowerer;
use syntax::NodeId;
use types::{StandardType, Type, TypeKind};

impl<'db> Lowerer<'db> {
    /// The type `member` of the standard module `module` means, written at `node`, with the
    /// type arguments of `subscript` if any; `member` is `None` for the module itself.
    pub(crate) fn standard_type(
        &mut self,
        node: NodeId,
        module: &str,
        member: Option<&str>,
        subscript: Option<NodeId>,
    ) -> Type<'db> {
        let Some(member) = member else {
            let message = format!("`{module}` is a module, not a type");
            return self.error(node, &message, "expected a type", None);
        };
        if (module, member) != ("random", "Philox") {
            let message = format!("`{module}.{member}` is not a type in Baryon");
            let hint = format!("the language reference lists the types `{module}` has");
            return self.error(node, &message, "not a type", Some(&hint));
        }
        if subscript.is_some() {
            return self.error(
                node,
                "`Philox` takes no type arguments",
                "remove the brackets",
                None,
            );
        }
        Type::new(self.db, TypeKind::Standard(StandardType::Philox))
    }
}
