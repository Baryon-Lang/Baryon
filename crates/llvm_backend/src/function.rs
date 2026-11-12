//! Writes one body as one LLVM function: its signature, a stack slot per local, each block's
//! statements and terminator, and a block of its own for each check that can fail.

use crate::Unsupported;
use crate::constants::Constants;
use crate::debug::UNWIND_TABLE;
use crate::literal::{global, wrap};
use crate::ty::{Repr, Types};
use mir::{BlockId, Body, LocalKind, Statement, StatementKind, Terminator, TerminatorKind};
use source::Span;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use types::{Item, Mode, Type, TypeKind};

mod places;

pub(crate) use places::Destination;
mod ssa;

/// How a parameter arrives, which decides where its local lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Passing {
    /// Not a parameter, or a parameter passed by value: the local is a stack slot.
    Slot,
    /// A borrow passed as a pointer: the local is the caller's memory, at the parameter.
    Pointer,
}

/// The state of writing one function.
pub(crate) struct Function<'f, 'a, 'db> {
    /// How to represent each type.
    pub(crate) types: Types<'a, 'db>,
    /// The symbol of each function instance called.
    pub(crate) symbol: &'f dyn Fn(&Item<'db>, &[Type<'db>]) -> String,
    /// The body being written.
    pub(crate) body: &'f Body<'db>,
    /// The error types the function can raise, in its `Raises` order; empty when it cannot fail.
    pub(crate) errors: &'f [Type<'db>],
    /// How each local's parameter arrives, by local.
    passing: Vec<Passing>,
    /// The instructions written so far, after the entry block.
    pub(crate) code: String,
    /// The number of the next temporary value, `%t0`, `%t1`, ….
    next_value: u32,
    /// The number of the next block made inside a statement, `%c0`, `%c1`, ….
    next_block: u32,
    /// The module's message and location constants, which a failed check points to.
    pub(crate) constants: &'f mut Constants<'a>,
    /// Where the statement or terminator being written comes from, for a failed check's
    /// location.
    pub(crate) span: Span,
    /// The blocks failed checks jump to, written after the body's own blocks.
    pub(crate) failures: String,
    /// The number of the next failure block, `%p0`, `%p1`, ….
    pub(crate) next_failure: u32,
    /// The declaration of each function or intrinsic called, by symbol.
    pub(crate) calls: BTreeMap<String, String>,
    /// The function's debug subprogram, `!N`, when the module has debug information: each
    /// instruction then carries the location of the statement it comes from.
    pub(crate) scope: Option<String>,
    /// The location every instruction gets in place of its statement's, for a function written
    /// without a body, such as the C `main`.
    pub(crate) fixed_location: Option<String>,
    /// The `!dbg` location last written, with the span it is for. Every instruction of a
    /// statement carries the same one, so it is worked out once per statement rather than once
    /// per instruction.
    location: Option<(Span, String)>,
    /// The function's symbol, which names the functions written for its `prange` loops.
    pub(crate) name: String,
    /// The functions written alongside this one, such as a `prange` loop's chunk function, each
    /// a whole definition.
    pub(crate) helpers: Vec<String>,
    /// Stack slots instructions need beyond the locals', such as the sign `lgamma_r` writes or
    /// a view's header, written in the entry block so an instruction inside a loop does not
    /// grow the stack each time.
    pub(crate) slots: String,
    /// Which locals are registers instead of stack slots, and their values (roadmap 2.7).
    ssa: ssa::Ssa,
}

impl<'f, 'a, 'db> Function<'f, 'a, 'db> {
    /// The state for writing `body`, which can raise `errors`, with nothing written yet.
    pub(crate) fn new(
        types: Types<'a, 'db>,
        symbol: &'f dyn Fn(&Item<'db>, &[Type<'db>]) -> String,
        constants: &'f mut Constants<'a>,
        body: &'f Body<'db>,
        errors: &'f [Type<'db>],
    ) -> Self {
        Self {
            types,
            symbol,
            body,
            errors,
            passing: vec![Passing::Slot; body.locals.len()],
            code: String::new(),
            next_value: 0,
            next_block: 0,
            constants,
            span: body.span,
            failures: String::new(),
            next_failure: 0,
            calls: BTreeMap::new(),
            scope: None,
            fixed_location: None,
            location: None,
            name: String::new(),
            helpers: Vec::new(),
            slots: String::new(),
            ssa: ssa::Ssa::default(),
        }
    }

