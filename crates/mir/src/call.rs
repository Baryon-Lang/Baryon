//! What a call calls and how its arguments pass: the [`Callee`] of a
//! [`Call`](crate::TerminatorKind::Call), the [`Builtin`] operations and [`MathFunction`]s the
//! backends build inline or call the runtime for, and each [`Argument`]'s mode.

use crate::{Operand, Place, TextMethod};
use types::{Item, Type};

/// The function a [`Call`](crate::TerminatorKind::Call) calls.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub enum Callee<'db> {
    /// A function or method named in the source, at these type arguments.
    Function {
        /// The function.
        item: Item<'db>,
        /// One type per type parameter of the function, in order; empty when it has none. In a
        /// built generic body these may themselves contain type parameters.
        type_arguments: Vec<Type<'db>>,
    },
    /// A function passed in as a value, of a `Callable` type.
    Value(Operand<'db>),
    /// A function the language provides, which has no item: each backend writes it itself.
    Builtin(Builtin),
}

/// A function the language provides that a [`Call`](crate::TerminatorKind::Call) can call by
/// [`Callee::Builtin`]. Built-ins that are only arithmetic, such as `len` or a conversion, are
/// lowered to rvalues instead; the ones here need code a backend or the runtime provides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub enum Builtin {
    /// Writes each argument to standard output in order, as Python's `print` writes it, with
    /// nothing between them and nothing after the last. Lowering spells out the separator and
    /// the line end as arguments of their own, so `print(a, b)` is a call with the arguments
    /// `a`, `" "`, `b` and `"\n"`, and the `sep=` and `end=` keywords need nothing more here.
    ///
    /// An argument is passed `Borrow`, or `Owned` when it is a constant. A `str` argument writes
    /// its characters unquoted. Any other value writes as the language reference's `print`
    /// entry says: a number, a `bool`, `None`, a `T | None`, a tuple, an enum member or a struct,
    /// whose items and fields write as Python's `repr` writes them. A backend that cannot write
    /// a type yet reports it. The destination has type `None` and the call cannot fail.
    Print,
    /// A mathematical function of floats. Every argument and the destination have one float
    /// type, `f32` or `f64`; lowering converts integer operands first. The one exception is
    /// [`Power`](MathFunction::Power) of complex numbers, where every argument and the
    /// destination have one complex type instead. The call cannot fail.
    Math(MathFunction),
    /// `a.sum()` of an `Array` or `Fixed` array of floats: one argument, the array, passed
    /// `Borrow`; the destination has the element type. The elements are added in row-major
    /// order in `NumPy`'s pairwise tree, so the result has the same bits as `numpy.sum` of the
    /// same array and rounding error grows with the logarithm of the length, not the length.
    /// Integer sums need no tree, since integer addition is exact, and lower to a loop.
    Sum,
    /// `a.dot(b)` of two rank-1 `Array` or `Fixed` arrays of floats: two arguments, the arrays,
    /// each passed `Borrow`, of one element type and the same length, which lowering checks
    /// first; the destination has the element type. Each product is rounded to the element
    /// type, and the products are added in [`Sum`](Self::Sum)'s pairwise order, so `a.dot(b)`
    /// has the same bits as `(a * b).sum()` without building `a * b`. A multiply and an add are
    /// never fused into one rounding. Integer dot products lower to a loop, as integer sums do.
    Dot,
    /// `xs.append(value)` on a `list`: two arguments, the list passed `InOut` and the value
    /// passed `Owned`, of the list's element type; the destination has type `None`. When the
    /// buffer is full it grows to twice its capacity, at least 4 elements, so a run of appends
    /// costs amortized constant time per element, as Python's and Rust's do. Running out of
    /// memory panics; the call cannot fail otherwise.
    Append,
    /// `xs.pop()` on a `list`: one argument, the list passed `InOut`; the destination has the
    /// list's element type. It removes the last element and moves it to the destination, in
    /// constant time, and keeps the buffer, so a later append does not allocate. An empty list
    /// panics with `pop from empty list`, the message of Python's `IndexError`; the call cannot
    /// fail otherwise.
    Pop,
    /// `random.Philox(seed, stream)`: two arguments, the seed and the stream, each an `i64`
    /// passed `Owned`; the destination has type `Philox`. It makes the generator whose numbers
    /// match `NumPy`'s `Generator(Philox(key=seed).jumped(stream))` (decision 0028). A negative
    /// seed or stream panics; the call cannot fail otherwise.
    Philox,
    /// `generator.uniform()`: one argument, a `Philox` passed `InOut`; the destination has type
    /// `f64`. It draws the generator's next 64-bit output `u` and gives `(u >> 11) * 2**-53`, a
    /// float in `[0, 1)`, as `NumPy`'s `Generator.random()` does. The call cannot fail.
    Uniform,
    /// Compares two `str`s as Python orders them, by code point with a prefix first: two
    /// arguments, each a `str` passed `Borrow`, or `Owned` for a literal; the destination has type
    /// `i64` and gets -1 when the first comes first, 0 when they are equal and 1 when the second
    /// comes first. Lowering compares that with 0 for `==`, `!=`, `<`, `<=`, `>` and `>=`. It
    /// reads both strings, so it costs time linear in the shorter. The call cannot fail.
    CompareStr,
    /// `needle in haystack` on two `str`s: two arguments, the haystack then the needle, each
    /// passed `Borrow`, or `Owned` for a literal; the destination has type `bool`. The empty
    /// string is in every string. The search takes time linear in the two lengths. The call
    /// cannot fail.
    ContainsStr,
    /// `len(text)` of a `str`: one argument passed `Borrow`, or `Owned` for a literal; the
    /// destination has type `i64`. It counts characters, code points, as Python does, not bytes,
    /// so it reads the whole string. The call cannot fail.
    LengthStr,
    /// One character of a `str`, for `for c in text` (roadmap 1.13.8): two arguments, the `str`
    /// passed `Borrow`, or `Owned` for a literal, and a cursor, an `i64` byte offset passed
    /// `InOut`; the destination is a `str` that borrows the bytes of the character that starts at
    /// the cursor, flagged as not owned (decision 0022), so it is never freed. The call moves the
    /// cursor past the character, so a loop that starts the cursor at 0 and calls it once for each
    /// of the [`LengthStr`](Self::LengthStr) characters visits each character in order.
    ///
    /// The cursor must be the first byte of a character inside the string, which that loop
    /// guarantees, so the call checks nothing. It reads one byte to find the character's width
    /// in UTF-8, so it takes constant time. The call cannot fail.
    CharacterStr,
    /// A method of `str`, such as `split` or `strip`, or indexing and slicing one (roadmap
    /// 1.13.12). [`TextMethod`] says what each takes and gives.
    Text(TextMethod),
    /// Looks a key up in a `dict` (decision 0035): two arguments, the `dict` and the key, of the
    /// key type, each passed `Borrow`, so a constant key is put in a local first; the destination is an
    /// `i64`, the position of the key's entry, or -1 when the key is not there. The position
    /// names the entry with an [`Index`](crate::Projection::Index) step on the `dict`.
    ///
    /// It hashes the key once and compares it with the keys in its probe sequence, so it takes
    /// constant time on average, plus the key's length for a `str`. The call cannot fail.
    DictFind,
    /// Adds a key that is not in a `dict` yet: two arguments, the `dict` passed `InOut` and the
    /// key passed `Owned`, as a move or copy of a place; the destination is an `i64`, the position of the new entry, the last
    /// in insertion order. The entry's value is not set: the stage that writes this call writes
    /// the value with an [`Index`](crate::Projection::Index) step before anything reads the
    /// `dict`, as lowering of `d[k] = v` does after a [`DictFind`](Self::DictFind) that gave -1.
    ///
    /// Adding may move every entry, so a position from before the call is stale after it. The
    /// table grows by doubling, so a run of insertions costs amortized constant time each.
    /// Running out of memory panics; the call cannot fail otherwise.
    DictInsert,
    /// Removes the entry at a position from a `dict`, as `d.pop(k)` does once it has found `k`:
    /// two arguments, the `dict` passed `InOut` and the position, an `i64` from
    /// [`DictFind`](Self::DictFind) that is not -1, passed `Owned`; the destination has the
    /// value type and receives the value, moved out. The key is dropped. The entry becomes a hole
    /// that iteration skips, so the other entries keep their positions and their order. The call
    /// cannot fail.
    DictRemove,
    /// Steps through a `dict` in insertion order: two arguments, the `dict` passed `Borrow` and a
    /// position, an `i64` of 0 or more, passed `Owned`; the destination is an `i64`, the first
    /// position at or after the argument that holds an entry, or -1 when none does. A loop over a
    /// `dict` starts at 0 and passes one past each position it visits. The call cannot fail.
    DictNext,
    /// `len(sys.argv)`, how many command-line arguments the program was started with, the
    /// program itself included (decision 0036): no arguments; the destination has type `i64`.
    /// Lowering builds `sys.argv` from it and [`Argument`](Self::Argument). The call cannot fail.
    ArgumentCount,
    /// One command-line argument, `sys.argv[i]` (decision 0036): one argument, the index, an
    /// `i64` passed `Owned`, at least 0 and below [`ArgumentCount`](Self::ArgumentCount), which
    /// lowering guarantees; the destination is a new owned `str`. Bytes that are not UTF-8 read
    /// as U+FFFD. It copies the argument, so it costs time linear in its length. The call cannot
    /// fail.
    Argument,
    /// Reads an environment variable, as `os.environ.get(name)` does (decision 0036): two
    /// arguments, the name, a `str` passed `Borrow`, or `Owned` for a literal, and a `bool`
    /// passed `InOut` that the call sets to whether the variable is set; the destination is a
    /// new owned `str`, the value, or the empty string when it is not set. Lowering builds the
    /// `str | None` of `get`, and the panic of `os.environ[name]`, from the two. A value that is
    /// not UTF-8 reads with U+FFFD in place of each bad sequence. The call cannot fail.
    EnvironmentGet,
    /// `sys.exit(status)` (decision 0036): one argument, the status, an `i64` passed `Owned`;
    /// the destination has type `None`, and the call never returns. It writes out what `print`
    /// holds back and ends the program with the status, running nothing else on the way out. A
    /// status outside 0 to 255 panics.
    Exit,
    /// Sorts a `list` in place, for `sorted(values)` (roadmap 1.13.13): two arguments, the list
    /// passed `InOut` and a `bool` passed `Owned`, set to sort from the largest; the destination
    /// has type `None`. The elements are numbers, `bool`s or `str`s, which Python orders: numbers
    /// by value, `str`s by code point with a prefix first. The sort is stable, also from the
    /// largest, so equal elements keep their order, as Python's does, and every float NaN goes
    /// last either way, as `numpy.sort` puts it (Collins, 2026-11-10). It is a merge sort:
    /// O(n log n) comparisons and a buffer of half the list. The call cannot fail.
    Sort,
    /// `round(x, digits)` (roadmap 1.13.13): two arguments, each passed `Owned`, the number, an
    /// `f32`, an `f64` or an `i64`, and the digits, an `i64`; the destination has the number's
    /// type. It gives the multiple of `10 ** -digits` nearest the number's exact value, ties to
    /// the even multiple, as Python's `round` does, so `round(2.675, 2)` is `2.67`. An integer
    /// with zero or more digits is itself; one whose result does not fit panics with
    /// ``integer overflow in `round` ``. NaN and the infinities come back unchanged.
    RoundDigits,
}

