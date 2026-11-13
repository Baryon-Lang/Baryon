//! Statements: assignments, `return`, `raise`, `assert`, and the control flow of `if`. Loops
//! are in the `loops` module.

use crate::builder::{Builder, Loop};
use crate::constants::integer;
use crate::operators::arithmetic;
use database::Definition;
use mir::{
    Check, Constant, ConstantValue, Local, Operand, Place, Rvalue, StatementKind, TerminatorKind,
};
use syntax::{NodeId, Operator, Role, SyntaxKind};
use types::{Type, TypeKind};

impl<'db> Builder<'db> {
    /// Lowers the whole function: its statements, then the return at its end when control can
    /// reach it, which gives `None` in a function that returns nothing.
    pub(crate) fn function(&mut self) {
        self.block(self.function, Role::Body);
        if self.is_dead() {
            return;
        }
        let function = self.function;
        if matches!(self.returns.kind(self.db), TypeKind::None) {
            self.return_none(function);
            self.drop_all(function);
            self.terminate(TerminatorKind::Return, function);
        } else {
            // The checker proved every path returns, so control never gets here.
            self.terminate(TerminatorKind::Unreachable, function);
        }
    }

    /// Lowers every statement of `parent` with the role `role`, in order.
    pub(crate) fn block(&mut self, parent: NodeId, role: Role) {
        let statements: Vec<NodeId> = self.tree.children_with(parent, role).collect();
        for statement in statements {
            self.enter();
            self.statement(statement);
            self.leave(statement);
        }
    }

    /// Lowers one statement.
    fn statement(&mut self, statement: NodeId) {
        let child = |role| self.tree.child(statement, role);
        match self.tree.kind(statement) {
            SyntaxKind::Pass => {}
            SyntaxKind::ExpressionStatement => {
                // A string on its own is a docstring or a comment, and does nothing.
                if let Some(value) = child(Role::Value)
                    && self.tree.kind(value) != SyntaxKind::StringLiteral
                {
                    self.discard(value);
                }
            }
            SyntaxKind::Assign => {
                let targets: Vec<NodeId> =
                    self.tree.children_with(statement, Role::Target).collect();
                match (targets.as_slice(), child(Role::Value)) {
                    ([target], Some(value)) => self.store(*target, value),
                    _ => self.unsupported(statement, "assignments to several targets"),
                }
            }
            SyntaxKind::AnnotatedAssign => {
                if let (Some(target), Some(value)) = (child(Role::Target), child(Role::Value)) {
                    self.store(target, value);
                }
            }
            SyntaxKind::AugmentedAssign => self.augmented(statement),
            SyntaxKind::Return => {
                match child(Role::Value) {
                    Some(value) => self.write(Place::from(Local::RETURN), self.returns, value),
                    None => self.return_none(statement),
                }
                self.drop_all(statement);
                self.terminate(TerminatorKind::Return, statement);
            }
            SyntaxKind::Delete => self.dict_delete(statement),
            SyntaxKind::Raise => self.raise(statement),
            SyntaxKind::If => self.if_statement(statement),
            SyntaxKind::While => self.while_statement(statement),
            SyntaxKind::For => self.for_statement(statement),
            SyntaxKind::Break | SyntaxKind::Continue => {
                let Some(&Loop { next, end }) = self.loops.last() else {
                    return;
                };
                let target = if self.tree.kind(statement) == SyntaxKind::Break {
                    end
                } else {
                    next
                };
                self.terminate(TerminatorKind::Goto(target), statement);
            }
            SyntaxKind::Assert => {
                let Some(condition) = child(Role::Condition) else {
                    return;
                };
                // The message is not computed: the panic names the failed assertion, and the
                // message joins it once lowering handles strings at run time.
                let condition = self.condition(condition);
                self.then(statement, |target| TerminatorKind::Assert {
                    condition,
                    check: Check::Assertion,
                    target,
                });
            }
            SyntaxKind::Match => self.match_statement(statement),
            SyntaxKind::With => self.with_statement(statement),
            SyntaxKind::Try => self.try_statement(statement),
            _ => self.unsupported(statement, "this statement"),
        }
    }

