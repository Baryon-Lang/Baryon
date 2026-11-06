//! Every kind of node a syntax tree can hold, with the children each one has.

/// What a node is: a function definition, a `for` loop, a name, an integer literal.
///
/// The kinds cover all of Python 3.12's syntax, because every source file is valid Python
/// (decision 0004). Which of these constructs the language accepts is a later stage's job; the
/// tree records what was written.
///
/// Each variant lists its children in source order, as `Role: Kind`. A `?` marks a child that
/// may be absent, `*` zero or more, `+` one or more. A node with no children listed is a leaf:
/// its text is its span of source, read with [`SyntaxTree::text`](crate::SyntaxTree::text).
/// "Expression" means any expression kind and "statement" any statement kind.
///
/// ```
/// use syntax::SyntaxKind;
///
/// assert!(SyntaxKind::For.is_statement());
/// assert!(SyntaxKind::Call.is_expression());
/// assert!(!SyntaxKind::Parameters.is_statement());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SyntaxKind {
    // Top level and definitions.
    /// A whole source file. Children: `Body: statement*`.
    Module,
    /// `def name[T](parameters) -> returns: body`, flagged `ASYNC` for `async def`.
    /// Children: `Decorator: Decorator*`, `Name: Name`, `TypeParameters: TypeParameters?`,
    /// `Parameters: Parameters`, `Returns: expression?`, `Body: statement+`.
    FunctionDefinition,
    /// `class Name[T](arguments): body`. Children: `Decorator: Decorator*`, `Name: Name`,
    /// `TypeParameters: TypeParameters?`, `Arguments: Arguments?`, `Body: statement+`.
    ClassDefinition,
    /// `@expression` above a definition. Children: `Value: expression`.
    Decorator,
    /// `[T, *Ts, **P]` after a definition's name. Children: `Item: TypeVariable`,
    /// `TypeVariableTuple` or `ParameterSpecification`, one or more.
    TypeParameters,
    /// `T`, `T: Bound` or `T = Default` in type parameters. Children: `Name: Name`,
    /// `Bound: expression?`, `Default: expression?`.
    TypeVariable,
    /// `*Ts` in type parameters. Children: `Name: Name`, `Default: expression?`.
    TypeVariableTuple,
    /// `**P` in type parameters. Children: `Name: Name`, `Default: expression?`.
    ParameterSpecification,
    /// The parenthesized parameter list of a function, or the parameters of a lambda. Children:
    /// `Parameter`, `StarParameter` and `DoubleStarParameter` nodes in source order, with role
    /// `PositionalOnly` before `/`, `KeywordOnly` after `*` or `*args`, and `Item` otherwise.
    Parameters,
    /// One ordinary parameter, `name: annotation = default`. Children: `Name: Name`,
    /// `Annotation: expression?`, `Default: expression?`.
    Parameter,
    /// `*args`. Children: `Name: Name`, `Annotation: expression?`.
    StarParameter,
    /// `**kwargs`. Children: `Name: Name`, `Annotation: expression?`.
    DoubleStarParameter,

    // Statements.
    /// `return value`. Children: `Value: expression?`.
    Return,
    /// `del targets`. Children: `Target: expression+`.
    Delete,
    /// `type Name[T] = value`. Children: `Name: Name`, `TypeParameters: TypeParameters?`,
    /// `Value: expression`.
    TypeAlias,
    /// `a = b = value`. Children: `Target: expression+`, `Value: expression`.
    Assign,
    /// `target += value` and the other in-place operators; the operator is on the node.
    /// Children: `Target: expression`, `Value: expression`.
    AugmentedAssign,
    /// `target: annotation = value`. Children: `Target: expression`,
    /// `Annotation: expression`, `Value: expression?`.
    AnnotatedAssign,
    /// `for target in iterator: body else: else`, flagged `ASYNC` for `async for`.
    /// Children: `Target: expression`, `Iterator: expression`, `Body: statement+`,
    /// `Else: statement*`.
    For,
    /// `while condition: body else: else`. Children: `Condition: expression`,
    /// `Body: statement+`, `Else: statement*`.
    While,
    /// `if condition: body` and its `elif` and `else` clauses. Children:
    /// `Condition: expression`, `Body: statement+`, `Else: ElifClause*`, `Else: ElseClause?`.
    If,
    /// `elif condition: body`. Children: `Condition: expression`, `Body: statement+`.
    ElifClause,
    /// `else: body` of an `if`. Children: `Body: statement+`.
    ElseClause,
    /// `with items: body`, flagged `ASYNC` for `async with`. Children: `Item: WithItem+`,
    /// `Body: statement+`.
    With,
    /// `value as target` in a `with`. Children: `Value: expression`, `Target: expression?`.
    WithItem,
    /// `match value:` and its cases. Children: `Value: expression`, `Case: MatchCase+`.
    Match,
    /// `case pattern if condition: body`. Children: `Pattern: pattern`,
    /// `Condition: expression?`, `Body: statement+`.
    MatchCase,
    /// `raise value from cause`. Children: `Value: expression?`, `Cause: expression?`.
    Raise,
    /// `try:` with its handlers, `else:` and `finally:`, flagged `STAR` for `except*`.
    /// Children: `Body: statement+`, `Handler: ExceptHandler*`, `Else: statement*`,
    /// `Finally: statement*`.
    Try,
    /// `except type as name: body`. Children: `Type: expression?`, `Name: Name?`,
    /// `Body: statement+`.
    ExceptHandler,
    /// `assert condition, message`. Children: `Condition: expression`,
    /// `Message: expression?`.
    Assert,
    /// `import a.b as c, d`. Children: `Item: Alias+`.
    Import,
    /// `from module import names`. Children: `Module: Name?`, `Item: Alias+`. The module is
    /// absent in `from . import x`; the dots of a relative import are not nodes, so a later
    /// stage counts them in the source text between `from` and the module.
    ImportFrom,
    /// `a.b as c` in an import. The name may be dotted, in which case it is one `Name` leaf
    /// spanning the whole dotted path. Children: `Name: Name`, `Alias: Name?`.
    Alias,
    /// `global names`. Children: `Item: Name+`.
    Global,
    /// `nonlocal names`. Children: `Item: Name+`.
    Nonlocal,
    /// An expression used as a statement, such as a call or a docstring.
    /// Children: `Value: expression`.
    ExpressionStatement,
    /// `pass`. A leaf.
    Pass,
    /// `break`. A leaf.
    Break,
    /// `continue`. A leaf.
    Continue,

    // Expressions.
    /// `a and b and c` or `a or b`; the operator is on the node. Children: `Item: expression+`.
    BooleanOperation,
    /// `target := value`. Children: `Target: Name`, `Value: expression`.
    NamedExpression,
    /// `left + right` and the other binary operators; the operator is on the node.
    /// Children: `Left: expression`, `Right: expression`.
    BinaryOperation,
    /// `-value`, `+value`, `~value` or `not value`; the operator is on the node.
    /// Children: `Value: expression`.
    UnaryOperation,
    /// `lambda parameters: body`. Children: `Parameters: Parameters?`, `Body: expression`.
    Lambda,
    /// `value if condition else other`. Children: `Value: expression`,
    /// `Condition: expression`, `Else: expression`.
    Conditional,
    /// `{key: value, **other}`. Children: `Item: DictionaryEntry` or `DoubleStarred`, zero or
    /// more.
    Dictionary,
    /// `key: value` in a dictionary. Children: `Key: expression`, `Value: expression`.
    DictionaryEntry,
    /// `{a, b}`. Children: `Item: expression+`.
    Set,
    /// `[value for ...]`. Children: `Value: expression`, `Item: Comprehension+`.
    ListComprehension,
    /// `{value for ...}`. Children: `Value: expression`, `Item: Comprehension+`.
    SetComprehension,
    /// `{key: value for ...}`. Children: `Key: expression`, `Value: expression`,
    /// `Item: Comprehension+`.
    DictionaryComprehension,
    /// `(value for ...)`, flagged `PARENTHESIZED` unless it is the sole argument of a call.
    /// Children: `Value: expression`, `Item: Comprehension+`.
    Generator,
    /// `for target in iterator if condition` inside a comprehension, flagged `ASYNC` for
    /// `async for`. Children: `Target: expression`, `Iterator: expression`,
    /// `Condition: expression*`.
    Comprehension,
    /// `await value`. Children: `Value: expression`.
    Await,
    /// `yield value`. Children: `Value: expression?`.
    Yield,
    /// `yield from value`. Children: `Value: expression`.
    YieldFrom,
    /// `a < b <= c`: a chain of comparisons. Children: `Left: expression`,
    /// `Item: Comparator+`.
    Compare,
    /// One link of a comparison chain: an operator and the operand after it. The operator is
    /// on the node. Its span starts where the operand before it ends, so it covers the operator.
    /// Children: `Right: expression`.
    Comparator,
    /// `function(arguments)`. Children: `Function: expression`, `Arguments: Arguments`.
    Call,
    /// The parenthesized arguments of a call or the bases of a class. Children, in source
    /// order: `Item: expression`, `Item: Starred`, `Item: Keyword`, `Item: DoubleStarred`.
    Arguments,
    /// `name=value` in arguments. Children: `Name: Name`, `Value: expression`.
    Keyword,
    /// `*value` in a call, a collection or an assignment target. Children: `Value: expression`.
    Starred,
    /// `**value` in a call or a dictionary. Children: `Value: expression`.
    DoubleStarred,
    /// An f-string, possibly implicitly joined with neighbouring strings: `f"x = {x}" "!"`.
    /// Children, in source order: `Item: StringLiteral`, `Item: FormattedText`,
    /// `Item: Interpolation`.
    FormattedString,
    /// A t-string, `t"..."`, which Python 3.14 added. Same children as `FormattedString`.
    TemplateString,
    /// The literal text between interpolations of an f-string. A leaf.
    FormattedText,
    /// `{value!r:format}` in an f-string, flagged `DEBUG` for `{value=}` and `CONVERT_STR`,
    /// `CONVERT_REPR` or `CONVERT_ASCII` for a conversion. Children: `Value: expression`,
    /// `Format: FormatSpecification?`.
    Interpolation,
    /// The format specification after `:` in an interpolation, such as `9.4f`. Children:
    /// `Item: FormattedText` and `Item: Interpolation`, in source order.
    FormatSpecification,
    /// A string literal, or several implicitly joined: `"a" "b"`. A leaf.
    StringLiteral,
    /// A bytes literal, or several implicitly joined. A leaf.
    BytesLiteral,
    /// An integer literal such as `42`, `0xff` or `1_000`. A leaf.
    Integer,
    /// A float literal such as `1.0` or `6.37e6`. A leaf.
    Float,
    /// An imaginary literal such as `2j`. A leaf.
    Imaginary,
    /// `True`. A leaf.
    True,
    /// `False`. A leaf.
    False,
    /// `None`. A leaf.
    None,
    /// `...`. A leaf.
    Ellipsis,
    /// `value.name`. Children: `Value: expression`, `Attribute: Name`.
    Attribute,
    /// `value[index]`. A comma-separated index is one `Tuple`. Children: `Value: expression`,
    /// `Index: expression`.
    Subscript,
    /// An identifier: a variable, function, parameter or attribute name. A leaf.
    Name,
    /// `[a, b]`. Children: `Item: expression*`.
    List,
    /// `a, b` or `(a, b)`, flagged `PARENTHESIZED` when written in parentheses.
    /// Children: `Item: expression*`.
    Tuple,
    /// `lower:upper:step` inside a subscript. Children: `Lower: expression?`,
    /// `Upper: expression?`, `Step: expression?`.
    Slice,
    /// Where the parser expected an expression and found none, after a syntax error. A leaf,
    /// usually with an empty span. A tree with diagnostics may contain these; a tree without
    /// any never does.
    Missing,

    // Patterns, the left-hand side of a `case`.
    /// A value to compare against, such as `0` or `Color.RED`. Children: `Value: expression`.
    PatternValue,
    /// `None`, `True` or `False`, compared by identity. A leaf.
    PatternSingleton,
    /// `[a, b, *rest]` or `(a, b)`. Children: `Item: pattern*`.
    PatternSequence,
    /// `{key: pattern, **rest}`. Children: `Key: expression` and `Pattern: pattern` in
    /// alternation, then `Rest: Name?`.
    PatternMapping,
    /// `Point(x, y=0)`. Children: `Class: expression`, `Item: pattern*`,
    /// `Item: PatternKeyword*`.
    PatternClass,
    /// `name=pattern` in a class pattern. Children: `Name: Name`, `Pattern: pattern`.
    PatternKeyword,
    /// `*rest` in a sequence pattern; `*_` has no name. Children: `Name: Name?`.
    PatternStar,
    /// `pattern as name`, a bare capture `name`, or the wildcard `_`, which has no children.
    /// Children: `Pattern: pattern?`, `Name: Name?`.
    PatternAs,
    /// `a | b`. Children: `Item: pattern+`.
    PatternOr,
}