impl Builtin {
    /// The name the source calls it by.
    ///
    /// ```
    /// assert_eq!(mir::Builtin::Print.name(), "print");
    /// assert_eq!(mir::Builtin::Math(mir::MathFunction::Sqrt).name(), "math.sqrt");
    /// assert_eq!(mir::Builtin::Sum.name(), "sum");
    /// assert_eq!(mir::Builtin::Dot.name(), "dot");
    /// assert_eq!(mir::Builtin::Append.name(), "list.append");
    /// assert_eq!(mir::Builtin::Pop.name(), "list.pop");
    /// assert_eq!(mir::Builtin::Philox.name(), "random.Philox");
    /// assert_eq!(mir::Builtin::Uniform.name(), "Philox.uniform");
    /// assert_eq!(mir::Builtin::ContainsStr.name(), "str in");
    /// assert_eq!(mir::Builtin::CharacterStr.name(), "str character");
    /// assert_eq!(mir::Builtin::Text(mir::TextMethod::Split).name(), "str.split");
    /// assert_eq!(mir::Builtin::DictFind.name(), "dict lookup");
    /// assert_eq!(mir::Builtin::Exit.name(), "sys.exit");
    /// assert_eq!(mir::Builtin::Sort.name(), "sorted");
    /// ```
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Print => "print",
            Self::Math(function) => function.name(),
            Self::Sum => "sum",
            Self::Dot => "dot",
            Self::Append => "list.append",
            Self::Pop => "list.pop",
            Self::Philox => "random.Philox",
            Self::Uniform => "Philox.uniform",
            Self::CompareStr => "str comparison",
            Self::ContainsStr => "str in",
            Self::LengthStr => "len of str",
            Self::CharacterStr => "str character",
            Self::Text(method) => method.name(),
            Self::DictFind => "dict lookup",
            Self::DictInsert => "dict insert",
            Self::DictRemove => "dict remove",
            Self::DictNext => "dict iteration",
            Self::ArgumentCount => "len of sys.argv",
            Self::Argument => "sys.argv item",
            Self::EnvironmentGet => "os.environ.get",
            Self::Exit => "sys.exit",
            Self::Sort => "sorted",
            Self::RoundDigits => "round",
        }
    }
}

