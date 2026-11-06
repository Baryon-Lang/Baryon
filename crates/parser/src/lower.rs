//! Lowering: one walk over Ruff's tree that writes the flat [`SyntaxTree`].
//!
//! The walk is split by what it lowers: this module holds the shared machinery and the pieces
//! of definitions (parameters, type parameters, arguments), and `statements`, `expressions` and
//! `patterns` each add the methods for their part of the grammar. Every method maps one Ruff
//! node to one node of the kind its documentation in `syntax` promises, children in source
//! order with their roles.

use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange};
use source::FileId;
use syntax::{Role, SyntaxKind, SyntaxTree, TreeBuilder};

/// Lowers a parsed module into a syntax tree whose root spans all of `text`.
pub(crate) fn module(file: FileId, text: &str, module: &ast::ModModule) -> SyntaxTree {
    let mut lowerer = Lowerer::new(file, text);
    lowerer
        .builder
        .open(SyntaxKind::Module, Role::Root, 0, end_of(text));
    lowerer.statements(&module.body, Role::Body);
    lowerer.builder.close();
    lowerer.builder.finish()
}

/// A tree holding only a module node that spans all of `text`.
pub(crate) fn empty(file: FileId, text: &str) -> SyntaxTree {
    let mut builder = TreeBuilder::new(file);
    builder.leaf(SyntaxKind::Module, Role::Root, 0, end_of(text));
    builder.finish()
}

/// Byte offset of the end of `text`. Source files are limited to 4 GiB, so it fits.
fn end_of(text: &str) -> u32 {
    u32::try_from(text.len()).unwrap_or(u32::MAX)
}

/// The state of one walk: the tree being built and the text it describes.
pub(crate) struct Lowerer<'a> {
    /// Receives nodes in preorder.
    pub(crate) builder: TreeBuilder,
    /// The source text, for the few nodes whose range Ruff does not record.
    pub(crate) text: &'a str,
}

impl<'a> Lowerer<'a> {
    /// Starts a walk for `file`, whose contents are `text`.
    fn new(file: FileId, text: &'a str) -> Self {
        Self {
            builder: TreeBuilder::new(file),
            text,
        }
    }

    /// Adds a node of `kind` spanning `range`, with the children `children` adds.
    pub(crate) fn node(
        &mut self,
        kind: SyntaxKind,
        role: Role,
        range: TextRange,
        children: impl FnOnce(&mut Self),
    ) {
        self.builder
            .open(kind, role, range.start().into(), range.end().into());
        children(self);
        self.builder.close();
    }

    /// Adds a leaf of `kind` spanning `range`.
    pub(crate) fn leaf(&mut self, kind: SyntaxKind, role: Role, range: TextRange) {
        self.builder
            .leaf(kind, role, range.start().into(), range.end().into());
    }

    /// Adds a `Name` leaf for an identifier.
    pub(crate) fn identifier(&mut self, identifier: &ast::Identifier, role: Role) {
        self.leaf(SyntaxKind::Name, role, identifier.range());
    }

    /// Adds a `Name` leaf for an identifier, if there is one.
    pub(crate) fn optional_identifier(&mut self, identifier: Option<&ast::Identifier>, role: Role) {
        if let Some(identifier) = identifier {
            self.identifier(identifier, role);
        }
    }

    /// Adds a `Decorator` node for each decorator.
    pub(crate) fn decorators(&mut self, decorators: &[ast::Decorator]) {
        for decorator in decorators {
            self.node(
                SyntaxKind::Decorator,
                Role::Decorator,
                decorator.range(),
                |this| {
                    this.expression(&decorator.expression, Role::Value);
                },
            );
        }
    }

    /// Adds a `TypeParameters` node, if the definition has type parameters.
    pub(crate) fn type_parameters(&mut self, parameters: Option<&ast::TypeParams>) {
        let Some(parameters) = parameters else {
            return;
        };
        self.node(
            SyntaxKind::TypeParameters,
            Role::TypeParameters,
            parameters.range(),
            |this| {
                for parameter in &parameters.type_params {
                    this.type_parameter(parameter);
                }
            },
        );
    }

