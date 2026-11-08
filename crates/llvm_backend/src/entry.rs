//! The program's entry point: a C `main` that calls the package's `main` function and turns how
//! it ended into the exit status. A test program's C `main` does the same for the one `@test`
//! function its command-line argument chooses.
//!
//! A `main` that cannot fail returns 0. A `main` that can fail (the reference, "Errors") is
//! passed an error value to write into, like any fallible call; when it fails, the C `main`
//! reads which error it holds and prints it to standard error as `error: ParseError(position=-3)`,
//! the error's type, then each field by name with its value as `print` writes it, a `str` quoted
//! as Python's `repr` writes it, then returns 1. Status 1 differs from a panic's 101, so a
//! script can tell a reported failure from a bug.

use crate::Unsupported;
use crate::debug::UNWIND_TABLE;
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
        let signature = self.entry_signature("define i32 @main()");
        self.run(symbol, errors, "")?;
        let text = format!("{signature} {{\nstart:\n{}}}\n", self.code);
        Ok((text, self.calls))
    }

    /// The C `main` of a test program, as text, with the declarations of what it calls: it runs
    /// the one test of `tests` whose position its first command-line argument names, counting
    /// from 0, and exits as [`entry`](Self::entry)'s `main` would for that function. Without an
    /// argument, or with one that names no test, it returns 2.
    ///
    /// One test per process keeps a panic, which ends the process, from ending the other tests.
    pub(crate) fn tests(
        mut self,
        tests: &[crate::Test<'_, 'db>],
    ) -> Result<(String, BTreeMap<String, String>), Unsupported> {
        let signature = self.entry_signature("define i32 @main(i32 %count, ptr %arguments)");
        self.declare("atoi", "declare i32 @atoi(ptr)");
        let given = self.emit("icmp sge i32 %count, 2");
        self.line(&format!("br i1 {given}, label %chosen, label %unknown"));
        self.line_label("chosen");
        let slot = self.emit("getelementptr inbounds ptr, ptr %arguments, i64 1");
        let text = self.emit(&format!("load ptr, ptr {slot}"));
        let position = self.emit(&format!("call i32 @atoi(ptr {text})"));
        let cases: Vec<String> = (0..tests.len())
            .map(|test| format!("i32 {test}, label %test{test}"))
            .collect();
        self.line(&format!(
            "switch i32 {position}, label %unknown [{}]",
            cases.join(" ")
        ));
        for (test, (symbol, errors)) in tests.iter().enumerate() {
            self.line_label(&format!("test{test}"));
            self.run(symbol, errors, &format!("test{test}."))?;
        }
        self.line_label("unknown");
        self.line("ret i32 2");
        let text = format!("{signature} {{\nstart:\n{}}}\n", self.code);
        Ok((text, self.calls))
    }

    /// `signature` with the artificial subprogram of a C `main` added when line tables are on,
    /// which also becomes the location of every instruction written after.
    fn entry_signature(&mut self, signature: &str) -> String {
        match self.constants.entry_subprogram() {
            Some(scope) => {
                self.fixed_location = Some(format!("!DILocation(line: 0, scope: {scope})"));
                format!("{signature} {UNWIND_TABLE} !dbg {scope}")
            }
            None => signature.to_owned(),
        }
    }

    /// Calls the function `symbol`, which raises `errors`, then returns the exit status from the
    /// C `main`: 0, or 1 after reporting the error it raised. `prefix` starts every label and
    /// local name written, so several runs fit in one function.
    fn run(
        &mut self,
        symbol: &str,
        errors: &[Reported<'db>],
        prefix: &str,
    ) -> Result<(), Unsupported> {
        let callee = global(symbol);
        if errors.is_empty() {
            self.line(&format!("call void {callee}()"));
            self.line("ret i32 0");
            return Ok(());
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
        let error = format!("%{prefix}error");
        self.line(&format!("{error} = alloca {llvm}"));
        let failed = self.emit(&format!("call i1 {callee}(ptr {error})"));
        self.line(&format!(
            "br i1 {failed}, label %{prefix}failed, label %{prefix}done"
        ));
        self.line_label(&format!("{prefix}done"));
        self.line("ret i32 0");
        self.line_label(&format!("{prefix}failed"));
        let index = self.error_tag(&error, fallible)?;
        let cases: Vec<String> = (1..errors.len())
            .map(|position| format!("i8 {position}, label %{prefix}e{position}"))
            .collect();
        self.line(&format!(
            "switch i8 {index}, label %{prefix}e0 [{}]",
            cases.join(" ")
        ));
        for (position, (ty, fields)) in errors.iter().enumerate() {
            self.line_label(&format!("{prefix}e{position}"));
            let position = u32::try_from(position)
                .map_err(|_| Unsupported::new("a function that raises this many errors"))?;
            let (payload, _) = self.error_payload(&error, fallible, position)?;
            self.report(&payload, *ty, fields)?;
            self.line("ret i32 1");
        }
        Ok(())
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
            if matches!(field_ty.kind(self.types.db), TypeKind::Str) {
                self.print_quoted(STDERR, &value);
            } else {
                self.print_value(STDERR, &value, field_ty)?;
            }
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
