//! The built-in operations on `str` that need a loop, each a call of the runtime: comparing two
//! strings ([`Builtin::CompareStr`](mir::Builtin::CompareStr)), finding one inside another
//! ([`Builtin::ContainsStr`](mir::Builtin::ContainsStr)) and counting characters
//! ([`Builtin::LengthStr`](mir::Builtin::LengthStr)). Taking one character
//! ([`Builtin::CharacterStr`](mir::Builtin::CharacterStr)) needs no loop and no runtime call: it
//! reads the character's first byte and borrows the bytes it covers.
//!
//! Each `str` argument is passed to the runtime as its byte pointer and its byte count, with the
//! "not owned" bit of decision 0022 masked off by [`text_parts`](Function::text_parts).

use crate::Unsupported;
use crate::function::{Function, block_label};
use crate::text::{STATIC, TEXT};
use mir::{Argument, BlockId, Place};

/// A runtime function over `str`s: its name, how many `str`s it takes, and the LLVM type it
/// returns and the destination's LLVM type, which differ when the result is widened.
pub(crate) struct TextFunction {
    /// The runtime's symbol.
    symbol: &'static str,
    /// How many `str` arguments it takes.
    arity: usize,
    /// The LLVM type the runtime returns, with any attribute the C ABI needs before it.
    returns: &'static str,
    /// The instruction that turns the returned value into the destination's type, before the
    /// value, or `None` when it is stored as it is.
    widen: Option<&'static str>,
    /// The destination's LLVM type.
    stores: &'static str,
}

/// `baryon_rt_str_compare`: -1, 0 or 1 as a C `int`, widened to the destination's `i64`.
pub(crate) const COMPARE: TextFunction = TextFunction {
    symbol: "baryon_rt_str_compare",
    arity: 2,
    returns: "i32",
    widen: Some("sext i32"),
    stores: "i64",
};

/// `baryon_rt_str_contains`: whether the second `str` occurs in the first, as a `bool`.
pub(crate) const CONTAINS: TextFunction = TextFunction {
    symbol: "baryon_rt_str_contains",
    arity: 2,
    returns: "zeroext i1",
    widen: None,
    stores: "i1",
};

/// `baryon_rt_str_length`: the number of characters, as an `i64`.
pub(crate) const LENGTH: TextFunction = TextFunction {
    symbol: "baryon_rt_str_length",
    arity: 1,
    returns: "i64",
    widen: None,
    stores: "i64",
};

impl<'db> Function<'_, '_, 'db> {
    /// Writes `destination = function(arguments…)` for a runtime function over `str`s, and goes
    /// to `target`.
    pub(crate) fn text_builtin(
        &mut self,
        function: &TextFunction,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        if arguments.len() != function.arity {
            return Err(Unsupported::new(format!(
                "`{}` with {} arguments",
                function.symbol,
                arguments.len()
            )));
        }
        let mut values = Vec::with_capacity(2 * arguments.len());
        for argument in arguments {
            let value = match argument {
                Argument::Owned(operand) => self.value(operand)?.0,
                Argument::Borrow(place) | Argument::InOut(place) => {
                    let (address, ty) = self.place(place)?;
                    self.load(&address, ty)?
                        .ok_or_else(|| Unsupported::new("`None` where a `str` is needed"))?
                }
            };
            let (bytes, length) = self.text_parts(&value);
            values.push(format!("ptr {bytes}, i64 {length}"));
        }
        let parameters = vec!["ptr, i64"; function.arity].join(", ");
        let (symbol, returns) = (function.symbol, function.returns);
        self.declare(
            symbol,
            &format!("declare {returns} @{symbol}({parameters}) nounwind"),
        );
        let returned = returns.rsplit(' ').next().unwrap_or(returns);
        let mut result = self.emit(&format!("call {returned} @{symbol}({})", values.join(", ")));
        if let Some(widen) = function.widen {
            result = self.emit(&format!("{widen} {result} to {}", function.stores));
        }
        let (address, _) = self.place(destination)?;
        self.line(&format!(
            "store {} {result}, ptr {address}",
            function.stores
        ));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// Writes the character of a `str` that starts at a byte cursor into `destination`, as a
    /// `str` flagged as not owned that points into the string's own bytes, moves the cursor past
    /// it, and goes to `target`. The character's width in UTF-8 comes from its first byte: below
    /// `0xC0` one byte, then two from `0xC0`, three from `0xE0` and four from `0xF0`. A
    /// continuation byte never starts a character, so it never reaches the test.
    pub(crate) fn text_character(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let [text, Argument::InOut(cursor)] = arguments else {
            return Err(Unsupported::new("`str` character with these arguments"));
        };
        let value = match text {
            Argument::Owned(operand) => self.value(operand)?.0,
            Argument::Borrow(place) | Argument::InOut(place) => {
                let (address, ty) = self.place(place)?;
                self.load(&address, ty)?
                    .ok_or_else(|| Unsupported::new("`None` where a `str` is needed"))?
            }
        };
        let (bytes, _) = self.text_parts(&value);
        let (cursor, _) = self.place(cursor)?;
        let offset = self.emit(&format!("load i64, ptr {cursor}"));
        let at = self.emit(&format!(
            "getelementptr inbounds i8, ptr {bytes}, i64 {offset}"
        ));
        let lead = self.emit(&format!("load i8, ptr {at}"));
        let lead = self.emit(&format!("zext i8 {lead} to i64"));
        let mut width = "1".to_owned();
        for start in [0xC0, 0xE0, 0xF0] {
            let longer = self.emit(&format!("icmp uge i64 {lead}, {start}"));
            let extra = self.emit(&format!("zext i1 {longer} to i64"));
            width = self.emit(&format!("add i64 {width}, {extra}"));
        }
        let word = self.emit(&format!("or i64 {width}, {}", STATIC.cast_signed()));
        let view = self.emit(&format!("insertvalue {TEXT} poison, ptr {at}, 0"));
        let view = self.emit(&format!("insertvalue {TEXT} {view}, i64 {word}, 1"));
        let (address, _) = self.place(destination)?;
        self.line(&format!("store {TEXT} {view}, ptr {address}"));
        let next = self.emit(&format!("add i64 {offset}, {width}"));
        self.line(&format!("store i64 {next}, ptr {cursor}"));
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }
}
