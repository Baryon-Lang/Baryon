//! The walk over one file's tree that resolves every name in it.

use crate::imports::Importer;
use crate::items::{Binding, ModuleItems, imports};
use crate::scopes::{Scope, ScopeKind, Scopes};
use database::{BUILTINS, Definition, File};
use diagnostics::Diagnostic;
use std::collections::BTreeMap;
use syntax::{NodeId, Role, SyntaxKind, SyntaxTree};

/// Resolves the names of one file, collecting each name's definition and the errors found.
pub(crate) struct Walker<'a> {
    /// The file's tree.
    tree: &'a SyntaxTree,
    /// The file's text, for reading names.
    text: &'a str,
    /// Resolves imports against the package.
    importer: Importer<'a>,
    /// The scopes in effect at the current node.
    scopes: Scopes,
    /// Each resolved `Name` node and its definition.
    references: BTreeMap<NodeId, Definition>,
    /// The errors found so far.
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Walker<'a> {
    /// A walker over `tree`, the tree of `file`, whose module-level names are `items`.
    pub(crate) fn new(
        tree: &'a SyntaxTree,
        text: &'a str,
        file: File,
        items: &ModuleItems,
        importer: Importer<'a>,
    ) -> Self {
        let mut walker = Self {
            tree,
            text,
            importer,
            scopes: Scopes::default(),
            references: BTreeMap::new(),
            diagnostics: items.diagnostics().to_vec(),
        };
        let mut module = Scope::new(ScopeKind::Module);
        for (name, binding) in items.iter() {
            let node = match binding {
                Binding::Defined(node) => *node,
                Binding::Imported(import) => import.node,
            };
            let result = walker.importer.top_level(tree, file, binding);
            let definition = walker.settle(result);
            if let Some(definition) = &definition {
                walker.references.insert(node, definition.clone());
            }
            module.bind(name, definition);
        }
        for import in items.repeated() {
            let result = walker.importer.resolve(tree, import);
            if let Some(definition) = walker.settle(result) {
                walker.references.insert(import.node, definition);
            }
        }
        walker.scopes.push(module);
        walker
    }

    /// Walks the whole file and returns each resolved name with its definition, and the errors
    /// in source order.
    pub(crate) fn run(mut self) -> (Vec<(NodeId, Definition)>, Vec<Diagnostic>) {
        let root = self.tree.root();
        self.block(root);
        self.diagnostics
            .sort_by_key(|diagnostic| diagnostic.primary().map(|label| label.span().start()));
        (self.references.into_iter().collect(), self.diagnostics)
    }

    /// Keeps a resolved definition, or records the error and keeps nothing.
    fn settle(&mut self, result: Result<Definition, Diagnostic>) -> Option<Definition> {
        result
            .map_err(|diagnostic| self.diagnostics.push(diagnostic))
            .ok()
    }

    /// The text of a `Name` node.
    fn name(&self, node: NodeId) -> &'a str {
        self.tree.text(node, self.text)
    }

    /// Walks the statements that are children of `node` with role `Body`.
    fn block(&mut self, node: NodeId) {
        for statement in self.tree.children(node).collect::<Vec<_>>() {
            if self.tree.node(statement).role() == Role::Body {
                self.statement(statement);
            }
        }
    }

    /// Resolves one statement, or a clause of one such as `elif` or `except`.
    fn statement(&mut self, node: NodeId) {
        match self.tree.kind(node) {
            SyntaxKind::FunctionDefinition => self.function(node),
            SyntaxKind::ClassDefinition => self.class(node),
            SyntaxKind::Import | SyntaxKind::ImportFrom => {
                for import in imports(self.tree, self.text, node) {
                    self.bind(import.node, &import.name);
                }
            }
            SyntaxKind::Global | SyntaxKind::Nonlocal => {}
            SyntaxKind::TypeAlias => {
                self.optional_target(node, Role::Name);
                let parameters = self.type_parameters(node);
                self.children(node, Role::Value);
                if parameters {
                    self.scopes.pop();
                }
            }
            _ => self.parts(node),
        }
    }

    /// Resolves each child of a statement or clause by the part it plays.
    fn parts(&mut self, node: NodeId) {
        let handler = self.tree.kind(node) == SyntaxKind::ExceptHandler;
        for child in self.tree.children(node).collect::<Vec<_>>() {
            let role = self.tree.node(child).role();
            let kind = self.tree.kind(child);
            if role == Role::Target || (handler && role == Role::Name) {
                self.target(child);
            } else if role == Role::Pattern {
                self.pattern(child);
            } else if kind.is_expression() {
                self.expression(child);
            } else {
                self.statement(child);
            }
        }
    }

    /// Resolves a function definition: its decorators and signature in the enclosing scopes,
    /// then its body in a scope of its own.
    fn function(&mut self, node: NodeId) {
        self.children(node, Role::Decorator);
        self.optional_target(node, Role::Name);
        let type_parameters = self.type_parameters(node);
        let parameters = self.tree.child(node, Role::Parameters);
        if let Some(parameters) = parameters {
            self.parameter_defaults(parameters);
        }
        self.children(node, Role::Returns);

        let mut scope = Scope::new(ScopeKind::Function);
        if let Some(parameters) = parameters {
            for name in self.parameter_names(parameters) {
                scope.bind(self.name(name), Some(Definition::Local(name)));
            }
        }
        self.collect_block(node, &mut scope);
        self.scopes.push(scope);
        if let Some(parameters) = parameters {
            for name in self.parameter_names(parameters) {
                self.bind(name, self.name(name));
            }
        }
        self.block(node);
        self.scopes.pop();
        if type_parameters {
            self.scopes.pop();
        }
    }

    /// Resolves a class definition: decorators and bases outside, the body in a class scope.
    fn class(&mut self, node: NodeId) {
        self.children(node, Role::Decorator);
        self.optional_target(node, Role::Name);
        let type_parameters = self.type_parameters(node);
        self.children(node, Role::Arguments);
        let mut scope = Scope::new(ScopeKind::Class);
        self.collect_block(node, &mut scope);
        self.scopes.push(scope);
        self.block(node);
        self.scopes.pop();
        if type_parameters {
            self.scopes.pop();
        }
    }

    /// Enters a scope for the type parameters of a definition, if it has any, resolving their
    /// bounds and defaults. Returns whether a scope was entered, for the caller to leave.
    fn type_parameters(&mut self, node: NodeId) -> bool {
        let Some(parameters) = self.tree.child(node, Role::TypeParameters) else {
            return false;
        };
        let mut scope = Scope::new(ScopeKind::TypeParameters);
        let variables: Vec<NodeId> = self.tree.children(parameters).collect();
        for &variable in &variables {
            if let Some(name) = self.tree.child(variable, Role::Name) {
                scope.bind(self.name(name), Some(Definition::Local(name)));
            }
        }
        self.scopes.push(scope);
        for variable in variables {
            self.optional_target(variable, Role::Name);
            self.children(variable, Role::Bound);
            self.children(variable, Role::Default);
        }
        true
    }

    /// Resolves the annotations and defaults of a parameter list, in the enclosing scopes.
    fn parameter_defaults(&mut self, parameters: NodeId) {
        for parameter in self.tree.children(parameters).collect::<Vec<_>>() {
            self.children(parameter, Role::Annotation);
            self.children(parameter, Role::Default);
        }
    }

    /// The `Name` node of each parameter in a parameter list.
    fn parameter_names(&self, parameters: NodeId) -> Vec<NodeId> {
        self.tree
            .children(parameters)
            .filter_map(|parameter| self.tree.child(parameter, Role::Name))
            .collect()
    }

    /// Resolves the children of `node` that have the given role, as expressions.
    fn children(&mut self, node: NodeId, role: Role) {
        for child in self.tree.children_with(node, role).collect::<Vec<_>>() {
            if self.tree.kind(child) == SyntaxKind::Decorator {
                self.children(child, Role::Value);
            } else {
                self.expression(child);
            }
        }
    }

    /// Records the binding of the child of `node` with the given role, if there is one.
    fn optional_target(&mut self, node: NodeId, role: Role) {
        if let Some(name) = self.tree.child(node, role) {
            self.target(name);
        }
    }

    /// Resolves an assignment target: names in it are bindings; anything else, such as the
    /// array in `grid[i] = 0.0`, is read.
    fn target(&mut self, node: NodeId) {
        match self.tree.kind(node) {
            SyntaxKind::Name => self.bind(node, self.name(node)),
            SyntaxKind::Tuple | SyntaxKind::List => {
                for item in self.tree.children(node).collect::<Vec<_>>() {
                    self.target(item);
                }
            }
            SyntaxKind::Starred => self.optional_target(node, Role::Value),
            _ => {
                self.expression(node);
            }
        }
    }

    /// Records the binding `name` at `node`: the definition the innermost scope gives the name,
    /// or a new local when the scope does not bind it yet.
    fn bind(&mut self, node: NodeId, name: &str) {
        let Some(scope) = self.scopes.innermost() else {
            return;
        };
        if scope.get(name).is_none() {
            scope.bind(name, Some(Definition::Local(node)));
        }
        if let Some(Some(definition)) = scope.get(name) {
            self.references.insert(node, definition.clone());
        }
    }

    /// Resolves a `match` pattern: captures bind names; values and class names are read.
    fn pattern(&mut self, node: NodeId) {
        let kind = self.tree.kind(node);
        for child in self.tree.children(node).collect::<Vec<_>>() {
            match self.tree.node(child).role() {
                Role::Name if kind == SyntaxKind::PatternKeyword => {}
                Role::Name | Role::Rest => self.bind(child, self.name(child)),
                Role::Class | Role::Key | Role::Value => {
                    self.expression(child);
                }
                _ => self.pattern(child),
            }
        }
    }

    /// Resolves an expression and returns what it refers to, when it is a name or a module
    /// attribute; `None` for any other expression.
    fn expression(&mut self, node: NodeId) -> Option<Definition> {
        match self.tree.kind(node) {
            SyntaxKind::Name => self.read(node),
            SyntaxKind::Attribute => self.attribute(node),
            SyntaxKind::Keyword => {
                self.children(node, Role::Value);
                None
            }
            SyntaxKind::ListComprehension
            | SyntaxKind::SetComprehension
            | SyntaxKind::DictionaryComprehension
            | SyntaxKind::Generator => {
                self.comprehension(node);
                None
            }
            SyntaxKind::Lambda => {
                self.lambda(node);
                None
            }
            SyntaxKind::NamedExpression => {
                self.optional_target(node, Role::Target);
                self.children(node, Role::Value);
                None
            }
            _ => {
                for child in self.tree.children(node).collect::<Vec<_>>() {
                    self.expression(child);
                }
                None
            }
        }
    }

    /// Resolves a name that is read, reporting it if it refers to nothing.
    fn read(&mut self, node: NodeId) -> Option<Definition> {
        let name = self.name(node);
        let definition = if let Some(found) = self.scopes.lookup(name) {
            found.clone()?
        } else if let Some(library) = self.importer.library("builtins", name) {
            library
        } else if let Some(builtin) = BUILTINS.into_iter().find(|&builtin| builtin == name) {
            Definition::Builtin(builtin)
        } else {
            self.diagnostics.push(
                Diagnostic::error(format!("undefined name `{name}`"))
                    .with_primary(self.tree.span(node), "not defined in this scope"),
            );
            return None;
        };
        self.references.insert(node, definition.clone());
        Some(definition)
    }

    /// Resolves `value.name`. The name is resolved here only when `value` is a module; any
    /// other attribute needs types, which the type checker resolves.
    fn attribute(&mut self, node: NodeId) -> Option<Definition> {
        let base = self
            .tree
            .child(node, Role::Value)
            .and_then(|value| self.expression(value));
        let attribute = self.tree.child(node, Role::Attribute)?;
        let member = self.name(attribute);
        let definition = match base? {
            Definition::Module(module) => {
                let result = self
                    .importer
                    .attribute(self.tree, &module, member, attribute);
                self.settle(result)?
            }
            Definition::Standard {
                module,
                member: None,
            } => self.importer.standard_member(module, member),
            _ => return None,
        };
        self.references.insert(attribute, definition.clone());
        Some(definition)
    }

    /// Resolves a comprehension. Its first iterable is evaluated in the enclosing scope; its
    /// targets, conditions, later iterables and value in a scope of its own.
    fn comprehension(&mut self, node: NodeId) {
        let clauses: Vec<NodeId> = self.tree.children_with(node, Role::Item).collect();
        if let Some(&first) = clauses.first() {
            self.children(first, Role::Iterator);
        }
        self.scopes.push(Scope::new(ScopeKind::Comprehension));
        for (index, clause) in clauses.into_iter().enumerate() {
            if index > 0 {
                self.children(clause, Role::Iterator);
            }
            self.optional_target(clause, Role::Target);
            self.children(clause, Role::Condition);
        }
        self.children(node, Role::Key);
        self.children(node, Role::Value);
        self.scopes.pop();
    }

    /// Resolves a lambda: its parameters form a scope around its body.
    fn lambda(&mut self, node: NodeId) {
        let parameters = self.tree.child(node, Role::Parameters);
        if let Some(parameters) = parameters {
            self.parameter_defaults(parameters);
        }
        self.scopes.push(Scope::new(ScopeKind::Function));
        if let Some(parameters) = parameters {
            for name in self.parameter_names(parameters) {
                self.bind(name, self.name(name));
            }
        }
        self.children(node, Role::Body);
        self.scopes.pop();
    }

    /// Adds to `scope` every name that the statements in the body of `node` bind, in source
    /// order, without entering nested functions or classes.
    fn collect_block(&mut self, node: NodeId, scope: &mut Scope) {
        for statement in self
            .tree
            .children_with(node, Role::Body)
            .collect::<Vec<_>>()
        {
            self.collect(statement, scope);
        }
    }

    /// Adds to `scope` every name that `node`, a statement or clause, binds.
    fn collect(&mut self, node: NodeId, scope: &mut Scope) {
        let kind = self.tree.kind(node);
        match kind {
            SyntaxKind::FunctionDefinition | SyntaxKind::ClassDefinition => {
                if let Some(name) = self.tree.child(node, Role::Name) {
                    scope.bind(self.name(name), Some(Definition::Local(name)));
                }
                return;
            }
            SyntaxKind::Import | SyntaxKind::ImportFrom => {
                for import in imports(self.tree, self.text, node) {
                    let result = self.importer.resolve(self.tree, &import);
                    let definition = self.settle(result);
                    scope.bind(&import.name, definition);
                }
                return;
            }
            _ => {}
        }
        for child in self.tree.children(node).collect::<Vec<_>>() {
            let role = self.tree.node(child).role();
            let child_kind = self.tree.kind(child);
            if role == Role::Target
                || role == Role::Name
                    && matches!(kind, SyntaxKind::ExceptHandler | SyntaxKind::TypeAlias)
            {
                self.collect_target(child, scope);
            } else if role == Role::Pattern {
                self.collect_pattern(child, scope);
            } else if !child_kind.is_expression() {
                self.collect(child, scope);
            }
        }
    }

    /// Adds the names a target binds to `scope`.
    fn collect_target(&self, node: NodeId, scope: &mut Scope) {
        match self.tree.kind(node) {
            SyntaxKind::Name => scope.bind(self.name(node), Some(Definition::Local(node))),
            SyntaxKind::Tuple | SyntaxKind::List | SyntaxKind::Starred => {
                for item in self.tree.children(node) {
                    self.collect_target(item, scope);
                }
            }
            _ => {}
        }
    }

    /// Adds the names a pattern captures to `scope`.
    fn collect_pattern(&self, node: NodeId, scope: &mut Scope) {
        let kind = self.tree.kind(node);
        for child in self.tree.children(node) {
            match self.tree.node(child).role() {
                Role::Name if kind == SyntaxKind::PatternKeyword => {}
                Role::Name | Role::Rest => {
                    scope.bind(self.name(child), Some(Definition::Local(child)));
                }
                Role::Class | Role::Key | Role::Value => {}
                _ => self.collect_pattern(child, scope),
            }
        }
    }
}
