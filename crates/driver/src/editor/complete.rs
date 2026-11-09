//! Completion: the names that could be typed at the cursor.

use super::hover::{member_signature, signature};
use super::{Cursor, is_public, outline};
use crate::Session;
use database::{BUILTINS, Definition, File, ModuleName};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Declaration, Item, TypeDb, TypeKind};

/// One name [`Session::complete`] offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    /// The name to insert.
    pub label: String,
    /// What the name is, which editors show as an icon.
    pub kind: CompletionKind,
    /// What the name is, as one line of code, as [`Hover::signature`](super::Hover::signature)
    /// writes it; `None` for a name the language provides, or a variable whose type is not
    /// known yet.
    pub detail: Option<String>,
}

/// What kind of name a [`Completion`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    /// A local variable or parameter.
    Variable,
    /// A top-level function.
    Function,
    /// A struct's method.
    Method,
    /// A struct's field.
    Field,
    /// A class or a type alias.
    Class,
    /// A member of an enum.
    EnumMember,
    /// A `Final` constant.
    Constant,
    /// A module or package of the program.
    Module,
    /// A name the language provides, such as `print`, `prange` or `f64`.
    Builtin,
}

impl Session {
    /// The names that could be typed at byte `offset` of `file`. After a dot they are the
    /// members of what is before it: a module's public definitions and submodules after `m.`,
    /// a struct value's fields and methods after `x.`, and an enum's members after `Phase.`.
    /// Anywhere else they are the names in scope: the local variables and parameters bound
    /// before the cursor in its function, the module's definitions and imports, and the names
    /// the language provides. A name that starts with `_` is offered only in its own module.
    ///
    /// Every name in scope is offered, whatever has been typed of it; the editor filters them
    /// as the user types. A dot after something whose type is not known, or that has no
    /// members, offers nothing.
    ///
    /// ```
    /// use driver::{CompletionKind, Session};
    ///
    /// let mut session = Session::new();
    /// let text = "class Body(Struct):\n    mass: f64\n\n    def weight(self) -> f64:\n        \
    ///             return 9.81 * self.mass\n\n\ndef total(b: Body) -> f64:\n    return b.\n";
    /// let file = session.add_file("main.byn", text)?;
    /// let after_dot = u32::try_from(text.rfind("b.").unwrap() + 2).unwrap();
    ///
    /// let offered = session.complete(file, after_dot);
    /// let labels: Vec<&str> = offered.iter().map(|item| item.label.as_str()).collect();
    /// assert_eq!(labels, ["mass", "weight"]);
    /// assert_eq!(offered[1].kind, CompletionKind::Method);
    /// assert_eq!(offered[1].detail.as_deref(), Some("def weight(self) -> f64"));
    /// # Ok::<(), source::SourceError>(())
    /// ```
    #[must_use]
    pub fn complete(&self, file: File, offset: u32) -> Vec<Completion> {
        let cursor = self.cursor(file, offset);
        let base = cursor
            .name()
            .and_then(|name| cursor.attribute_of(name))
            .and_then(|attribute| cursor.tree.child(attribute, Role::Value));
        let mut offered = match base {
            Some(base) => members(&cursor, base),
            None => in_scope(&cursor),
        };
        let mut seen = Vec::new();
        offered.retain(|item| {
            let new = !seen.contains(&item.label);
            if new {
                seen.push(item.label.clone());
            }
            new
        });
        offered
    }
}

/// The members of what `base` is: a module, an enum class, or a struct value.
fn members(cursor: &Cursor<'_>, base: NodeId) -> Vec<Completion> {
    let named = match cursor.tree.kind(base) {
        SyntaxKind::Name => Some(base),
        SyntaxKind::Attribute => cursor.tree.child(base, Role::Attribute),
        _ => None,
    };
    if let Some(Definition::Module(module)) =
        named.and_then(|name| cursor.resolution.definition(name))
    {
        return module_members(cursor, module);
    }
    let Some(class) = cursor.class_of(base) else {
        return Vec::new();
    };
    let db = cursor.db;
    let own = class.file(db) == cursor.file;
    let value = cursor.expression(base);
    match &db.declaration(class).declaration {
        Declaration::Enum { variants } if value.is_none() => variants
            .iter()
            .filter(|variant| own || is_public(variant))
            .map(|variant| Completion {
                label: variant.clone(),
                kind: CompletionKind::EnumMember,
                detail: member_signature(db, class, variant),
            })
            .collect(),
        Declaration::Struct {
            fields, methods, ..
        } if value.is_some_and(|ty| matches!(ty.kind(db), TypeKind::Struct(_))) => {
            let tree = db.parse(class.file(db)).tree();
            let text = class.file(db).source(db).text();
            let fields = fields.iter().map(|field| Completion {
                label: field.name.clone(),
                kind: CompletionKind::Field,
                detail: Some(format!("{}: {}", field.name, field.ty.display(db))),
            });
            let methods = methods.iter().map(|&method| Completion {
                label: tree.text(method.name(db), text).to_owned(),
                kind: CompletionKind::Method,
                detail: Some(signature(db, method)),
            });
            fields
                .chain(methods)
                .filter(|item| own || is_public(&item.label))
                .collect()
        }
        _ => Vec::new(),
    }
}