    /// Adds one type parameter: `T: Bound = Default`, `*Ts` or `**P`.
    fn type_parameter(&mut self, parameter: &ast::TypeParam) {
        match parameter {
            ast::TypeParam::TypeVar(variable) => {
                self.node(
                    SyntaxKind::TypeVariable,
                    Role::Item,
                    variable.range(),
                    |this| {
                        this.identifier(&variable.name, Role::Name);
                        this.optional_expression(variable.bound.as_deref(), Role::Bound);
                        this.optional_expression(variable.default.as_deref(), Role::Default);
                    },
                );
            }
            ast::TypeParam::TypeVarTuple(tuple) => {
                self.node(
                    SyntaxKind::TypeVariableTuple,
                    Role::Item,
                    tuple.range(),
                    |this| {
                        this.identifier(&tuple.name, Role::Name);
                        this.optional_expression(tuple.default.as_deref(), Role::Default);
                    },
                );
            }
            ast::TypeParam::ParamSpec(specification) => {
                self.node(
                    SyntaxKind::ParameterSpecification,
                    Role::Item,
                    specification.range(),
                    |this| {
                        this.identifier(&specification.name, Role::Name);
                        this.optional_expression(specification.default.as_deref(), Role::Default);
                    },
                );
            }
        }
    }

    /// Adds a `Parameters` node. Python fixes the order: positional-only, ordinary, `*args`,
    /// keyword-only, `**kwargs`, which is also Ruff's field order.
    pub(crate) fn parameters(&mut self, parameters: &ast::Parameters) {
        self.node(
            SyntaxKind::Parameters,
            Role::Parameters,
            parameters.range(),
            |this| {
                for parameter in &parameters.posonlyargs {
                    this.parameter_with_default(parameter, Role::PositionalOnly);
                }
                for parameter in &parameters.args {
                    this.parameter_with_default(parameter, Role::Item);
                }
                if let Some(parameter) = &parameters.vararg {
                    this.variadic_parameter(SyntaxKind::StarParameter, parameter);
                }
                for parameter in &parameters.kwonlyargs {
                    this.parameter_with_default(parameter, Role::KeywordOnly);
                }
                if let Some(parameter) = &parameters.kwarg {
                    this.variadic_parameter(SyntaxKind::DoubleStarParameter, parameter);
                }
            },
        );
    }

    /// Adds an ordinary `Parameter`: name, annotation and default.
    fn parameter_with_default(&mut self, parameter: &ast::ParameterWithDefault, role: Role) {
        self.node(SyntaxKind::Parameter, role, parameter.range(), |this| {
            this.identifier(&parameter.parameter.name, Role::Name);
            this.optional_expression(parameter.parameter.annotation.as_deref(), Role::Annotation);
            this.optional_expression(parameter.default.as_deref(), Role::Default);
        });
    }

    /// Adds a `*args` or `**kwargs` parameter, which takes no default.
    fn variadic_parameter(&mut self, kind: SyntaxKind, parameter: &ast::Parameter) {
        self.node(kind, Role::Item, parameter.range(), |this| {
            this.identifier(&parameter.name, Role::Name);
            this.optional_expression(parameter.annotation.as_deref(), Role::Annotation);
        });
    }

    /// Adds an `Arguments` node with positional and keyword arguments in source order.
    pub(crate) fn arguments(&mut self, arguments: &ast::Arguments) {
        self.node(
            SyntaxKind::Arguments,
            Role::Arguments,
            arguments.range(),
            |this| {
                for argument in arguments.iter_source_order() {
                    match argument {
                        ast::ArgOrKeyword::Arg(expression) => {
                            this.expression(expression, Role::Item);
                        }
                        ast::ArgOrKeyword::Keyword(keyword) => this.keyword(keyword),
                    }
                }
            },
        );
    }

    /// Adds `name=value` as a `Keyword`, or `**value` as a `DoubleStarred`.
    fn keyword(&mut self, keyword: &ast::Keyword) {
        match &keyword.arg {
            Some(name) => self.node(SyntaxKind::Keyword, Role::Item, keyword.range(), |this| {
                this.identifier(name, Role::Name);
                this.expression(&keyword.value, Role::Value);
            }),
            None => self.node(
                SyntaxKind::DoubleStarred,
                Role::Item,
                keyword.range(),
                |this| {
                    this.expression(&keyword.value, Role::Value);
                },
            ),
        }
    }
}
