//! Drops of a struct whose class has `__del__` (decision 0063): the drop calls the method on the
//! value first, then drops its fields as any struct's.
//!
//! The method takes the value by pointer, as any `InOut` argument, and nothing else unless it
//! can fail. A method that can fail also takes a pointer to an error slot in the entry block and
//! returns a failure flag; a failure prints the error as an uncaught error is printed, then
//! panics, since a drop cannot raise:
//!
//! ```text
//! error: OSError(kind=ErrorKind.PERMISSION, errno=13, ...)
//! panic: `GzipFile.__del__` raised the error above when its value was dropped
//! ```

use crate::function::Function;
use crate::panic::Failure;
use crate::print::STDERR;
use crate::ty::Repr;
use crate::{Module, Unsupported};
use types::{Nominal, Type, TypeKind};

/// The callback a module without [`Module::destructors`](crate::Module::destructors) uses: no
/// struct has `__del__`.
pub(crate) fn none<'db>(_: &Nominal<'db>) -> Option<crate::Destructor<'db>> {
    None
}

impl<'a, 'db> Module<'a, 'db> {
    /// Calls `destructor`'s method, for a struct whose class has `__del__`, whenever a value of
    /// the struct is dropped, before its fields are dropped (decision 0063). Without it, no
    /// struct has one. Call it before [`define`](Self::define).
    ///
    /// Cost: one call per drop of such a struct; a struct without one costs nothing more.
    ///
    /// ```
    /// # #[salsa::db]
    /// # #[derive(Clone, Default)]
    /// # struct Database { storage: salsa::Storage<Self> }
    /// # #[salsa::db]
    /// # impl salsa::Database for Database {}
    /// # #[salsa::db]
    /// # impl database::Db for Database {
    /// #     fn parse(&self, _: database::File) -> &database::ParsedFile { unreachable!() }
    /// #     fn resolve(&self, _: database::Package, _: database::File) -> &database::Resolution {
    /// #         unreachable!()
    /// #     }
    /// # }
    /// let database = Database::default();
    /// let sources = source::SourceMap::new();
    /// let declared = |_: &types::Nominal<'_>| types::Declared::Struct(Vec::new());
    /// let symbol = |_: &types::Item<'_>, _: &[types::Type<'_>]| String::new();
    /// let names = |_: &types::Nominal<'_>| llvm_backend::ClassNames::default();
    /// let destructor = |_: &types::Nominal<'_>| None;
    /// let mut module =
    ///     llvm_backend::Module::new(&database, &sources, &declared, &names, &symbol);
    /// module.destructors(&destructor);
    /// assert!(!module.finish().contains("__del__"));
    /// ```
    pub fn destructors(
        &mut self,
        destructor: &'a dyn Fn(&Nominal<'db>) -> Option<crate::Destructor<'db>>,
    ) {
        self.types.destructor = destructor;
    }
}

impl<'db> Function<'_, '_, 'db> {
    /// Calls the `__del__` of the struct of type `ty` at `address`, when its class has one.
    pub(crate) fn destroy(&mut self, address: &str, ty: Type<'db>) -> Result<(), Unsupported> {
        let TypeKind::Struct(nominal) = ty.kind(self.types.db) else {
            return Ok(());
        };
        let Some(destructor) = (self.types.destructor)(nominal) else {
            return Ok(());
        };
        let callee = crate::literal::global(&destructor.symbol);
        let (mut written, mut parameters) = (Vec::new(), Vec::new());
        if self.types.repr(ty)? != Repr::Empty {
            written.push(format!("ptr {address}"));
            parameters.push("ptr");
        }
        if destructor.errors.is_empty() {
            self.declare(
                &destructor.symbol,
                &format!("declare void {callee}({})", parameters.join(", ")),
            );
            self.line(&format!("call void {callee}({})", written.join(", ")));
            return Ok(());
        }
        let none = Type::new(self.types.db, TypeKind::None);
        let fallible = Type::new(
            self.types.db,
            TypeKind::Fallible {
                value: none,
                errors: destructor.errors.clone(),
            },
        );
        let llvm = self.types.value(fallible)?;
        let error = self.slot(&llvm);
        written.push(format!("ptr {error}"));
        parameters.push("ptr");
        self.declare(
            &destructor.symbol,
            &format!("declare i1 {callee}({})", parameters.join(", ")),
        );
        let failed = self.emit(&format!("call i1 {callee}({})", written.join(", ")));
        let (raised, done) = (self.fresh_block(), self.fresh_block());
        self.line(&format!("br i1 {failed}, label %{raised}, label %{done}"));
        self.line_label(&raised);
        let index = self.error_tag(&error, fallible)?;
        let reports: Vec<String> = destructor
            .errors
            .iter()
            .map(|_| self.fresh_block())
            .collect();
        let cases: Vec<String> = reports
            .iter()
            .enumerate()
            .skip(1)
            .map(|(position, label)| format!("i8 {position}, label %{label}"))
            .collect();
        self.line(&format!(
            "switch i8 {index}, label %{} [{}]",
            reports[0],
            cases.join(" ")
        ));
        let message = format!(
            "`{}.__del__` raised the error above when its value was dropped",
            destructor.class
        );
        let failure = self.failure_block(&Failure::Message(message));
        for (position, (&error_type, label)) in destructor.errors.iter().zip(&reports).enumerate() {
            self.line_label(label);
            let position = u32::try_from(position)
                .map_err(|_| Unsupported::new("a `__del__` that raises this many errors"))?;
            let (payload, _) = self.error_payload(&error, fallible, position)?;
            let value = self
                .load(&payload, error_type)?
                .ok_or_else(|| Unsupported::new("printing an error with no value"))?;
            self.print_text(STDERR, b"error: ");
            self.print_value(STDERR, &value, error_type, crate::print::Form::Repr)?;
            self.print_text(STDERR, b"\n");
            self.line(&format!("br label %{failure}"));
        }
        self.line_label(&done);
        Ok(())
    }

    /// Whether the struct type `ty` has a `__del__`, which makes its drop do something even when
    /// its fields own nothing.
    pub(crate) fn has_destructor(&self, ty: Type<'db>) -> bool {
        matches!(ty.kind(self.types.db), TypeKind::Struct(nominal)
            if (self.types.destructor)(nominal).is_some())
    }
}
