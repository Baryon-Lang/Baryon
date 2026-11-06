//! Lowering statements: everything that can appear in a block.

use crate::lower::Lowerer;
use ruff_python_ast as ast;
use ruff_text_size::Ranged;
use syntax::{Flags, Role, SyntaxKind};

impl Lowerer<'_> {
    /// Adds each statement of a block, all with the same role.
    pub(crate) fn statements(&mut self, statements: &[ast::Stmt], role: Role) {
        for statement in statements {
            self.statement(statement, role);
        }
    }

    /// Adds one statement.
    fn statement(&mut self, statement: &ast::Stmt, role: Role) {
        let range = statement.range();
        match statement {
            ast::Stmt::FunctionDef(function) => self.function_definition(function, role),
            ast::Stmt::ClassDef(class) => self.class_definition(class, role),
            ast::Stmt::Return(statement) => self.node(SyntaxKind::Return, role, range, |this| {
                this.optional_expression(statement.value.as_deref(), Role::Value);
            }),
            ast::Stmt::Delete(statement) => self.node(SyntaxKind::Delete, role, range, |this| {
                this.expressions(&statement.targets, Role::Target);
            }),
            ast::Stmt::TypeAlias(alias) => self.node(SyntaxKind::TypeAlias, role, range, |this| {
                this.expression(&alias.name, Role::Name);
                this.type_parameters(alias.type_params.as_deref());
                this.expression(&alias.value, Role::Value);
            }),
            ast::Stmt::Assign(assign) => self.node(SyntaxKind::Assign, role, range, |this| {
                this.expressions(&assign.targets, Role::Target);
                this.expression(&assign.value, Role::Value);
            }),
            ast::Stmt::AugAssign(assign) => {
                self.node(SyntaxKind::AugmentedAssign, role, range, |this| {
                    this.builder.set_operator(Self::binary_operator(assign.op));
                    this.expression(&assign.target, Role::Target);
                    this.expression(&assign.value, Role::Value);
                });
            }
            ast::Stmt::AnnAssign(assign) => {
                self.node(SyntaxKind::AnnotatedAssign, role, range, |this| {
                    this.expression(&assign.target, Role::Target);
                    this.expression(&assign.annotation, Role::Annotation);
                    this.optional_expression(assign.value.as_deref(), Role::Value);
                });
            }
            ast::Stmt::For(statement) => self.for_loop(statement, role),
            ast::Stmt::While(statement) => self.while_loop(statement, role),
            ast::Stmt::If(statement) => self.if_statement(statement, role),
            ast::Stmt::With(statement) => self.with_statement(statement, role),
            ast::Stmt::Match(statement) => self.match_statement(statement, role),
            ast::Stmt::Raise(statement) => self.node(SyntaxKind::Raise, role, range, |this| {
                this.optional_expression(statement.exc.as_deref(), Role::Value);
                this.optional_expression(statement.cause.as_deref(), Role::Cause);
            }),
            ast::Stmt::Try(statement) => self.try_statement(statement, role),
            ast::Stmt::Assert(statement) => self.node(SyntaxKind::Assert, role, range, |this| {
                this.expression(&statement.test, Role::Condition);
                this.optional_expression(statement.msg.as_deref(), Role::Message);
            }),
            ast::Stmt::Import(statement) => self.node(SyntaxKind::Import, role, range, |this| {
                this.aliases(&statement.names);
            }),
            ast::Stmt::ImportFrom(statement) => {
                self.node(SyntaxKind::ImportFrom, role, range, |this| {
                    this.optional_identifier(statement.module.as_ref(), Role::Module);
                    this.aliases(&statement.names);
                });
            }
            ast::Stmt::Global(statement) => self.node(SyntaxKind::Global, role, range, |this| {
                for name in &statement.names {
                    this.identifier(name, Role::Item);
                }
            }),
            ast::Stmt::Nonlocal(statement) => {
                self.node(SyntaxKind::Nonlocal, role, range, |this| {
                    for name in &statement.names {
                        this.identifier(name, Role::Item);
                    }
                });
            }
            ast::Stmt::Expr(statement) => {
                self.node(SyntaxKind::ExpressionStatement, role, range, |this| {
                    this.expression(&statement.value, Role::Value);
                });
            }
            ast::Stmt::Pass(_) => self.leaf(SyntaxKind::Pass, role, range),
            ast::Stmt::Break(_) => self.leaf(SyntaxKind::Break, role, range),
            ast::Stmt::Continue(_) => self.leaf(SyntaxKind::Continue, role, range),
            // IPython escape commands such as `%time` exist only in Ruff's notebook mode, never
            // in the module mode this crate parses with.
            ast::Stmt::IpyEscapeCommand(_) => self.leaf(SyntaxKind::Missing, role, range),
        }
    }

    /// Adds `def`: decorators, name, type parameters, parameters, return type, body.
    fn function_definition(&mut self, function: &ast::StmtFunctionDef, role: Role) {
        let range = function.range();
        self.node(SyntaxKind::FunctionDefinition, role, range, |this| {
            if function.is_async {
                this.builder.set_flags(Flags::ASYNC);
            }
            this.decorators(&function.decorator_list);
            this.identifier(&function.name, Role::Name);
            this.type_parameters(function.type_params.as_deref());
            this.parameters(&function.parameters);
            this.optional_expression(function.returns.as_deref(), Role::Returns);
            this.statements(&function.body, Role::Body);
        });
    }

    /// Adds `class`: decorators, name, type parameters, bases, body.
    fn class_definition(&mut self, class: &ast::StmtClassDef, role: Role) {
        let range = class.range();
        self.node(SyntaxKind::ClassDefinition, role, range, |this| {
            this.decorators(&class.decorator_list);
            this.identifier(&class.name, Role::Name);
            this.type_parameters(class.type_params.as_deref());
            if let Some(arguments) = &class.arguments {
                this.arguments(arguments);
            }
            this.statements(&class.body, Role::Body);
        });
    }

    /// Adds a `for` loop and its `else:` block.
    fn for_loop(&mut self, statement: &ast::StmtFor, role: Role) {
        let range = statement.range();
        self.node(SyntaxKind::For, role, range, |this| {
            if statement.is_async {
                this.builder.set_flags(Flags::ASYNC);
            }
            this.expression(&statement.target, Role::Target);
            this.expression(&statement.iter, Role::Iterator);
            this.statements(&statement.body, Role::Body);
            this.statements(&statement.orelse, Role::Else);
        });
    }

    /// Adds a `while` loop and its `else:` block.
    fn while_loop(&mut self, statement: &ast::StmtWhile, role: Role) {
        let range = statement.range();
        self.node(SyntaxKind::While, role, range, |this| {
            this.expression(&statement.test, Role::Condition);
            this.statements(&statement.body, Role::Body);
            this.statements(&statement.orelse, Role::Else);
        });
    }

    /// Adds an `if` with its `elif` and `else` clauses.
    fn if_statement(&mut self, statement: &ast::StmtIf, role: Role) {
        let range = statement.range();
        self.node(SyntaxKind::If, role, range, |this| {
            this.expression(&statement.test, Role::Condition);
            this.statements(&statement.body, Role::Body);
            for clause in &statement.elif_else_clauses {
                this.elif_else_clause(clause);
            }
        });
    }

    /// Adds a `with` statement and its items.
    fn with_statement(&mut self, statement: &ast::StmtWith, role: Role) {
        let range = statement.range();
        self.node(SyntaxKind::With, role, range, |this| {
            if statement.is_async {
                this.builder.set_flags(Flags::ASYNC);
            }
            for item in &statement.items {
                this.node(SyntaxKind::WithItem, Role::Item, item.range(), |this| {
                    this.expression(&item.context_expr, Role::Value);
                    this.optional_expression(item.optional_vars.as_deref(), Role::Target);
                });
            }
            this.statements(&statement.body, Role::Body);
        });
    }

    /// Adds a `match` statement and its cases.
    fn match_statement(&mut self, statement: &ast::StmtMatch, role: Role) {
        let range = statement.range();
        self.node(SyntaxKind::Match, role, range, |this| {
            this.expression(&statement.subject, Role::Value);
            for case in &statement.cases {
                this.node(SyntaxKind::MatchCase, Role::Case, case.range(), |this| {
                    this.pattern(&case.pattern, Role::Pattern);
                    this.optional_expression(case.guard.as_deref(), Role::Condition);
                    this.statements(&case.body, Role::Body);
                });
            }
        });
    }

    /// Adds a `try` statement with its handlers, `else:` and `finally:` blocks.
    fn try_statement(&mut self, statement: &ast::StmtTry, role: Role) {
        let range = statement.range();
        self.node(SyntaxKind::Try, role, range, |this| {
            if statement.is_star {
                this.builder.set_flags(Flags::STAR);
            }
            this.statements(&statement.body, Role::Body);
            for ast::ExceptHandler::ExceptHandler(handler) in &statement.handlers {
                this.node(
                    SyntaxKind::ExceptHandler,
                    Role::Handler,
                    handler.range(),
                    |this| {
                        this.optional_expression(handler.type_.as_deref(), Role::Type);
                        this.optional_identifier(handler.name.as_ref(), Role::Name);
                        this.statements(&handler.body, Role::Body);
                    },
                );
            }
            this.statements(&statement.orelse, Role::Else);
            this.statements(&statement.finalbody, Role::Finally);
        });
    }

    /// Adds an `elif` or `else` clause of an `if`.
    fn elif_else_clause(&mut self, clause: &ast::ElifElseClause) {
        let kind = if clause.test.is_some() {
            SyntaxKind::ElifClause
        } else {
            SyntaxKind::ElseClause
        };
        self.node(kind, Role::Else, clause.range(), |this| {
            this.optional_expression(clause.test.as_ref(), Role::Condition);
            this.statements(&clause.body, Role::Body);
        });
    }

    /// Adds an `Alias` node for each name of an import.
    fn aliases(&mut self, aliases: &[ast::Alias]) {
        for alias in aliases {
            self.node(SyntaxKind::Alias, Role::Item, alias.range(), |this| {
                this.identifier(&alias.name, Role::Name);
                this.optional_identifier(alias.asname.as_ref(), Role::Alias);
            });
        }
    }
}