    /// Writes `body` as the function `name`, and returns its text, with the functions written
    /// for its `prange` loops after it, and the declarations of what it calls. An `internal`
    /// function is visible only inside the module, as an outlined `prange` body is.
    pub(crate) fn write(
        types: Types<'a, 'db>,
        symbol: &'f dyn Fn(&Item<'db>, &[Type<'db>]) -> String,
        constants: &'f mut Constants<'a>,
        name: &str,
        body: &'f Body<'db>,
        errors: &'f [Type<'db>],
        internal: bool,
    ) -> Result<(String, BTreeMap<String, String>), Unsupported> {
        let mut demoted = std::collections::BTreeSet::new();
        loop {
            let mark = constants.mark();
            let mut function = Function::new(types, symbol, &mut *constants, body, errors);
            function.ssa = function.candidates(&demoted)?;
            let written = function.write_body(name, internal)?;
            if function.ssa.escaped.is_empty() {
                return Ok((written, function.calls));
            }
            // A promoted local's address was needed: write the function again with it, and
            // every other such local, in a stack slot.
            demoted.extend(function.ssa.escaped.iter().copied());
            constants.roll_back(mark);
        }
    }

    /// The locals to promote to registers (the `ssa` module): every number or `bool` that is
    /// not passed by pointer and not in `demoted`, with its LLVM type, and each promoted
    /// parameter's argument.
    fn candidates(
        &self,
        demoted: &std::collections::BTreeSet<usize>,
    ) -> Result<ssa::Ssa, Unsupported> {
        let mut promoted = HashMap::new();
        let mut arguments = HashMap::new();
        for (index, local) in self.body.locals.iter().enumerate() {
            if demoted.contains(&index) || self.types.number(local.ty).is_none() {
                continue;
            }
            let Repr::Value(ty) = self.types.repr(local.ty)? else {
                continue;
            };
            if let LocalKind::Parameter { mode, .. } = &local.kind {
                if *mode == Mode::InOut {
                    continue;
                }
                arguments.insert(index, format!("%a{index}"));
            }
            promoted.insert(index, ty);
        }
        Ok(ssa::Ssa::new(self.body.locals.len(), promoted, arguments))
    }

    /// Writes the body as the function `name`, `internal` or not, with the locals in
    /// `self.ssa` promoted, and returns its text with its helpers after it.
    fn write_body(&mut self, name: &str, internal: bool) -> Result<String, Unsupported> {
        let body = self.body;
        name.clone_into(&mut self.name);
        self.scope = self.constants.subprogram(name, body.span);
        let (mut signature, entry) = self.prologue(name)?;
        self.ssa.enter();
        if internal {
            signature = signature.replacen("define ", "define internal ", 1);
        }
        if let Some(scope) = &self.scope {
            let _ = write!(signature, " {UNWIND_TABLE} !dbg {scope}");
        }
        // The loop metadata of each `simd` loop, by its header block, made at its first
        // back edge.
        let mut loops: BTreeMap<BlockId, String> = BTreeMap::new();
        for (index, block) in body.blocks.iter().enumerate() {
            let id = BlockId::new(index);
            let label = block_label(id);
            let _ = writeln!(self.code, "{label}:");
            let marker = self.ssa.start_block(index, &label);
            self.code.push_str(&marker);
            for statement in &block.statements {
                self.span = statement.span;
                self.statement(statement)?;
            }
            self.span = block.terminator.span;
            match block.terminator.kind {
                // A jump back to a `simd` loop's header, from a block after it, is the loop's
                // back edge, where LLVM reads the request to vectorize.
                TerminatorKind::Goto(header)
                    if header <= id && body.vectorized.contains(&header) =>
                {
                    let metadata = loops
                        .entry(header)
                        .or_insert_with(|| self.constants.vectorize());
                    let metadata = metadata.clone();
                    self.line(&format!(
                        "br label %{}, !llvm.loop {metadata}",
                        block_label(header)
                    ));
                }
                _ => self.terminator(&block.terminator)?,
            }
        }
        // The failure blocks pass values to the panic they report, so they are rewritten too.
        self.code.push_str(&self.failures);
        let code = self.ssa.finish(&self.code);
        let mut text = format!("{signature} {{\nstart:\n{}{entry}{code}}}\n", self.slots);
        for helper in &self.helpers {
            text.push('\n');
            text.push_str(helper);
        }
        Ok(text)
    }

