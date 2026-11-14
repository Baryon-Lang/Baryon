//! `str`'s methods, [`Builtin::Text`](mir::Builtin::Text) (roadmap 1.13.12): each a call of the
//! runtime function that does the work, `baryon_rt_str_take`, `baryon_rt_str_split` and so on.
//!
//! A `str` argument passes as its byte pointer and byte count, the flag of decision 0022 masked
//! off; a position passes as an `i64`; a `list[str]` passes as its element pointer and length.
//! A new `str` comes back as `{ ptr, i64 }` with the flag clear, so the destination owns it. A
//! new `list[str]` is written by the runtime straight into the destination's header, which has
//! the runtime's `OwnedList` layout, `{ ptr, i64, i64 }`.
//!
//! `int(text)` and `float(text)` (decision 0037) can fail, so they are written by
//! `parse_number`, which branches to the call's error edge as a call of a compiled function that
//! raises does; so is `data.decode()` (decision 0064), by `decode`. `text.encode()` cannot fail,
//! and the runtime writes its new `Array[u8, 1]` straight into the destination's header.

use crate::Unsupported;
use crate::function::{Function, block_label};
use crate::panic::Failure;
use crate::text::TEXT;
use mir::{Argument, BlockId, ErrorEdge, Place, TextMethod};

/// What a runtime function for a `str` method returns, and so how the destination is written.
#[derive(Clone, Copy)]
enum Returns {
    /// A new owned `str`, `{ ptr, i64 }`, stored as it is.
    Text,
    /// A `bool`, returned as a C `bool`.
    Boolean,
    /// An `i64`.
    Integer,
    /// Nothing: the runtime writes a `list[str]`, or the `Array[u8, 1]` of `encode`, through a
    /// pointer to the destination, passed last.
    List,
}

impl<'db> Function<'_, '_, 'db> {
    /// Writes `destination = text.method(arguments…)` for the `str` method `method`, and goes to
    /// `target`. [`TextMethod`] says what each takes; an argument list of another shape is
    /// reported as unsupported, never guessed at.
    pub(crate) fn text_method(
        &mut self,
        method: TextMethod,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
    ) -> Result<(), Unsupported> {
        let shape = |texts: usize| -> Result<(), Unsupported> {
            if arguments.len() == texts {
                Ok(())
            } else {
                Err(Unsupported::new(format!(
                    "`{}` with {} arguments",
                    method.name(),
                    arguments.len()
                )))
            }
        };
        let (symbol, returns, values) = match method {
            TextMethod::Take => {
                shape(4)?;
                let mut values = vec![self.text_value(&arguments[0])?];
                for argument in &arguments[1..] {
                    let Argument::Owned(operand) = argument else {
                        return Err(Unsupported::new("a `str` position that is not owned"));
                    };
                    values.push(format!("i64 {}", self.value(operand)?.0));
                }
                ("baryon_rt_str_take", Returns::Text, values)
            }
            TextMethod::Split => {
                shape(2)?;
                let text = self.text_value(&arguments[0])?;
                let (bytes, length) = self.text_argument(&arguments[1])?;
                let filled = self.emit(&format!("icmp ne i64 {length}, 0"));
                self.check(&filled, &Failure::Message("empty separator".to_owned()));
                let separator = format!("ptr {bytes}, i64 {length}");
                ("baryon_rt_str_split", Returns::List, vec![text, separator])
            }
            TextMethod::SplitWhitespace => {
                shape(1)?;
                let values = vec![self.text_value(&arguments[0])?];
                ("baryon_rt_str_split_whitespace", Returns::List, values)
            }
            TextMethod::Strip | TextMethod::StripStart | TextMethod::StripEnd => {
                let side = match method {
                    TextMethod::StripStart => 1,
                    TextMethod::StripEnd => 2,
                    _ => 0,
                };
                let mut values = self.text_values(arguments)?;
                let symbol = match values.len() {
                    1 => "baryon_rt_str_strip",
                    2 => "baryon_rt_str_strip_characters",
                    _ => return shape(2),
                };
                values.push(format!("i32 {side}"));
                (symbol, Returns::Text, values)
            }
            TextMethod::Join => {
                shape(2)?;
                let separator = self.text_value(&arguments[0])?;
                let Argument::Borrow(list) = &arguments[1] else {
                    return Err(Unsupported::new(
                        "`str.join` of a list that is not borrowed",
                    ));
                };
                let (header, _) = self.place(list)?;
                let data = self.emit(&format!("load ptr, ptr {header}"));
                let length = self.list_length(&header);
                let pieces = format!("ptr {data}, i64 {length}");
                ("baryon_rt_str_join", Returns::Text, vec![separator, pieces])
            }
            TextMethod::StartsWith | TextMethod::EndsWith | TextMethod::Find => {
                shape(2)?;
                let (symbol, returns) = match method {
                    TextMethod::StartsWith => ("baryon_rt_str_starts_with", Returns::Boolean),
                    TextMethod::EndsWith => ("baryon_rt_str_ends_with", Returns::Boolean),
                    _ => ("baryon_rt_str_find", Returns::Integer),
                };
                (symbol, returns, self.text_values(arguments)?)
            }
            TextMethod::Replace => {
                shape(3)?;
                (
                    "baryon_rt_str_replace",
                    Returns::Text,
                    self.text_values(arguments)?,
                )
            }
            TextMethod::Lower | TextMethod::Upper | TextMethod::Encode => {
                shape(1)?;
                let (symbol, returns) = match method {
                    TextMethod::Lower => ("baryon_rt_str_lower", Returns::Text),
                    TextMethod::Upper => ("baryon_rt_str_upper", Returns::Text),
                    _ => ("baryon_rt_str_encode", Returns::List),
                };
                (symbol, returns, self.text_values(arguments)?)
            }
            TextMethod::ParseInt | TextMethod::ParseFloat | TextMethod::Decode => {
                let name = method.name();
                return Err(Unsupported::new(format!("`{name}` with no error edge")));
            }
        };
        self.runtime_text_call(symbol, returns, values, destination)?;
        self.line(&format!("br label %{}", block_label(target)));
        Ok(())
    }