/// A mathematical function of floats, called by [`Builtin::Math`]. Each gives the result IEEE
/// 754 and the C library give for its argument type; none of them fails or panics. NaN and the
/// infinities pass through as C's functions define, so `sqrt(-1.0)` is NaN, as `numpy.sqrt`
/// gives, where Python's `math.sqrt` raises.
///
/// These are the functions of Python's `math` module that take and return floats (roadmap
/// 1.13.1). `math.floor`, `math.ceil` and `math.trunc` return an integer in Python, so each is a
/// call of [`Floor`](Self::Floor), [`Ceil`](Self::Ceil) or [`Truncate`](Self::Truncate) and then
/// a conversion. `isnan`, `isinf`, `isfinite`, `degrees` and `radians` need no function: lowering
/// writes them as comparisons and arithmetic. Functions of integers alone, such as `gcd`, are not
/// here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub enum MathFunction {
    /// `math.sqrt(x)`: the square root, correctly rounded.
    Sqrt,
    /// `math.cbrt(x)`: the cube root, negative for a negative `x`.
    Cbrt,
    /// `math.exp(x)`: e to the power `x`.
    Exp,
    /// `math.exp2(x)`: 2 to the power `x`.
    Exp2,
    /// `math.expm1(x)`: `exp(x) - 1`, accurate when `x` is close to zero.
    ExpMinusOne,
    /// `math.log(x)`: the natural logarithm.
    Log,
    /// `math.log2(x)`: the base-2 logarithm.
    Log2,
    /// `math.log10(x)`: the base-10 logarithm.
    Log10,
    /// `math.log1p(x)`: `log(1 + x)`, accurate when `x` is close to zero.
    LogOnePlus,
    /// `math.sin(x)`, with `x` in radians.
    Sin,
    /// `math.cos(x)`, with `x` in radians.
    Cos,
    /// `math.tan(x)`, with `x` in radians.
    Tan,
    /// `math.asin(x)`: the arc sine in radians, in `[-pi/2, pi/2]`.
    Asin,
    /// `math.acos(x)`: the arc cosine in radians, in `[0, pi]`.
    Acos,
    /// `math.atan(x)`: the arc tangent in radians, in `[-pi/2, pi/2]`.
    Atan,
    /// `math.atan2(y, x)`: the angle of the point `(x, y)` in radians, in `[-pi, pi]`; two
    /// arguments, `y` first, as in Python and C.
    Atan2,
    /// `math.sinh(x)`: the hyperbolic sine.
    Sinh,
    /// `math.cosh(x)`: the hyperbolic cosine.
    Cosh,
    /// `math.tanh(x)`: the hyperbolic tangent.
    Tanh,
    /// `math.asinh(x)`: the inverse hyperbolic sine.
    Asinh,
    /// `math.acosh(x)`: the inverse hyperbolic cosine.
    Acosh,
    /// `math.atanh(x)`: the inverse hyperbolic tangent.
    Atanh,
    /// `math.erf(x)`: the error function.
    Erf,
    /// `math.erfc(x)`: `1 - erf(x)`, accurate when `erf(x)` is close to 1.
    Erfc,
    /// `math.gamma(x)`: the gamma function, C's `tgamma`.
    Gamma,
    /// `math.lgamma(x)`: the natural logarithm of the absolute value of the gamma function. It
    /// must not write C's global `signgam`, which a `prange` loop would race on.
    LogGamma,
    /// `math.fabs(x)`: the absolute value, always a float.
    Fabs,
    /// `math.hypot(x, y)`: `sqrt(x * x + y * y)` without overflow or underflow in between; two
    /// arguments. Python's `hypot` of more than two arguments is not here. `abs` of a complex
    /// number is `hypot` of its parts, as Python computes it.
    Hypot,
    /// `math.copysign(x, y)`: the size of `x` with the sign of `y`; two arguments.
    CopySign,
    /// `math.fmod(x, y)`: the remainder of `x / y` truncated toward zero, with the sign of `x`,
    /// as C's `fmod`; two arguments. It differs from `%`, which floors as Python does.
    TruncatedRemainder,
    /// `math.fma(x, y, z)`: `x * y + z` rounded once; three arguments.
    FusedMultiplyAdd,
    /// `x ** y` on floats, and `math.pow(x, y)`: two arguments, the base then the exponent.
    ///
    /// Also `x ** y` on two complex numbers of one type, `complex64` or `complex128`, computed
    /// as Python's `complex.__pow__` computes it: by repeated squaring for a whole exponent of
    /// at most 100 in size, through polar form otherwise. Where Python raises
    /// `ZeroDivisionError`, zero to a negative or complex power, the result is `nan+nanj`, as
    /// `NumPy` gives; where it raises `OverflowError`, the parts are infinite.
    Power,
    /// `math.floor(x)` before its conversion to an integer: the largest whole number not above
    /// `x`, still a float.
    Floor,
    /// `math.ceil(x)` before its conversion to an integer: the smallest whole number not below
    /// `x`, still a float.
    Ceil,
    /// `math.trunc(x)` before its conversion to an integer: `x` with its fraction dropped,
    /// rounding toward zero, still a float.
    Truncate,
    /// `round` of a float to the nearest whole number, ties to even, as Python's `round` and
    /// `numpy.round` do; the result is still a float, so `2.5` gives `2.0`.
    RoundEven,
}