    /// The `define` line, and the entry block: a slot per local, each by-value parameter stored
    /// in its slot, and a jump to the first block.
    fn prologue(&mut self, name: &str) -> Result<(String, String), Unsupported> {
        let returns = self.returns()?;
        let mut parameters = Vec::new();
        let mut entry = String::new();
        for (index, local) in self.body.locals.iter().enumerate() {
            let Repr::Value(ty) = self.types.repr(local.ty)? else {
                continue;
            };
            let slot = format!("%_{index}");
            let promoted = self.ssa.is_promoted(index);
            let LocalKind::Parameter { mode, .. } = &local.kind else {
                if !promoted {
                    let _ = writeln!(entry, "  {slot} = alloca {ty}");
                }
                continue;
            };
            let argument = format!("%a{index}");
            match mode {
                Mode::Borrow if !self.types.by_value(local.ty) => {
                    let pointee = self.types.pointee(local.ty);
                    parameters.push(format!("ptr noalias readonly{pointee} {argument}"));
                    self.passing[index] = Passing::Pointer;
                }
                Mode::InOut => {
                    let pointee = self.types.pointee(local.ty);
                    parameters.push(format!("ptr noalias{pointee} {argument}"));
                    self.passing[index] = Passing::Pointer;
                }
                Mode::Borrow | Mode::Owned => {
                    parameters.push(format!("{ty} {argument}"));
                    if !promoted {
                        let _ = writeln!(entry, "  {slot} = alloca {ty}");
                    }
                }
            }
        }
        for (index, local) in self.body.locals.iter().enumerate() {
            if let (LocalKind::Parameter { .. }, Passing::Slot, Repr::Value(ty), false) = (
                &local.kind,
                self.passing[index],
                self.types.repr(local.ty)?,
                self.ssa.is_promoted(index),
            ) {
                let _ = writeln!(entry, "  store {ty} %a{index}, ptr %_{index}");
            }
        }
        if !self.errors.is_empty() {
            parameters.push("ptr noalias %error".to_owned());
        }
        entry.push_str("  br label %bb0\n");
        let signature = format!(
            "define {returns} {}({})",
            global(name),
            parameters.join(", ")
        );
        Ok((signature, entry))
    }

    /// Writes one statement.
    fn statement(&mut self, statement: &Statement<'db>) -> Result<(), Unsupported> {
        match &statement.kind {
            StatementKind::Assign(place, rvalue)
                if place.projections.is_empty() && self.ssa.is_promoted(place.local.index()) =>
            {
                let ty = self.body.locals[place.local.index()].ty;
                if let Some(value) = self.rvalue(rvalue, ty)? {
                    self.ssa.assign(place.local.index(), value);
                }
                Ok(())
            }
            StatementKind::Assign(place, rvalue) => {
                let (address, ty) = self.place(place)?;
                // A union is written in parts: its tag, then its member in the payload.
                if let (mir::Rvalue::Aggregate(_, operands), TypeKind::Union(_)) =
                    (rvalue, ty.kind(self.types.db))
                    && let [operand] = operands.as_slice()
                {
                    return self.store_union(&address, ty, operand);
                }
                let Repr::Value(llvm) = self.types.repr(ty)? else {
                    return Ok(());
                };
                if let Some(value) = self.rvalue(rvalue, ty)? {
                    self.line(&format!("store {llvm} {value}, ptr {address}"));
                }
                Ok(())
            }
            StatementKind::Drop(place) => self.drop_place(place),
            StatementKind::Accumulate {
                place,
                operator,
                value,
            } => self.accumulate(place, *operator, value),
        }
    }