    /// Writes `destination = int(text)` or `float(text)` (decision 0037) and goes to `target`,
    /// or to the error edge's block with the `ValueError` in its local. The runtime reads the
    /// number and raises as a compiled function does: `baryon_rt_str_parse_int` and
    /// `baryon_rt_str_parse_float` take the text, the destination's address, written only on
    /// success, and the error local's address, and return an `i1` that is true when they failed.
    pub(crate) fn parse_number(
        &mut self,
        method: TextMethod,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
        error: Option<&ErrorEdge>,
    ) -> Result<(), Unsupported> {
        let name = method.name();
        let edge = error.ok_or_else(|| Unsupported::new(format!("`{name}` with no error edge")))?;
        let [text] = arguments else {
            return Err(Unsupported::new(format!("`{name}` with these arguments")));
        };
        let (bytes, length) = self.text_argument(text)?;
        let (out, _) = self.place(destination)?;
        let (raised, _) = self.place(&edge.error.into())?;
        let symbol = if method == TextMethod::ParseInt {
            "baryon_rt_str_parse_int"
        } else {
            "baryon_rt_str_parse_float"
        };
        self.declare(
            symbol,
            &format!("declare zeroext i1 @{symbol}(ptr, i64, ptr, ptr) nounwind"),
        );
        let failed = self.emit(&format!(
            "call zeroext i1 @{symbol}(ptr {bytes}, i64 {length}, ptr {out}, ptr {raised})"
        ));
        self.line(&format!(
            "br i1 {failed}, label %{}, label %{}",
            block_label(edge.target),
            block_label(target)
        ));
        Ok(())
    }

    /// Writes `destination = data.decode()` (decision 0064) and goes to `target`, or to the
    /// error edge's block with the `ValueError` in its local when the bytes are not UTF-8.
    /// `baryon_rt_array_decode` takes the address of the borrowed `Array[u8, 1]`'s header, the
    /// destination's address, written only on success, and the error local's address, and
    /// returns an `i1` that is true when it failed.
    pub(crate) fn decode(
        &mut self,
        arguments: &[Argument<'db>],
        destination: &Place,
        target: BlockId,
        error: Option<&ErrorEdge>,
    ) -> Result<(), Unsupported> {
        let edge = error.ok_or_else(|| Unsupported::new("`Array.decode` with no error edge"))?;
        let [Argument::Borrow(array)] = arguments else {
            return Err(Unsupported::new("`Array.decode` with these arguments"));
        };
        let (array, _) = self.place(array)?;
        let (out, _) = self.place(destination)?;
        let (raised, _) = self.place(&edge.error.into())?;
        let symbol = "baryon_rt_array_decode";
        self.declare(
            symbol,
            &format!("declare zeroext i1 @{symbol}(ptr, ptr, ptr) nounwind"),
        );
        let failed = self.emit(&format!(
            "call zeroext i1 @{symbol}(ptr {array}, ptr {out}, ptr {raised})"
        ));
        self.line(&format!(
            "br i1 {failed}, label %{}, label %{}",
            block_label(edge.target),
            block_label(target)
        ));
        Ok(())
    }

    /// The `str` that `argument` passes, as the two LLVM call arguments `ptr bytes, i64 count`.
    fn text_value(&mut self, argument: &Argument<'db>) -> Result<String, Unsupported> {
        let (bytes, length) = self.text_argument(argument)?;
        Ok(format!("ptr {bytes}, i64 {length}"))
    }

    /// [`text_value`](Self::text_value) of each of `arguments`, every one a `str`.
    fn text_values(&mut self, arguments: &[Argument<'db>]) -> Result<Vec<String>, Unsupported> {
        arguments
            .iter()
            .map(|argument| self.text_value(argument))
            .collect()
    }

    /// Declares and calls the runtime function `symbol` with `values`, each an LLVM argument
    /// with its type, and writes what it gives to `destination`.
    fn runtime_text_call(
        &mut self,
        symbol: &str,
        returns: Returns,
        mut values: Vec<String>,
        destination: &Place,
    ) -> Result<(), Unsupported> {
        let (address, _) = self.place(destination)?;
        if let Returns::List = returns {
            values.push(format!("ptr {address}"));
        }
        // Each value is one or more `type operand` pairs, such as `ptr %t1, i64 %t3`.
        let parameters: Vec<&str> = values
            .iter()
            .flat_map(|value| value.split(", "))
            .map(|pair| pair.split(' ').next().unwrap_or(pair))
            .collect();
        let (declared, called, stored) = match returns {
            Returns::Text => (TEXT, TEXT, Some(TEXT)),
            Returns::Boolean => ("zeroext i1", "i1", Some("i1")),
            Returns::Integer => ("i64", "i64", Some("i64")),
            Returns::List => ("void", "void", None),
        };
        self.declare(
            symbol,
            &format!(
                "declare {declared} @{symbol}({}) nounwind",
                parameters.join(", ")
            ),
        );
        let call = format!("call {called} @{symbol}({})", values.join(", "));
        match stored {
            Some(ty) => {
                let result = self.emit(&call);
                self.line(&format!("store {ty} {result}, ptr {address}"));
            }
            None => self.line(&call),
        }
        Ok(())
    }
}