    /// `with unsafe:`, which allows `a.unchecked[i]` in its body and otherwise runs the body as
    /// it is. Other `with` statements are not lowered yet.
    fn with_statement(&mut self, statement: NodeId) {
        let items: Vec<NodeId> = self.tree.children_with(statement, Role::Item).collect();
        let is_unsafe = match items.as_slice() {
            [item] => {
                self.tree.child(*item, Role::Target).is_none()
                    && self.tree.child(*item, Role::Value).is_some_and(|value| {
                        self.tree.kind(value) == SyntaxKind::Name && self.text_of(value) == "unsafe"
                    })
            }
            _ => false,
        };
        match items.as_slice() {
            _ if is_unsafe => self.block(statement, Role::Body),
            [item] if self.tree.child(*item, Role::Target).is_some() => {
                self.with_file(statement, *item);
            }
            _ => self.unsupported(
                statement,
                "`with` other than `with unsafe:` and `with open`",
            ),
        }
    }

    /// `_0 = None`, for a `return` without a value.
    pub(crate) fn return_none(&mut self, node: NodeId) {
        let nothing = Operand::Constant(Constant {
            ty: self.returns,
            value: ConstantValue::None,
        });
        self.assign(Place::from(Local::RETURN), Rvalue::Use(nothing), node);
    }

    /// `target = value`. The value is computed first, then the old value of a target that owns
    /// memory is dropped, then the new one moves in, so `x = f(x)` reads the old `x`.
    pub(crate) fn store(&mut self, target: NodeId, value: NodeId) {
        if self.tree.kind(target) == SyntaxKind::Tuple {
            return self.unpack(target, value);
        }
        if self.is_dict_subscript(target) {
            return self.dict_store(target, value);
        }
        if self.is_library_access(target) {
            return self.library_store(target);
        }
        if self.is_list_slice(target) {
            return self.list_slice_store(target, value);
        }
        if self.is_selection(target) {
            return self.selection_store(target, value);
        }
        if self.is_column(target) {
            return self.column_store(target, value);
        }
        let Some((place, ty)) = self.target(target) else {
            return;
        };
        if place.is_view() {
            return self.store_view(place, ty, value);
        }
        if !self.needs_drop(ty) {
            return self.write(place, ty, value);
        }
        let temporary = self.temporary(ty, value);
        self.write(Place::from(temporary), ty, value);
        self.replace(place, Operand::Move(Place::from(temporary)), target);
    }

    /// The place the assignment target `target` names, and the type it holds: a variable, a
    /// field or an element. `None` after an error for any other target.
    pub(crate) fn target(&mut self, target: NodeId) -> Option<(Place, Type<'db>)> {
        match self.tree.kind(target) {
            SyntaxKind::Name => {
                let Some(local) = self.target_variable(target) else {
                    self.unsupported(target, "assignments to this name");
                    return None;
                };
                Some((Place::from(local), self.local_type(local)))
            }
            SyntaxKind::Attribute | SyntaxKind::Subscript => {
                Some((self.place(target)?, self.ty(target)))
            }
            _ => {
                self.unsupported(target, "assignments to this target");
                None
            }
        }
    }

    /// Drops the old value at `place`, which owns memory, and moves `value` in. The new value
    /// was computed first, so a value that reads the old one sees it.
    pub(crate) fn replace(&mut self, place: Place, value: Operand<'db>, target: NodeId) {
        self.push(StatementKind::Drop(place.clone()), target);
        self.assign(place, Rvalue::Use(value), target);
    }

