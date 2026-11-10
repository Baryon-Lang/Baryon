//! Hover: what the name under the cursor is, as one line of code, and its documentation string.

use super::{Target, module_name, outline};
use crate::Session;
use database::{Db, File};
use source::Span;
use syntax::{Role, SyntaxKind};
use types::{Declaration, Item, Mode, Signature, TypeDb, Value};

/// What [`Session::hover`] shows for a name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hover {
    /// The name the cursor is on, which the editor highlights.
    pub span: Span,
    /// What the name is, as one line of code: a function's signature (`def step(dt: f64) ->
    /// None`), a class's header, a variable, field or constant with its type (`energy: f64`), an
    /// enum member (`Phase.GAS`), a module (`module physics.nbody`), or the bare name of
    /// something the language or the standard library provides.
    pub signature: String,
    /// The documentation string of what the name refers to, cleaned of its indentation as
    /// `baryon doc` shows it, or a line saying where a built-in name is described. `None` for
    /// a variable, or a definition without one. Only public definitions, those `baryon doc`
    /// documents, carry theirs.
    pub documentation: Option<String>,
}

impl Session {
    /// What the name under the cursor at byte `offset` of `file` is: its type, or its
    /// signature, and the documentation string of what it refers to. `None` when the cursor is
    /// not on a name, or on one nothing is known about, as in code too broken to check.
    ///
    /// A local variable shows the type the checker gave it, so a hover is the quickest way to
    /// see what an inferred variable holds:
    ///
    /// ```
    /// use driver::Session;
    ///
    /// let mut session = Session::new();
    /// let text = "def kinetic(m: f64, v: f64) -> f64:\n    energy = 0.5 * m * v * v\n    \
    ///             return energy\n";
    /// let file = session.add_file("main.byn", text)?;
    /// let energy = u32::try_from(text.rfind("energy").unwrap()).unwrap();
    /// assert_eq!(session.hover(file, energy).unwrap().signature, "energy: f64");
    /// # Ok::<(), source::SourceError>(())
    /// ```
    #[must_use]
    pub fn hover(&self, file: File, offset: u32) -> Option<Hover> {
        let cursor = self.cursor(file, offset);
        let (span, target) = cursor.target()?;
        let db = cursor.db;
        let (signature, documentation) = match &target {
            Target::Local(binding) => {
                let name = cursor.tree.text(*binding, cursor.text);
                let signature = match cursor.local(*binding) {
                    Some(ty) => format!("{name}: {}", ty.display(db)),
                    None => name.to_owned(),
                };
                (signature, None)
            }
            Target::Item(item) => (signature(db, *item), item_documentation(db, *item)),
            Target::Member { class, name } => (
                member_signature(db, *class, name)?,
                member_documentation(db, *class, name),
            ),
            Target::Attribute(name, ty) => (format!("{name}: {}", ty.display(db)), None),
            Target::Module(module) => {
                let documentation =
                    database::modules(db, cursor.package)
                        .get(module)
                        .and_then(|file| {
                            documentation::document(
                                db.parse(file).tree(),
                                file.source(db).text(),
                                "",
                            )
                            .text()
                            .map(str::to_owned)
                        });
                (format!("module {module}"), documentation)
            }
            Target::Standard(name) => (
                name.clone(),
                Some("Part of the standard library.".to_owned()),
            ),
            Target::Builtin(name) => (
                (*name).to_owned(),
                Some("Built into the language; the language reference describes it.".to_owned()),
            ),
        };
        Some(Hover {
            span,
            signature,
            documentation,
        })
    }
}

/// What a top-level definition or method declares, as one line of code.
pub(super) fn signature(db: &dyn TypeDb, item: Item<'_>) -> String {
    let file = item.file(db);
    let tree = db.parse(file).tree();
    let text = file.source(db).text();
    let name = tree.text(item.name(db), text);
    match &db.declaration(item).declaration {
        Declaration::Function(signature) => function(db, name, signature),
        Declaration::Struct { .. } | Declaration::Enum { .. } | Declaration::Trait { .. } => {
            outline::statement_of(tree, item.name(db)).map_or_else(
                || format!("class {name}"),
                |class| outline::class_header(tree, text, class),
            )
        }
        Declaration::Alias(ty) => format!("{name} = {}", ty.display(db)),
        Declaration::Constant { ty, value } => {
            let value = match value {
                Some(Value::Integer(integer)) => format!(" = {integer}"),
                Some(Value::Float(bits)) => format!(" = {:?}", f64::from_bits(*bits)),
                None => String::new(),
            };
            format!("{name}: Final[{}]{value}", ty.display(db))
        }
    }
}

