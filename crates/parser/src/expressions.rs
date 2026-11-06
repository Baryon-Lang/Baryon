//! Lowering expressions: everything that has a value.

use crate::lower::Lowerer;
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange, TextSize};
use syntax::{Flags, Operator, Role, SyntaxKind};

impl Lowerer<'_> {
    /// Adds each expression, all with the same role.
    pub(crate) fn expressions(&mut self, expressions: &[ast::Expr], role: Role) {
        for expression in expressions {
            self.expression(expression, role);
        }
    }

    /// Adds an expression, if there is one.
    pub(crate) fn optional_expression(&mut self, expression: Option<&ast::Expr>, role: Role) {
        if let Some(expression) = expression {
            self.expression(expression, role);
        }
    }

    /// Adds one expression.
    pub(crate) fn expression(&mut self, expression: &ast::Expr, role: Role) {
        let range = expression.range();
        match expression {
            ast::Expr::BoolOp(operation) => self.boolean_operation(operation, role),
            ast::Expr::Named(named) => {
                self.node(SyntaxKind::NamedExpression, role, range, |this| {
                    this.expression(&named.target, Role::Target);
                    this.expression(&named.value, Role::Value);
                });
            }
            ast::Expr::BinOp(operation) => {
                self.node(SyntaxKind::BinaryOperation, role, range, |this| {
                    this.builder
                        .set_operator(Self::binary_operator(operation.op));
                    this.expression(&operation.left, Role::Left);
                    this.expression(&operation.right, Role::Right);
                });
            }
            ast::Expr::UnaryOp(operation) => self.unary_operation(operation, role),
            ast::Expr::Lambda(lambda) => self.lambda(lambda, role),
            ast::Expr::If(conditional) => {
                self.node(SyntaxKind::Conditional, role, range, |this| {
                    this.expression(&conditional.body, Role::Value);
                    this.expression(&conditional.test, Role::Condition);
                    this.expression(&conditional.orelse, Role::Else);
                });
            }
            ast::Expr::Dict(dictionary) => self.dictionary(dictionary, role),
            ast::Expr::Set(set) => self.node(SyntaxKind::Set, role, range, |this| {
                this.expressions(&set.elts, Role::Item);
            }),
            ast::Expr::ListComp(comprehension) => self.list_comprehension(comprehension, role),
            ast::Expr::SetComp(comprehension) => self.set_comprehension(comprehension, role),
            ast::Expr::DictComp(comprehension) => {
                self.dictionary_comprehension(comprehension, role);
            }
            ast::Expr::Generator(generator) => self.generator(generator, role),
            ast::Expr::Await(operation) => self.node(SyntaxKind::Await, role, range, |this| {
                this.expression(&operation.value, Role::Value);
            }),
            ast::Expr::Yield(operation) => self.node(SyntaxKind::Yield, role, range, |this| {
                this.optional_expression(operation.value.as_deref(), Role::Value);
            }),
            ast::Expr::YieldFrom(operation) => {
                self.node(SyntaxKind::YieldFrom, role, range, |this| {
                    this.expression(&operation.value, Role::Value);
                });
            }
            ast::Expr::Compare(comparison) => self.comparison(comparison, role),
            ast::Expr::Call(call) => self.node(SyntaxKind::Call, role, range, |this| {
                this.expression(&call.func, Role::Function);
                this.arguments(&call.arguments);
            }),
            ast::Expr::FString(string) => self.formatted_string(string, role),
            ast::Expr::TString(string) => self.template_string(string, role),
            ast::Expr::StringLiteral(_) => self.leaf(SyntaxKind::StringLiteral, role, range),
            ast::Expr::BytesLiteral(_) => self.leaf(SyntaxKind::BytesLiteral, role, range),
            ast::Expr::NumberLiteral(number) => self.number(number, role),
            ast::Expr::BooleanLiteral(boolean) => self.boolean(boolean, role),
            ast::Expr::NoneLiteral(_) => self.leaf(SyntaxKind::None, role, range),
            ast::Expr::EllipsisLiteral(_) => self.leaf(SyntaxKind::Ellipsis, role, range),
            ast::Expr::Attribute(attribute) => {
                self.node(SyntaxKind::Attribute, role, range, |this| {
                    this.expression(&attribute.value, Role::Value);
                    this.identifier(&attribute.attr, Role::Attribute);
                });
            }
            ast::Expr::Subscript(subscript) => {
                self.node(SyntaxKind::Subscript, role, range, |this| {
                    this.expression(&subscript.value, Role::Value);
                    this.expression(&subscript.slice, Role::Index);
                });
            }
            ast::Expr::Starred(starred) => self.node(SyntaxKind::Starred, role, range, |this| {
                this.expression(&starred.value, Role::Value);
            }),
            // Ruff stands an empty name with an invalid context in for an expression it expected
            // and did not find.
            ast::Expr::Name(name) if name.ctx == ast::ExprContext::Invalid => {
                self.leaf(SyntaxKind::Missing, role, range);
            }
            ast::Expr::Name(_) => self.leaf(SyntaxKind::Name, role, range),
            ast::Expr::List(list) => self.node(SyntaxKind::List, role, range, |this| {
                this.expressions(&list.elts, Role::Item);
            }),
            ast::Expr::Tuple(tuple) => self.node(SyntaxKind::Tuple, role, range, |this| {
                if tuple.parenthesized {
                    this.builder.set_flags(Flags::PARENTHESIZED);
                }
                this.expressions(&tuple.elts, Role::Item);
            }),
            ast::Expr::Slice(slice) => self.node(SyntaxKind::Slice, role, range, |this| {
                this.optional_expression(slice.lower.as_deref(), Role::Lower);
                this.optional_expression(slice.upper.as_deref(), Role::Upper);
                this.optional_expression(slice.step.as_deref(), Role::Step);
            }),
            // Notebook-only syntax, as for statements: never produced in module mode.
            ast::Expr::IpyEscapeCommand(_) => self.leaf(SyntaxKind::Missing, role, range),
        }
    }

    /// Adds `a and b` or `a or b`.
    fn boolean_operation(&mut self, operation: &ast::ExprBoolOp, role: Role) {
        let range = operation.range();
        self.node(SyntaxKind::BooleanOperation, role, range, |this| {
            this.builder.set_operator(match operation.op {
                ast::BoolOp::And => Operator::And,
                ast::BoolOp::Or => Operator::Or,
            });
            this.expressions(&operation.values, Role::Item);
        });
    }

    /// Adds `-x`, `+x`, `~x` or `not x`.
    fn unary_operation(&mut self, operation: &ast::ExprUnaryOp, role: Role) {
        let range = operation.range();
        self.node(SyntaxKind::UnaryOperation, role, range, |this| {
            this.builder.set_operator(match operation.op {
                ast::UnaryOp::Invert => Operator::Invert,
                ast::UnaryOp::Not => Operator::Not,
                ast::UnaryOp::UAdd => Operator::Plus,
                ast::UnaryOp::USub => Operator::Negate,
            });
            this.expression(&operation.operand, Role::Value);
        });
    }

    /// Adds `lambda parameters: body`.
    fn lambda(&mut self, lambda: &ast::ExprLambda, role: Role) {
        let range = lambda.range();
        self.node(SyntaxKind::Lambda, role, range, |this| {
            if let Some(parameters) = &lambda.parameters {
                this.parameters(parameters);
            }
            this.expression(&lambda.body, Role::Body);
        });
    }

    /// Adds a dictionary display.
    fn dictionary(&mut self, dictionary: &ast::ExprDict, role: Role) {
        let range = dictionary.range();
        self.node(SyntaxKind::Dictionary, role, range, |this| {
            for item in &dictionary.items {
                this.dictionary_item(item);
            }
        });
    }

    /// Adds `[value for ...]`.
    fn list_comprehension(&mut self, comprehension: &ast::ExprListComp, role: Role) {
        let range = comprehension.range();
        self.node(SyntaxKind::ListComprehension, role, range, |this| {
            this.expression(&comprehension.elt, Role::Value);
            this.comprehensions(&comprehension.generators);
        });
    }

    /// Adds `{value for ...}`.
    fn set_comprehension(&mut self, comprehension: &ast::ExprSetComp, role: Role) {
        let range = comprehension.range();
        self.node(SyntaxKind::SetComprehension, role, range, |this| {
            this.expression(&comprehension.elt, Role::Value);
            this.comprehensions(&comprehension.generators);
        });
    }

    /// Adds `{key: value for ...}`.
    fn dictionary_comprehension(&mut self, comprehension: &ast::ExprDictComp, role: Role) {
        let range = comprehension.range();
        self.node(SyntaxKind::DictionaryComprehension, role, range, |this| {
            this.optional_expression(comprehension.key.as_deref(), Role::Key);
            this.expression(&comprehension.value, Role::Value);
            this.comprehensions(&comprehension.generators);
        });
    }

    /// Adds `(value for ...)`.
    fn generator(&mut self, generator: &ast::ExprGenerator, role: Role) {
        let range = generator.range();
        self.node(SyntaxKind::Generator, role, range, |this| {
            if generator.parenthesized {
                this.builder.set_flags(Flags::PARENTHESIZED);
            }
            this.expression(&generator.elt, Role::Value);
            this.comprehensions(&generator.generators);
        });
    }

    /// Adds an f-string and any plain strings implicitly joined to it.
    fn formatted_string(&mut self, string: &ast::ExprFString, role: Role) {
        let range = string.range();
        self.node(SyntaxKind::FormattedString, role, range, |this| {
            for part in &string.value {
                match part {
                    ast::FStringPartRef::Literal(literal) => {
                        this.leaf(SyntaxKind::StringLiteral, Role::Item, literal.range());
                    }
                    ast::FStringPartRef::FString(string) => {
                        this.interpolated_elements(&string.elements);
                    }
                }
            }
        });
    }

    /// Adds a t-string.
    fn template_string(&mut self, string: &ast::ExprTString, role: Role) {
        let range = string.range();
        self.node(SyntaxKind::TemplateString, role, range, |this| {
            for part in &string.value {
                this.interpolated_elements(&part.elements);
            }
        });
    }

    /// Adds an integer, float or imaginary literal.
    fn number(&mut self, number: &ast::ExprNumberLiteral, role: Role) {
        let range = number.range();
        let kind = match number.value {
            ast::Number::Int(_) => SyntaxKind::Integer,
            ast::Number::Float(_) => SyntaxKind::Float,
            ast::Number::Complex { .. } => SyntaxKind::Imaginary,
        };
        self.leaf(kind, role, range);
    }

    /// Adds `True` or `False`.
    fn boolean(&mut self, boolean: &ast::ExprBooleanLiteral, role: Role) {
        let range = boolean.range();
        let kind = if boolean.value {
            SyntaxKind::True
        } else {
            SyntaxKind::False
        };
        self.leaf(kind, role, range);
    }

    /// The operator of a binary operation or an augmented assignment.
    pub(crate) fn binary_operator(operator: ast::Operator) -> Operator {
        match operator {
            ast::Operator::Add => Operator::Add,
            ast::Operator::Sub => Operator::Subtract,
            ast::Operator::Mult => Operator::Multiply,
            ast::Operator::MatMult => Operator::MatrixMultiply,
            ast::Operator::Div => Operator::Divide,
            ast::Operator::Mod => Operator::Modulo,
            ast::Operator::Pow => Operator::Power,
            ast::Operator::LShift => Operator::LeftShift,
            ast::Operator::RShift => Operator::RightShift,
            ast::Operator::BitOr => Operator::BitOr,
            ast::Operator::BitXor => Operator::BitXor,
            ast::Operator::BitAnd => Operator::BitAnd,
            ast::Operator::FloorDiv => Operator::FloorDivide,
        }
    }

    /// Adds a comparison chain: the first operand, then one `Comparator` per operator.
    ///
    /// Ruff records no range for an operator, so each comparator starts where the operand
    /// before it ends, which puts the operator inside its span.
    fn comparison(&mut self, comparison: &ast::ExprCompare, role: Role) {
        self.node(SyntaxKind::Compare, role, comparison.range(), |this| {
            let Some((left, comparators)) = comparison.operands.split_first() else {
                return;
            };
            let mut previous_end = left.end();
            this.expression(left, Role::Left);
            for (operator, operand) in comparison.ops.iter().zip(comparators) {
                let range = TextRange::new(previous_end, operand.end());
                this.node(SyntaxKind::Comparator, Role::Item, range, |this| {
                    this.builder.set_operator(match operator {
                        ast::CmpOp::Eq => Operator::Equal,
                        ast::CmpOp::NotEq => Operator::NotEqual,
                        ast::CmpOp::Lt => Operator::Less,
                        ast::CmpOp::LtE => Operator::LessEqual,
                        ast::CmpOp::Gt => Operator::Greater,
                        ast::CmpOp::GtE => Operator::GreaterEqual,
                        ast::CmpOp::Is => Operator::Is,
                        ast::CmpOp::IsNot => Operator::IsNot,
                        ast::CmpOp::In => Operator::In,
                        ast::CmpOp::NotIn => Operator::NotIn,
                    });
                    this.expression(operand, Role::Right);
                });
                previous_end = operand.end();
            }
        });
    }

    /// Adds `key: value` as a `DictionaryEntry`, or `**value` as a `DoubleStarred`.
    ///
    /// Ruff records no range for either, so an entry spans its key and value, and an unpacking
    /// starts at the `**` before its value.
    fn dictionary_item(&mut self, item: &ast::DictItem) {
        if let Some(key) = &item.key {
            let range = TextRange::new(key.start(), item.value.end());
            self.node(SyntaxKind::DictionaryEntry, Role::Item, range, |this| {
                this.expression(key, Role::Key);
                this.expression(&item.value, Role::Value);
            });
        } else {
            let value_start = item.value.start();
            let start = self
                .text
                .get(..usize::from(value_start))
                .and_then(|before| before.rfind("**"))
                .and_then(|offset| u32::try_from(offset).ok())
                .map_or(value_start, TextSize::new);
            let range = TextRange::new(start, item.value.end());
            self.node(SyntaxKind::DoubleStarred, Role::Item, range, |this| {
                this.expression(&item.value, Role::Value);
            });
        }
    }

    /// Adds a `Comprehension` node for each `for` clause of a comprehension.
    fn comprehensions(&mut self, comprehensions: &[ast::Comprehension]) {
        for comprehension in comprehensions {
            self.node(
                SyntaxKind::Comprehension,
                Role::Item,
                comprehension.range(),
                |this| {
                    if comprehension.is_async {
                        this.builder.set_flags(Flags::ASYNC);
                    }
                    this.expression(&comprehension.target, Role::Target);
                    this.expression(&comprehension.iter, Role::Iterator);
                    this.expressions(&comprehension.ifs, Role::Condition);
                },
            );
        }
    }

    /// Adds the literal text and interpolations of an f-string, t-string or format
    /// specification.
    fn interpolated_elements(&mut self, elements: &ast::InterpolatedStringElements) {
        for element in elements {
            match element {
                ast::InterpolatedStringElement::Literal(literal) => {
                    self.leaf(SyntaxKind::FormattedText, Role::Item, literal.range());
                }
                ast::InterpolatedStringElement::Interpolation(interpolation) => {
                    self.interpolation(interpolation);
                }
            }
        }
    }

    /// Adds `{value=!r:format}` as an `Interpolation`.
    fn interpolation(&mut self, interpolation: &ast::InterpolatedElement) {
        self.node(
            SyntaxKind::Interpolation,
            Role::Item,
            interpolation.range(),
            |this| {
                if interpolation.debug_text.is_some() {
                    this.builder.set_flags(Flags::DEBUG);
                }
                match interpolation.conversion {
                    ast::ConversionFlag::None => {}
                    ast::ConversionFlag::Str => this.builder.set_flags(Flags::CONVERT_STR),
                    ast::ConversionFlag::Repr => this.builder.set_flags(Flags::CONVERT_REPR),
                    ast::ConversionFlag::Ascii => this.builder.set_flags(Flags::CONVERT_ASCII),
                }
                this.expression(&interpolation.expression, Role::Value);
                if let Some(format) = &interpolation.format_spec {
                    this.node(
                        SyntaxKind::FormatSpecification,
                        Role::Format,
                        format.range(),
                        |this| {
                            this.interpolated_elements(&format.elements);
                        },
                    );
                }
            },
        );
    }
}