impl SyntaxKind {
    /// Whether this kind is a statement: something that can appear in a block's body.
    #[must_use]
    pub fn is_statement(self) -> bool {
        matches!(
            self,
            Self::FunctionDefinition
                | Self::ClassDefinition
                | Self::Return
                | Self::Delete
                | Self::TypeAlias
                | Self::Assign
                | Self::AugmentedAssign
                | Self::AnnotatedAssign
                | Self::For
                | Self::While
                | Self::If
                | Self::With
                | Self::Match
                | Self::Raise
                | Self::Try
                | Self::Assert
                | Self::Import
                | Self::ImportFrom
                | Self::Global
                | Self::Nonlocal
                | Self::ExpressionStatement
                | Self::Pass
                | Self::Break
                | Self::Continue
        )
    }

    /// Whether this kind is an expression: something that has a value.
    #[must_use]
    pub fn is_expression(self) -> bool {
        matches!(
            self,
            Self::BooleanOperation
                | Self::NamedExpression
                | Self::BinaryOperation
                | Self::UnaryOperation
                | Self::Lambda
                | Self::Conditional
                | Self::Dictionary
                | Self::Set
                | Self::ListComprehension
                | Self::SetComprehension
                | Self::DictionaryComprehension
                | Self::Generator
                | Self::Await
                | Self::Yield
                | Self::YieldFrom
                | Self::Compare
                | Self::Call
                | Self::Starred
                | Self::FormattedString
                | Self::TemplateString
                | Self::StringLiteral
                | Self::BytesLiteral
                | Self::Integer
                | Self::Float
                | Self::Imaginary
                | Self::True
                | Self::False
                | Self::None
                | Self::Ellipsis
                | Self::Attribute
                | Self::Subscript
                | Self::Name
                | Self::List
                | Self::Tuple
                | Self::Slice
                | Self::Missing
        )
    }
}