    /// `target op= value` on a number: the value converts to the target's type, as number
    /// mixing allows only when the result keeps it (decision 0014). On a `str`, `+=` appends
    /// (the `text` module).
    fn augmented(&mut self, statement: NodeId) {
        let (Some(target), Some(value)) = (
            self.tree.child(statement, Role::Target),
            self.tree.child(statement, Role::Value),
        ) else {
            return;
        };
        if self.is_selection(target) {
            return self.unsupported(statement, "in-place operators on elements an array chooses");
        }
        let ty = self.ty(target);
        let array = self.is_array(ty);
        let float_parameter = self.is_float_parameter(ty);
        let syntax = self.tree.node(statement).operator();
        if matches!(ty.kind(self.db), TypeKind::Str) && syntax == Operator::Add {
            return self.append_text(target, value, statement);
        }
        if !array && !float_parameter && !matches!(ty.kind(self.db), TypeKind::Primitive(_)) {
            return self.unsupported(statement, "in-place operators on values other than numbers");
        }
        if syntax == Operator::Power
            && (self.is_float(ty) || float_parameter || self.is_complex(ty))
        {
            let Some(place) = self.place(target) else {
                return;
            };
            let operands = vec![Operand::Copy(place.clone()), self.operand_as(value, ty)];
            return self.math_call(place, mir::MathFunction::Power, operands, statement);
        }
        if syntax == Operator::Power && self.is_integer(ty) {
            let Some(place) = self.place(target) else {
                return;
            };
            let operands = (Operand::Copy(place.clone()), self.operand_as(value, ty));
            return self.integer_power(place, operands, ty, statement);
        }
        let Some(operator) = arithmetic(syntax) else {
            return self.unsupported(statement, &format!("`{}=`", syntax.symbol()));
        };
        let Some(mut place) = self.place(target) else {
            return;
        };
        if let Some(lane) = self.lane(&place) {
            place = lane;
        }
        if let Some(place) = self.accumulated_place(&place) {
            return self.accumulate(place, operator, ty, value, statement);
        }
        if place.is_view() {
            return self.augmented_view(place, ty, operator, value, statement);
        }
        if array {
            return self.augmented_array(place, ty, operator, value, statement);
        }
        let right = self.operand_as(value, ty);
        let left = Operand::Copy(place.clone());
        self.arithmetic(place, operator, (left, right), ty, statement);
    }

    /// `raise error`: drops what the function owns and fails with the error (decision 0005).
    fn raise(&mut self, statement: NodeId) {
        if self.tree.child(statement, Role::Cause).is_some() {
            return self.unsupported(statement, "`raise ... from`");
        }
        let Some(value) = self.tree.child(statement, Role::Value) else {
            return self.raise_again(statement);
        };
        let error = self.operand(value);
        self.fail(error, self.ty(value), statement);
    }

    /// The `bool` value of a condition. Owning temporaries it made are dropped before control
    /// branches on it, so a loop condition frees them on every pass.
    pub(crate) fn condition(&mut self, condition: NodeId) -> Operand<'db> {
        self.enter();
        let operand = self.operand(condition);
        self.leave(condition);
        operand
    }

    /// `if`, its `elif` clauses and its `else`: each condition is tested in order, and the first
    /// that holds runs its body.
    fn if_statement(&mut self, statement: NodeId) {
        let mut clauses = vec![(self.tree.child(statement, Role::Condition), statement)];
        clauses.extend(
            self.tree
                .children_with(statement, Role::Else)
                .map(|clause| (self.tree.child(clause, Role::Condition), clause)),
        );
        let has_else = clauses
            .last()
            .is_some_and(|(condition, _)| condition.is_none());
        let count = clauses.len();
        let mut join = None;
        for (index, (condition, clause)) in clauses.into_iter().enumerate() {
            let Some(condition) = condition else {
                // `else`: the last clause.
                self.block(clause, Role::Body);
                self.jump(&mut join, clause);
                break;
            };
            let condition = self.condition(condition);
            let then = self.new_block();
            // The last test of an `if` without `else` goes straight on when it fails.
            let otherwise = if index + 1 == count && !has_else {
                *join.get_or_insert_with(|| self.new_block())
            } else {
                self.new_block()
            };
            self.terminate(
                TerminatorKind::Branch {
                    condition,
                    then,
                    otherwise,
                },
                clause,
            );
            self.start(then);
            self.block(clause, Role::Body);
            self.jump(&mut join, clause);
            self.start(otherwise);
        }
        self.join(join);
    }

    /// The value of an integer literal, possibly negative, or of a `Final` integer constant.
    pub(crate) fn constant_integer(&self, node: NodeId) -> Option<i128> {
        match self.tree.kind(node) {
            SyntaxKind::Integer => {
                integer(&self.text_of(node).replace('_', "").to_ascii_lowercase())
            }
            SyntaxKind::UnaryOperation if self.tree.node(node).operator() == Operator::Negate => {
                self.constant_integer(self.tree.child(node, Role::Value)?)
                    .map(|value| -value)
            }
            SyntaxKind::Name => {
                let &Definition::Item(file, name) = self.definition(node)? else {
                    return None;
                };
                let item = types::Item::new(self.db, self.package, file, name);
                match &self.db.declaration(item).declaration {
                    types::Declaration::Constant {
                        value: Some(types::Value::Integer(value)),
                        ..
                    } => Some(i128::from(*value)),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
