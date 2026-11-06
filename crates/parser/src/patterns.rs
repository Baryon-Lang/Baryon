//! Lowering patterns: the left-hand side of a `case` in a `match` statement.

use crate::lower::Lowerer;
use ruff_python_ast as ast;
use ruff_text_size::Ranged;
use syntax::{Role, SyntaxKind};

impl Lowerer<'_> {
    /// Adds one pattern.
    pub(crate) fn pattern(&mut self, pattern: &ast::Pattern, role: Role) {
        let range = pattern.range();
        match pattern {
            ast::Pattern::MatchValue(value) => {
                self.node(SyntaxKind::PatternValue, role, range, |this| {
                    this.expression(&value.value, Role::Value);
                });
            }
            ast::Pattern::MatchSingleton(_) => self.leaf(SyntaxKind::PatternSingleton, role, range),
            ast::Pattern::MatchSequence(sequence) => {
                self.node(SyntaxKind::PatternSequence, role, range, |this| {
                    this.patterns(&sequence.patterns, Role::Item);
                });
            }
            ast::Pattern::MatchMapping(mapping) => {
                self.node(SyntaxKind::PatternMapping, role, range, |this| {
                    for (key, pattern) in mapping.keys.iter().zip(&mapping.patterns) {
                        this.expression(key, Role::Key);
                        this.pattern(pattern, Role::Pattern);
                    }
                    this.optional_identifier(mapping.rest.as_ref(), Role::Rest);
                });
            }
            ast::Pattern::MatchClass(class) => {
                self.node(SyntaxKind::PatternClass, role, range, |this| {
                    this.expression(&class.cls, Role::Class);
                    for argument in class.arguments.iter_source_order() {
                        match argument {
                            ast::PatternOrKeyword::Pattern(pattern) => {
                                this.pattern(pattern, Role::Item);
                            }
                            ast::PatternOrKeyword::Keyword(keyword) => {
                                this.pattern_keyword(keyword);
                            }
                        }
                    }
                });
            }
            ast::Pattern::MatchStar(star) => {
                self.node(SyntaxKind::PatternStar, role, range, |this| {
                    this.optional_identifier(star.name.as_ref(), Role::Name);
                });
            }
            ast::Pattern::MatchAs(capture) => {
                self.node(SyntaxKind::PatternAs, role, range, |this| {
                    if let Some(pattern) = &capture.pattern {
                        this.pattern(pattern, Role::Pattern);
                    }
                    this.optional_identifier(capture.name.as_ref(), Role::Name);
                });
            }
            ast::Pattern::MatchOr(alternatives) => {
                self.node(SyntaxKind::PatternOr, role, range, |this| {
                    this.patterns(&alternatives.patterns, Role::Item);
                });
            }
        }
    }

    /// Adds `name=pattern` in a class pattern.
    fn pattern_keyword(&mut self, keyword: &ast::PatternKeyword) {
        self.node(
            SyntaxKind::PatternKeyword,
            Role::Item,
            keyword.range(),
            |this| {
                this.identifier(&keyword.attr, Role::Name);
                this.pattern(&keyword.pattern, Role::Pattern);
            },
        );
    }

    /// Adds each pattern, all with the same role.
    fn patterns(&mut self, patterns: &[ast::Pattern], role: Role) {
        for pattern in patterns {
            self.pattern(pattern, role);
        }
    }
}