/// A function's signature as a program writes it: `def scale[T](x: T, factor: f64 = ...) -> T`.
/// A parameter's mode wraps its type, and `self` is written bare.
fn function(db: &dyn TypeDb, name: &str, signature: &Signature<'_>) -> String {
    let mut type_parameters = String::new();
    if !signature.type_parameters.is_empty() {
        let parameters: Vec<String> = signature
            .type_parameters
            .iter()
            .map(|ty| ty.display(db).to_string())
            .collect();
        type_parameters = format!("[{}]", parameters.join(", "));
    }
    let parameters: Vec<String> = signature
        .parameters
        .iter()
        .enumerate()
        .map(|(index, parameter)| {
            if index == 0 && parameter.name == "self" {
                return parameter.name.clone();
            }
            let ty = parameter.ty.display(db);
            let ty = match parameter.mode {
                Mode::Borrow => ty.to_string(),
                Mode::InOut => format!("InOut[{ty}]"),
                Mode::Owned => format!("Owned[{ty}]"),
            };
            let default = if parameter.default { " = ..." } else { "" };
            format!("{}: {ty}{default}", parameter.name)
        })
        .collect();
    format!(
        "def {name}{type_parameters}({}) -> {}",
        parameters.join(", "),
        signature.returns.display(db)
    )
}

/// What the member `name` of `class` is, as one line of code: a field and its type, a method's
/// signature, or an enum member. `None` when the class has no such member.
pub(super) fn member_signature(db: &dyn TypeDb, class: Item<'_>, name: &str) -> Option<String> {
    let file = class.file(db);
    let tree = db.parse(file).tree();
    let text = file.source(db).text();
    match &db.declaration(class).declaration {
        Declaration::Struct {
            fields, methods, ..
        } => {
            if let Some(field) = fields.iter().find(|field| field.name == name) {
                return Some(format!("{name}: {}", field.ty.display(db)));
            }
            methods
                .iter()
                .find(|method| tree.text(method.name(db), text) == name)
                .map(|&method| signature(db, method))
        }
        Declaration::Trait { methods, .. } => methods
            .iter()
            .find(|method| tree.text(method.name(db), text) == name)
            .map(|&method| signature(db, method)),
        Declaration::Enum { variants } => variants
            .iter()
            .any(|variant| variant == name)
            .then(|| format!("{}.{name}", tree.text(class.name(db), text))),
        _ => None,
    }
}

/// The documentation string of a top-level definition or a method.
fn item_documentation(db: &dyn Db, item: Item<'_>) -> Option<String> {
    let file = item.file(db);
    let tree = db.parse(file).tree();
    let text = file.source(db).text();
    let name = item.name(db);
    let statement = tree
        .children_with(tree.root(), Role::Body)
        .take_while(|statement| statement.index() <= name.index())
        .last()?;
    if tree.kind(statement) == SyntaxKind::ClassDefinition
        && tree.child(statement, Role::Name) != Some(name)
    {
        let class = tree.child(statement, Role::Name)?;
        return documented(db, file, &[tree.text(class, text), tree.text(name, text)]);
    }
    documented(db, file, &[tree.text(name, text)])
}

/// The documentation string of the member `name` of `class`.
fn member_documentation(db: &dyn Db, class: Item<'_>, name: &str) -> Option<String> {
    let file = class.file(db);
    let class_name = db
        .parse(file)
        .tree()
        .text(class.name(db), file.source(db).text());
    documented(db, file, &[class_name, name])
}

/// The documentation string `baryon doc` gives the item at `path` in `file`: a top-level name,
/// or a class's name and one of its members.
fn documented(db: &dyn Db, file: File, path: &[&str]) -> Option<String> {
    let module = documentation::document(
        db.parse(file).tree(),
        file.source(db).text(),
        &module_name(db, file),
    );
    let (first, rest) = path.split_first()?;
    let mut item = module.items().iter().find(|item| item.name() == *first)?;
    for name in rest {
        item = item
            .members()
            .iter()
            .find(|member| member.name() == *name)?;
    }
    item.text().map(str::to_owned)
}