impl MathFunction {
    /// The name the source calls it by. `**` is written `pow`, its name in `math`, and
    /// [`Floor`](Self::Floor), [`Ceil`](Self::Ceil) and [`Truncate`](Self::Truncate) by the
    /// `math` function they are the first step of.
    ///
    /// ```
    /// assert_eq!(mir::MathFunction::Power.name(), "math.pow");
    /// assert_eq!(mir::MathFunction::LogOnePlus.name(), "math.log1p");
    /// assert_eq!(mir::MathFunction::RoundEven.name(), "round");
    /// ```
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Sqrt => "math.sqrt",
            Self::Cbrt => "math.cbrt",
            Self::Exp => "math.exp",
            Self::Exp2 => "math.exp2",
            Self::ExpMinusOne => "math.expm1",
            Self::Log => "math.log",
            Self::Log2 => "math.log2",
            Self::Log10 => "math.log10",
            Self::LogOnePlus => "math.log1p",
            Self::Sin => "math.sin",
            Self::Cos => "math.cos",
            Self::Tan => "math.tan",
            Self::Asin => "math.asin",
            Self::Acos => "math.acos",
            Self::Atan => "math.atan",
            Self::Atan2 => "math.atan2",
            Self::Sinh => "math.sinh",
            Self::Cosh => "math.cosh",
            Self::Tanh => "math.tanh",
            Self::Asinh => "math.asinh",
            Self::Acosh => "math.acosh",
            Self::Atanh => "math.atanh",
            Self::Erf => "math.erf",
            Self::Erfc => "math.erfc",
            Self::Gamma => "math.gamma",
            Self::LogGamma => "math.lgamma",
            Self::Fabs => "math.fabs",
            Self::Hypot => "math.hypot",
            Self::CopySign => "math.copysign",
            Self::TruncatedRemainder => "math.fmod",
            Self::FusedMultiplyAdd => "math.fma",
            Self::Power => "math.pow",
            Self::Floor => "math.floor",
            Self::Ceil => "math.ceil",
            Self::Truncate => "math.trunc",
            Self::RoundEven => "round",
        }
    }

    /// How many arguments it takes: 3 for [`FusedMultiplyAdd`](Self::FusedMultiplyAdd); 2 for
    /// [`Power`](Self::Power), [`Atan2`](Self::Atan2), [`Hypot`](Self::Hypot),
    /// [`CopySign`](Self::CopySign) and [`TruncatedRemainder`](Self::TruncatedRemainder); 1 for
    /// the rest.
    ///
    /// ```
    /// assert_eq!(mir::MathFunction::Sqrt.arity(), 1);
    /// assert_eq!(mir::MathFunction::Power.arity(), 2);
    /// assert_eq!(mir::MathFunction::FusedMultiplyAdd.arity(), 3);
    /// ```
    #[must_use]
    pub fn arity(self) -> usize {
        match self {
            Self::FusedMultiplyAdd => 3,
            Self::Power | Self::Atan2 | Self::Hypot | Self::CopySign | Self::TruncatedRemainder => {
                2
            }
            _ => 1,
        }
    }
}

/// One argument of a call, in the mode its parameter declares (decision 0006). The modes stay
/// visible here so the reference checker and the backends' `noalias` metadata read them directly.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub enum Argument<'db> {
    /// A read-only borrow of the place, the default mode.
    Borrow(Place),
    /// An `InOut` borrow: the callee may change the place, and nothing else may touch it during
    /// the call.
    InOut(Place),
    /// An `Owned` argument: the value moves, or for a copy type copies, into the callee.
    Owned(Operand<'db>),
}
