//! Writes one body as one LLVM function: its signature, a stack slot per local, each block's
//! statements and terminator, and a block of its own for each check that can fail.

use crate::Unsupported;
use crate::debug::UNWIND_TABLE;
use crate::panic::Constants;
use crate::ty::{Repr, Types};
use mir::{
    BlockId, Body, ConstantValue, LocalKind, Operand, Place, Projection, Statement, StatementKind,
    Terminator, TerminatorKind,
};
use source::Span;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use types::{Item, Mode, Type, TypeKind};

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
        }
    }

    /// Writes `body` as the function `name`, and returns its text and the declarations of what
    /// it calls.
    pub(crate) fn write(
        types: Types<'a, 'db>,
        symbol: &'f dyn Fn(&Item<'db>, &[Type<'db>]) -> String,
        constants: &'f mut Constants<'a>,
        name: &str,
        body: &'f Body<'db>,
        errors: &'f [Type<'db>],
    ) -> Result<(String, BTreeMap<String, String>), Unsupported> {
        let mut function = Self::new(types, symbol, constants, body, errors);
        function.scope = function.constants.subprogram(name, body.span);
        let (mut signature, entry) = function.prologue(name)?;
        if let Some(scope) = &function.scope {
            let _ = write!(signature, " {UNWIND_TABLE} !dbg {scope}");
        }
        for (index, block) in body.blocks.iter().enumerate() {
            function.line_label(&block_label(BlockId::new(index)));
            for statement in &block.statements {
                function.span = statement.span;
                function.statement(statement)?;
            }
            function.span = block.terminator.span;
            function.terminator(&block.terminator)?;
        }
        let text = format!(
            "{signature} {{\nstart:\n{entry}{}{}}}\n",
            function.code, function.failures
        );
        Ok((text, function.calls))
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
            let LocalKind::Parameter { mode, .. } = &local.kind else {
                let _ = writeln!(entry, "  {slot} = alloca {ty}");
                continue;
            };
            let argument = format!("%a{index}");
            match mode {
                Mode::Borrow if !self.types.by_value(local.ty) => {
                    parameters.push(format!("ptr noalias readonly {argument}"));
                    self.passing[index] = Passing::Pointer;
                }
                Mode::InOut => {
                    parameters.push(format!("ptr noalias {argument}"));
                    self.passing[index] = Passing::Pointer;
                }
                Mode::Borrow | Mode::Owned => {
                    parameters.push(format!("{ty} {argument}"));
                    let _ = writeln!(entry, "  {slot} = alloca {ty}");
                }
            }
        }
        for (index, local) in self.body.locals.iter().enumerate() {
            if let (LocalKind::Parameter { .. }, Passing::Slot, Repr::Value(ty)) =
                (&local.kind, self.passing[index], self.types.repr(local.ty)?)
            {
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
            StatementKind::Assign(place, rvalue) => {
                let (address, ty) = self.place(place)?;
                let Repr::Value(llvm) = self.types.repr(ty)? else {
                    return Ok(());
                };
                if let Some(value) = self.rvalue(rvalue, ty)? {
                    self.line(&format!("store {llvm} {value}, ptr {address}"));
                }
                Ok(())
            }
            StatementKind::Drop(place) => self.drop_place(place),
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
            TerminatorKind::ParallelFor { .. } => {
                return Err(Unsupported::new("`prange` loops"));
            }
        }
        Ok(())
    }

    /// The address of `place` and the type of the value there.
    pub(crate) fn place(&mut self, place: &Place) -> Result<(String, Type<'db>), Unsupported> {
        let local = place.local.index();
        let mut ty = self.body.locals[local].ty;
        let mut address = match self.passing[local] {
            Passing::Slot => format!("%_{local}"),
            Passing::Pointer => format!("%a{local}"),
        };
        for projection in &place.projections {
            let position = match projection {
                Projection::Field(position) => position,
                Projection::Index(indices) => {
                    (address, ty) = self.element(&address, ty, indices)?;
                    continue;
                }
                Projection::Error(position) => {
                    (address, ty) = self.error_payload(&address, ty, *position)?;
                    continue;
                }
                Projection::Value => {
                    let TypeKind::Optional(value) = ty.kind(self.types.db) else {
                        return Err(self.types.unsupported(ty));
                    };
                    let container = self.types.value(ty)?;
                    address = self.emit(&format!(
                        "getelementptr inbounds {container}, ptr {address}, i32 0, i32 0"
                    ));
                    ty = *value;
                    continue;
                }
            };
            let container = self.types.value(ty)?;
            ty = self.field(ty, *position)?;
            let field = self.fresh();
            self.line(&format!(
                "{field} = getelementptr inbounds {container}, ptr {address}, i32 0, i32 {position}"
            ));
            address = field;
        }
        Ok((address, ty))
    }

    /// The type of item or field `position` of the tuple or struct type `ty`.
    pub(crate) fn field(&self, ty: Type<'db>, position: u32) -> Result<Type<'db>, Unsupported> {
        let items = match ty.kind(self.types.db) {
            TypeKind::Tuple(items) => items.clone(),
            TypeKind::Struct(nominal) => match (self.types.declared)(nominal) {
                types::Declared::Struct(fields) => fields,
                types::Declared::Enum { .. } => Vec::new(),
            },
            _ => Vec::new(),
        };
        items
            .get(position as usize)
            .copied()
            .ok_or_else(|| self.types.unsupported(ty))
    }

    /// The value of `operand` and its type, or `None` for a `None` value, which has no value.
    pub(crate) fn operand(
        &mut self,
        operand: &Operand<'db>,
    ) -> Result<Option<(String, Type<'db>)>, Unsupported> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => {
                let (address, ty) = self.place(place)?;
                Ok(self.load(&address, ty)?.map(|value| (value, ty)))
            }
            Operand::Constant(constant) => {
                let value = match &constant.value {
                    // An absent `T | None` is all zeros, its flag false; `None` itself has no value.
                    ConstantValue::None
                        if matches!(constant.ty.kind(self.types.db), TypeKind::Optional(_)) =>
                    {
                        "zeroinitializer".to_owned()
                    }
                    ConstantValue::None => return Ok(None),
                    ConstantValue::Bool(value) => value.to_string(),
                    ConstantValue::Integer(value) => {
                        let bits = self.integer_bits(constant.ty)?;
                        wrap(*value, bits).to_string()
                    }
                    ConstantValue::Float(bits) => float(
                        f64::from_bits(*bits),
                        self.types.value(constant.ty)? == "float",
                    ),
                    ConstantValue::Str(text) => self.text_constant(text),
                    ConstantValue::Complex { .. } => {
                        return Err(self.types.unsupported(constant.ty));
                    }
                };
                Ok(Some((value, constant.ty)))
            }
        }
    }

    /// The value of `operand`, which has a type other than `None`.
    pub(crate) fn value(
        &mut self,
        operand: &Operand<'db>,
    ) -> Result<(String, Type<'db>), Unsupported> {
        self.operand(operand)?
            .ok_or_else(|| Unsupported::new("`None` where a value is needed"))
    }

    /// Loads the value of type `ty` at `address`; `None` for a `None` value.
    pub(crate) fn load(
        &mut self,
        address: &str,
        ty: Type<'db>,
    ) -> Result<Option<String>, Unsupported> {
        let Repr::Value(llvm) = self.types.repr(ty)? else {
            return Ok(None);
        };
        let value = self.fresh();
        self.line(&format!("{value} = load {llvm}, ptr {address}"));
        Ok(Some(value))
    }

    /// The width of the integer, `bool` or enum type `ty`.
    fn integer_bits(&self, ty: Type<'db>) -> Result<u32, Unsupported> {
        match self.types.number(ty) {
            Some(crate::ty::Number::Integer { bits, .. }) => Ok(bits),
            Some(crate::ty::Number::Bool) => Ok(1),
            _ => Err(self.types.unsupported(ty)),
        }
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
    pub(crate) fn located(&self, instruction: &str) -> String {
        if let Some(location) = &self.fixed_location {
            return format!("{instruction}, !dbg {location}");
        }
        match &self.scope {
            Some(scope) => format!(
                "{instruction}, !dbg {}",
                self.constants.debug_location(scope, self.span)
            ),
            None => instruction.to_owned(),
        }
    }

    /// Writes one instruction.
    pub(crate) fn line(&mut self, instruction: &str) {
        let located = self.located(instruction);
        let _ = writeln!(self.code, "  {located}");
    }

    /// Starts the block `label`.
    pub(crate) fn line_label(&mut self, label: &str) {
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

/// `@name` for a global, quoted when the name has characters LLVM does not allow bare.
pub(crate) fn global(name: &str) -> String {
    let bare = name
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$'));
    if bare {
        format!("@{name}")
    } else {
        let mut quoted = String::from("@\"");
        for byte in name.bytes() {
            if byte == b'"' || byte == b'\\' || !(0x20..0x7f).contains(&byte) {
                let _ = write!(quoted, "\\{byte:02X}");
            } else {
                quoted.push(char::from(byte));
            }
        }
        quoted.push('"');
        quoted
    }
}

/// `value` as an LLVM constant of an integer type `bits` wide: wrapped to the width and written
/// signed, the form LLVM reads for every integer type.
pub(crate) fn wrap(value: i128, bits: u32) -> i128 {
    if bits >= 128 {
        return value;
    }
    let shift = 128 - bits;
    (value << shift) >> shift
}

/// A float constant in LLVM's exact hexadecimal form. An `f32` constant is rounded to `f32`
/// first and written as the `f64` holding the same value, as LLVM requires.
pub(crate) fn float(value: f64, single: bool) -> String {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "rounding an f32 constant to f32 is the point"
    )]
    let value = if single {
        f64::from(value as f32)
    } else {
        value
    };
    format!("0x{:016X}", value.to_bits())
}
