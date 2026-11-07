//! The program's entry point: a C `main` that calls the package's `main` function and turns how
//! it ended into the exit status.
//!
//! A `main` that cannot fail returns 0. A `main` that can fail (the reference, "Errors") is
//! passed an error value to write into, like any fallible call; when it fails, the C `main`
//! reads which error it holds and prints it to standard error as `error: ParseError(position=-3)`,
//! the error's type, then each field by name with its value as `print` writes it, then returns
//! 1. Status 1 differs from a panic's 101, so a script can tell a reported failure from a bug.

use crate::Unsupported;
use crate::function::{Function, global};
use crate::print::STDERR;
use mir::Body;
use std::collections::BTreeMap;
use types::{Type, TypeKind};

/// An error type `main` can raise, with its fields' names in declaration order, for the report
/// of an error that ends the program.
pub(crate) type Reported<'db> = (Type<'db>, Vec<String>);

impl<'db> Function<'_, '_, 'db> {
    /// The C `main` that runs the function `symbol`, which raises `errors`, as text, with the
    /// declarations of what it calls.
    pub(crate) fn entry(
        mut self,
        symbol: &str,
        errors: &[Reported<'db>],
    ) -> Result<(String, BTreeMap<String, String>), Unsupported> {
        let callee = global(symbol);
        let mut signature = String::from("define i32 @main()");
        if let Some(scope) = self.constants.entry_subprogram() {
            signature = format!("{signature} !dbg {scope}");
            self.fixed_location = Some(format!("!DILocation(line: 0, scope: {scope})"));
        }
        if errors.is_empty() {
            self.line(&format!("call void {callee}()"));
            self.line("ret i32 0");
            let text = format!("{signature} {{\nstart:\n{}}}\n", self.code);
            return Ok((text, self.calls));
        }
        let none = Type::new(self.types.db, TypeKind::None);
        let fallible = Type::new(
            self.types.db,
            TypeKind::Fallible {
                value: none,
                errors: errors.iter().map(|(ty, _)| *ty).collect(),
            },
        );
        let llvm = self.types.value(fallible)?;
        self.line(&format!("%error = alloca {llvm}"));
        let failed = self.emit(&format!("call i1 {callee}(ptr %error)"));
        self.line(&format!("br i1 {failed}, label %failed, label %done"));
        self.line_label("done");
        self.line("ret i32 0");
        self.line_label("failed");
        let index = self.error_tag("%error", fallible)?;
        let cases: Vec<String> = (1..errors.len())
            .map(|position| format!("i8 {position}, label %e{position}"))
            .collect();
        self.line(&format!(
            "switch i8 {index}, label %e0 [{}]",
            cases.join(" ")
        ));
        for (position, (ty, fields)) in errors.iter().enumerate() {
            self.line_label(&format!("e{position}"));
            let position = u32::try_from(position)
                .map_err(|_| Unsupported::new("a `main` that raises this many errors"))?;
            let (payload, _) = self.error_payload("%error", fallible, position)?;
            self.report(&payload, *ty, fields)?;
            self.line("ret i32 1");
        }
        let text = format!("{signature} {{\nstart:\n{}}}\n", self.code);
        Ok((text, self.calls))
    }

    /// Prints the error of type `ty` at `address`, whose fields are named `fields`, to standard
    /// error, as `error: Name(first=1, second=2.5)` and a line end.
    fn report(
        &mut self,
        address: &str,
        ty: Type<'db>,
        fields: &[String],
    ) -> Result<(), Unsupported> {
        let name = ty.display(self.types.db).to_string();
        self.print_text(STDERR, format!("error: {name}(").as_bytes());
        let llvm = self.types.value(ty)?;
        for (position, field) in fields.iter().enumerate() {
            let separator = if position == 0 { "" } else { ", " };
            self.print_text(STDERR, format!("{separator}{field}=").as_bytes());
            let position = u32::try_from(position)
                .map_err(|_| Unsupported::new("an error with this many fields"))?;
            let field_ty = self.field(ty, position)?;
            let field_address = self.emit(&format!(
                "getelementptr inbounds {llvm}, ptr {address}, i32 0, i32 {position}"
            ));
            let value = self
                .load(&field_address, field_ty)?
                .ok_or_else(|| Unsupported::new("printing `None`"))?;
            self.print_value(STDERR, &value, field_ty)?;
        }
        self.print_text(STDERR, b")\n");
        Ok(())
    }
}

/// An empty body, which the entry's writer needs to exist but never reads.
pub(crate) fn no_body(db: &dyn database::Db) -> Body<'_> {
    let span = source::Span::new(source::FileId::from_index(0), 0, 0);
    Body::new(Type::new(db, TypeKind::None), span)
}