/// A module's public definitions, then the modules directly inside it when it is a package.
fn module_members(cursor: &Cursor<'_>, module: &ModuleName) -> Vec<Completion> {
    let db = cursor.db;
    let modules = database::modules(db, cursor.package);
    let mut offered = Vec::new();
    if let Some(file) = modules.get(module) {
        let tree = db.parse(file).tree();
        let text = file.source(db).text();
        offered.extend(
            outline::definitions(tree, text, tree.root())
                .into_iter()
                .filter(|defined| is_public(&defined.text))
                .map(|defined| {
                    let item = Item::new(db, cursor.package, file, defined.name);
                    completion_of(db, item, defined.text)
                }),
        );
    }
    let depth = module.components().len();
    offered.extend(modules.iter().filter_map(|(name, _)| {
        let components = name.components();
        (components.len() > depth && components[..depth] == *module.components()).then(|| {
            Completion {
                label: components[depth].clone(),
                kind: CompletionKind::Module,
                detail: Some(format!("module {}", components[..=depth].join("."))),
            }
        })
    }));
    offered
}

/// The names in scope at the cursor: its function's variables bound so far, the module's
/// definitions and imports, and the language's names.
fn in_scope(cursor: &Cursor<'_>) -> Vec<Completion> {
    let db = cursor.db;
    let tree = cursor.tree;
    let mut offered = Vec::new();
    if let Some((function, _)) = cursor.function {
        let span = tree.node(function);
        // A variable is offered from the line after its first binding, so `x = ` does not
        // offer `x`.
        let line_start = cursor.text[..cursor.offset as usize]
            .rfind('\n')
            .map_or(0, |newline| newline + 1);
        let mut bindings: Vec<NodeId> = cursor
            .resolution
            .references()
            .iter()
            .filter_map(|(_, definition)| match definition {
                Definition::Local(binding) => Some(*binding),
                _ => None,
            })
            .filter(|&binding| {
                let start = tree.node(binding).start();
                span.start() <= start && (start as usize) < line_start && start < span.end()
            })
            .collect();
        bindings.sort();
        bindings.dedup();
        offered.extend(bindings.into_iter().map(|binding| {
            let label = tree.text(binding, cursor.text).to_owned();
            Completion {
                detail: cursor
                    .local(binding)
                    .map(|ty| format!("{label}: {}", ty.display(db))),
                label,
                kind: CompletionKind::Variable,
            }
        }));
    }
    offered.extend(
        outline::definitions(tree, cursor.text, tree.root())
            .into_iter()
            .map(|defined| {
                let item = Item::new(db, cursor.package, cursor.file, defined.name);
                completion_of(db, item, defined.text)
            }),
    );
    for (name, label) in outline::imports(tree, cursor.text) {
        offered.push(match cursor.resolution.definition(name) {
            Some(Definition::Item(file, definition)) => {
                completion_of(db, Item::new(db, cursor.package, *file, *definition), label)
            }
            _ => Completion {
                label,
                kind: CompletionKind::Module,
                detail: None,
            },
        });
    }
    offered.extend(BUILTINS.iter().map(|&builtin| Completion {
        label: builtin.to_owned(),
        kind: CompletionKind::Builtin,
        detail: None,
    }));
    offered
}

/// The completion for a top-level definition or method, offered under `label`.
fn completion_of(db: &dyn TypeDb, item: Item<'_>, label: String) -> Completion {
    let kind = match &db.declaration(item).declaration {
        Declaration::Function(_) => CompletionKind::Function,
        Declaration::Struct { .. } | Declaration::Enum { .. } | Declaration::Alias(_) => {
            CompletionKind::Class
        }
        Declaration::Constant { .. } => CompletionKind::Constant,
    };
    Completion {
        label,
        kind,
        detail: Some(signature(db, item)),
    }
}
