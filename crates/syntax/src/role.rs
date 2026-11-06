//! What part a node plays in its parent: the condition of an `if`, the default of a parameter.

/// The part a node plays in its parent.
///
/// A kind alone does not always say which child is which. A parameter's annotation and its
/// default are both expressions; so are a slice's three bounds. The role names the slot, like a
/// field name in a struct, so a reader can ask for [`SyntaxTree::child`](crate::SyntaxTree::child)
/// `(parameter, Role::Default)` instead of counting children. Each [`SyntaxKind`](crate::SyntaxKind)
/// lists the roles its children take.
///
/// ```
/// use syntax::Role;
///
/// assert_eq!(format!("{:?}", Role::Condition), "Condition");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Role {
    /// The root of the tree, which has no parent.
    Root,
    /// One of a list of like children: a tuple element, a call argument, an import alias.
    Item,
    /// A statement in the main body of a block, or a lambda's result expression.
    Body,
    /// A statement in an `else:` block, or the `else` branch of a conditional expression.
    /// Also the `elif` and `else` clauses of an `if` statement.
    Else,
    /// A statement in a `finally:` block.
    Finally,
    /// An `except` clause of a `try` statement.
    Handler,
    /// A `case` clause of a `match` statement.
    Case,
    /// The name being defined or referred to: a function, class, parameter or keyword name.
    Name,
    /// The new name after `as` in an import.
    Alias,
    /// The module in `from module import name`.
    Module,
    /// A decorator, written `@name` above a definition.
    Decorator,
    /// A definition's type parameters, written `[T: Bound]`.
    TypeParameters,
    /// A function's or lambda's parameter list.
    Parameters,
    /// A call's or class definition's argument list.
    Arguments,
    /// A parameter that can only be passed by position: one before `/`.
    PositionalOnly,
    /// A parameter that can only be passed by keyword: one after `*` or `*args`.
    KeywordOnly,
    /// What is assigned to, looped over into, or bound: the left of `=`, the variable of `for`.
    Target,
    /// The main expression of a node: an assigned value, a returned value, an operand.
    Value,
    /// The collection a `for` loop or comprehension iterates over.
    Iterator,
    /// A condition: of `if`, `while`, `elif`, `assert`, a comprehension filter or a case guard.
    Condition,
    /// A type annotation, written after `:`.
    Annotation,
    /// A function's return type, written after `->`.
    Returns,
    /// A default value, written after `=` on a parameter or type parameter.
    Default,
    /// A type parameter's bound, written after `:` in `[T: Bound]`.
    Bound,
    /// The exception type an `except` clause catches.
    Type,
    /// The function being called.
    Function,
    /// The class a class pattern matches.
    Class,
    /// The left operand of a binary operation or the first operand of a comparison.
    Left,
    /// The right operand of a binary operation or a comparator.
    Right,
    /// A dictionary key or mapping-pattern key.
    Key,
    /// The index inside `[]` of a subscript.
    Index,
    /// The attribute name after `.`.
    Attribute,
    /// A slice's start, before the first `:`.
    Lower,
    /// A slice's stop, after the first `:`.
    Upper,
    /// A slice's step, after the second `:`.
    Step,
    /// The cause of a `raise`, written after `from`.
    Cause,
    /// The message of an `assert`, written after `,`.
    Message,
    /// A sub-pattern of a pattern or `case`.
    Pattern,
    /// The name capturing the remaining entries of a mapping pattern, written `**rest`.
    Rest,
    /// A format specification of an f-string interpolation, written after `:`.
    Format,
}