    /// Writes one terminator.
    fn terminator(&mut self, terminator: &Terminator<'db>) -> Result<(), Unsupported> {
        match &terminator.kind {
            TerminatorKind::Goto(target) => {
                self.line(&format!("br label %{}", block_label(*target)));
            }
            TerminatorKind::Branch {
                condition,
                then,
                otherwise,
            } => {
                let (condition, _) = self.value(condition)?;
                self.line(&format!(
                    "br i1 {condition}, label %{}, label %{}",
                    block_label(*then),
                    block_label(*otherwise)
                ));
            }
            TerminatorKind::Switch {
                value,
                cases,
                otherwise,
            } => {
                let (value, ty) = self.value(value)?;
                let llvm = self.types.value(ty)?;
                let bits = self.integer_bits(ty)?;
                let cases: Vec<String> = cases
                    .iter()
                    .map(|(case, target)| {
                        format!(
                            "{llvm} {}, label %{}",
                            wrap(*case, bits),
                            block_label(*target)
                        )
                    })
                    .collect();
                self.line(&format!(
                    "switch {llvm} {value}, label %{} [ {} ]",
                    block_label(*otherwise),
                    cases.join(" ")
                ));
            }
            TerminatorKind::Return => self.return_value()?,
            TerminatorKind::Call {
                callee,
                arguments,
                destination,
                target,
                error,
            } => self.call(callee, arguments, destination, *target, error.as_ref())?,
            TerminatorKind::Assert {
                condition,
                check,
                target,
            } => {
                if self.constants.wraps && check.is_overflow() {
                    // A release build keeps the wrapped result and drops the check.
                    self.line(&format!("br label %{}", block_label(*target)));
                    return Ok(());
                }
                let (condition, _) = self.value(condition)?;
                let failure = self.failure(check)?;
                let failed = self.failure_block(&failure);
                self.line(&format!(
                    "br i1 {condition}, label %{}, label %{failed}",
                    block_label(*target)
                ));
            }
            TerminatorKind::Raise(error) => self.raise(error)?,
            TerminatorKind::Unreachable => self.line("unreachable"),
            TerminatorKind::ParallelFor {
                body,
                count,
                arguments,
                reductions,
                accumulations,
                target,
            } => self.parallel_for(*body, count, arguments, reductions, accumulations, *target)?,
        }
        Ok(())
    }
    /// Whether `local` is a register instead of a stack slot (the `ssa` module).
    pub(crate) fn promoted(&self, local: usize) -> bool {
        self.ssa.is_promoted(local)
    }

    /// The value the promoted `local` holds here (the `ssa` module).
    pub(crate) fn read(&mut self, local: usize) -> String {
        self.ssa.read(local)
    }

    /// A new temporary value's name.
    pub(crate) fn fresh(&mut self) -> String {
        let name = format!("%t{}", self.next_value);
        self.next_value += 1;
        name
    }

    /// A new block label for control flow inside a statement.
    pub(crate) fn fresh_block(&mut self) -> String {
        let name = format!("c{}", self.next_block);
        self.next_block += 1;
        name
    }

    /// `instruction` with the `!dbg` location of the statement being written, when the module
    /// has debug information.
    pub(crate) fn located(&mut self, instruction: &str) -> String {
        match self.location() {
            Some(location) => format!("{instruction}, !dbg {location}"),
            None => instruction.to_owned(),
        }
    }

    /// The `!dbg` location of the statement being written, when the module has debug
    /// information.
    fn location(&mut self) -> Option<&str> {
        if let Some(location) = &self.fixed_location {
            return Some(location);
        }
        let scope = self.scope.as_ref()?;
        if self
            .location
            .as_ref()
            .is_none_or(|(span, _)| *span != self.span)
        {
            let location = self.constants.debug_location(scope, self.span);
            self.location = Some((self.span, location));
        }
        self.location
            .as_ref()
            .map(|(_, location)| location.as_str())
    }

    /// Writes one instruction.
    pub(crate) fn line(&mut self, instruction: &str) {
        self.ssa.record(instruction);
        self.code.push_str("  ");
        self.code.push_str(instruction);
        self.end_line();
    }

    /// Writes `result = instruction`, an instruction that gives a value and so never branches,
    /// straight into the function's text.
    pub(crate) fn line_with_result(&mut self, result: &str, instruction: &str) {
        let _ = write!(self.code, "  {result} = {instruction}");
        self.end_line();
    }

    /// Ends the instruction just written with its `!dbg` location, when the module has debug
    /// information, and a line end.
    fn end_line(&mut self) {
        if self.location().is_some() {
            // Read again from the fields, since `location` borrows all of `self`.
            let location = self.fixed_location.as_deref().or_else(|| {
                self.location
                    .as_ref()
                    .map(|(_, location)| location.as_str())
            });
            if let Some(location) = location {
                let _ = write!(self.code, ", !dbg {location}");
            }
        }
        self.code.push('\n');
    }

    /// Starts the block `label`.
    pub(crate) fn line_label(&mut self, label: &str) {
        self.ssa.start_label(label);
        let _ = writeln!(self.code, "{label}:");
    }

    /// Records that the function calls `symbol`, declared as `declaration`.
    pub(crate) fn declare(&mut self, symbol: &str, declaration: &str) {
        self.calls
            .entry(symbol.to_owned())
            .or_insert_with(|| declaration.to_owned());
    }
}

/// The label of block `block`: `bb0`, `bb1`, ….
pub(crate) fn block_label(block: BlockId) -> String {
    format!("bb{}", block.index())
}
