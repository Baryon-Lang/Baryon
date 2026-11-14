//! Each function's body, made ready for a backend, and its LLVM text, as queries the database
//! keeps (roadmap 2.2.2, decision 0050). `baryon watch` keeps one database between rebuilds,
//! and `salsa` (decision 0016) runs a query again only when something it read changed, so an
//! edit rewrites only the functions it reaches; `baryon build` starts from an empty database
//! and runs each query once. The queries of a program's functions run on every core at once,
//! each thread with its own handle to the database, which shares their results.
//!
//! Everything a query uses is read through the database, which is what makes keeping its
//! result safe: the declarations of the types a function uses, the symbols of the functions it
//! calls, and the source lines its instructions point back to. The lines come from
//! [`source_before`], the text of the function's file up to the end of its last line, so a
//! function whose own text, and the text above it, did not change keeps its text, whatever
//! changed below it.

use super::functions::{Function, file_functions};
use crate::compile::instances;
use database::{Db, File, Package};
use diagnostics::Diagnostic;
use mir::Body;
use source::{FileId, SourceFile, Sources};
use types::{Declaration, Item, Nominal, TypeDb, TypeKind};

/// A function and the type arguments it is compiled at, interned so a query can take it.
#[salsa::interned(debug)]
pub(super) struct Wanted<'db> {
    /// The function or method.
    pub(super) item: Item<'db>,
    /// One concrete type per type parameter; none for a function that is not generic.
    #[returns(ref)]
    pub(super) arguments: Vec<types::Type<'db>>,
}

/// What a build is for, which every function's text depends on.
#[salsa::interned(debug)]
pub(super) struct Settings<'db> {
    /// The package being built.
    pub(super) package: Package,
    /// The package's absolute directory when the text carries line tables, `None` without.
    #[returns(ref)]
    pub(super) root: Option<String>,
    /// Whether integer overflow wraps, as in a release build.
    pub(super) wraps: bool,
}

/// A function's body ready for a backend, or the errors of what lowering cannot handle yet.
#[derive(Clone, Debug, PartialEq, Eq, salsa::SalsaValue)]
pub(super) enum Ready<'db> {
    /// The body.
    Body(Body<'db>),
    /// The errors.
    Errors(Vec<Diagnostic>),
}

/// The body of `wanted` ready for a backend ([`instances::ready`]), or the errors of what
/// lowering cannot handle yet.
#[salsa::tracked(returns(ref))]
pub(super) fn ready_body<'db>(
    db: &'db dyn TypeDb,
    settings: Settings<'db>,
    wanted: Wanted<'db>,
) -> Ready<'db> {
    let package = *settings.package(db);
    let declared = declared(db, package);
    let implementation = move |method: Item<'db>, implementor: types::Type<'db>| {
        checker::implementation(db, package, method, implementor)
    };
    let instance = instances::Instance {
        item: *wanted.item(db),
        arguments: wanted.arguments(db).clone(),
    };
    let destructor = |nominal: &Nominal<'db>| checker::destructor(db, package, nominal).is_some();
    match instances::ready(db, &instance, &declared, &destructor, &implementation) {
        Ok(body) => Ready::Body(body),
        Err(errors) => Ready::Errors(errors),
    }
}

/// The LLVM text of `wanted`, written by a module of its own and taken out of it, or the
/// error the backend gives for what it cannot write yet, at the function's name. `None` when
/// the body itself has errors, which [`ready_body`] reports.
#[salsa::tracked(returns(ref))]
pub(super) fn written<'db>(
    db: &'db dyn TypeDb,
    settings: Settings<'db>,
    wanted: Wanted<'db>,
) -> Option<Result<llvm_backend::Written, Diagnostic>> {
    let Ready::Body(body) = ready_body(db, settings, wanted) else {
        return None;
    };
    let package = *settings.package(db);
    let item = *wanted.item(db);
    // A default's function raises what the function it belongs to declares.
    let owner = owner(db, package, item);
    let raises = match &db.declaration(owner).declaration {
        Declaration::Function(signature) => match signature.returns.kind(db) {
            TypeKind::Fallible { errors, .. } => errors.as_slice(),
            _ => &[],
        },
        _ => &[],
    };
    let declared = declared(db, package);
    let names = names(db, package);
    let symbol =
        |item: &Item<'db>, arguments: &[types::Type<'db>]| symbol(db, package, *item, arguments);
    let file = item.file(db);
    let sources = Before {
        db,
        package,
        own: file,
        prefix: source_before(db, file, body.span.end()).0.as_ref(),
    };
    let destructor = destructors(db, package);
    let mut module = llvm_backend::Module::new(db, &sources, &declared, &names, &symbol);
    module.destructors(&destructor);
    if let Some(root) = settings.root(db) {
        module.debug_info(root);
    }
    if *settings.wraps(db) {
        module.wrap_on_overflow();
    }
    let name = symbol(&item, wanted.arguments(db));
    Some(
        module
            .define(&name, body, raises)
            .map(|()| module.take())
            .map_err(|unsupported| at(db, item, unsupported.to_string())),
    )
}

/// The text of `file` up to the end of the line holding the byte at `end`: the part of the
/// file a function ending there points its line tables into. A query, so it is the same value,
/// and the function's text is kept, until the text above that point changes.
#[salsa::tracked(returns(ref))]
fn source_before(db: &dyn Db, file: File, end: u32) -> Prefix {
    let source = file.source(db);
    let text = source.text();
    let end = (end as usize).min(text.len());
    let line_end = text[end..]
        .find('\n')
        .map_or(text.len(), |offset| end + offset + 1);
    Prefix(SourceFile::new(source.name(), &text[..line_end]).ok())
}

/// The start of a file, as [`source_before`] keeps it: `None` only if it could not be made,
/// which a prefix of a file that already exists never is.
struct Prefix(Option<SourceFile>);

impl PartialEq for Prefix {
    /// Equal when the file's name and the text kept are.
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Some(one), Some(other)) => one.name() == other.name() && one.text() == other.text(),
            (None, None) => true,
            _ => false,
        }
    }
}

impl Eq for Prefix {}

/// The sources a function's text is written from: its own file as far as [`source_before`]
/// keeps it, and every other file whole.
struct Before<'db> {
    /// The database the other files are read from.
    db: &'db dyn TypeDb,
    /// The package whose files they are.
    package: Package,
    /// The function's file.
    own: File,
    /// The function's file up to its last line, or `None` to read it whole.
    prefix: Option<&'db SourceFile>,
}

impl std::fmt::Debug for Before<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Before")
            .field("own", &self.own.id(self.db))
            .field("prefix", &self.prefix.map(SourceFile::name))
            .finish_non_exhaustive()
    }
}

impl Sources for Before<'_> {
    /// The function's file as [`source_before`] keeps it, or another file of the package
    /// whole, and the function's file whole for an id that names no file, which no span the
    /// database made carries.
    fn file(&self, id: FileId) -> &SourceFile {
        let own = self.own.id(self.db) == id;
        if let Some(prefix) = self.prefix.filter(|_| own) {
            return prefix;
        }
        let file = self
            .package
            .files(self.db)
            .iter()
            .find(|file| file.id(self.db) == id)
            .unwrap_or(&self.own);
        file.source(self.db)
    }
}

/// The symbol of `item` at the type `arguments`: its module's name and its own, joined by
/// dots, with a method's class between them, then the arguments in brackets for an instance
/// of a generic function: `main.square[f64]`. A default's function is the function's symbol
/// with the parameter's name after a colon (decision 0027).
pub(super) fn symbol<'db>(
    db: &'db dyn TypeDb,
    package: Package,
    item: Item<'db>,
    arguments: &[types::Type<'db>],
) -> String {
    let base = base_symbol(db, package, item);
    if arguments.is_empty() {
        return base;
    }
    // A function's own type shows as its signature, which two functions can share, so its
    // symbol names the function instead.
    let shown: Vec<String> = arguments
        .iter()
        .map(|argument| match argument.kind(db) {
            TypeKind::Function { item, .. } => base_symbol(db, package, *item),
            _ => argument.display(db).to_string(),
        })
        .collect();
    format!("{base}[{}]", shown.join(", "))
}

/// The symbol of `item` without type arguments, from its file's functions; empty for an item
/// that is no function.
fn base_symbol<'db>(db: &'db dyn TypeDb, package: Package, item: Item<'db>) -> String {
    file_functions(db, package, item.file(db))
        .iter()
        .find_map(|function| {
            if function.item == item {
                return Some(function.symbol.clone());
            }
            function
                .defaults
                .iter()
                .find(|(default, _)| *default == item)
                .map(|(_, symbol)| symbol.clone())
        })
        .unwrap_or_default()
}

/// The function `item` belongs to when it computes a default, otherwise `item` itself.
fn owner<'db>(db: &'db dyn TypeDb, package: Package, item: Item<'db>) -> Item<'db> {
    file_functions(db, package, item.file(db))
        .iter()
        .find(|function: &&Function<'db>| {
            function
                .defaults
                .iter()
                .any(|(default, _)| *default == item)
        })
        .map_or(item, |function| function.item)
}

/// An error at the name of `item`.
pub(super) fn at(db: &dyn TypeDb, item: Item<'_>, message: impl Into<String>) -> Diagnostic {
    let message = message.into();
    let tree = db.parse(item.file(db)).tree();
    Diagnostic::error(message.clone()).with_primary(tree.span(item.name(db)), message)
}

/// What each struct or enum of `package` declares, as layouts, the passes and the backend read it.
pub(super) fn declared<'db>(
    db: &'db dyn TypeDb,
    package: Package,
) -> impl Fn(&Nominal<'db>) -> types::Declared<'db> {
    move |nominal| checker::declared(db, package, nominal)
}

/// The `__del__` a drop of each struct of `package` calls first, when its class has one
/// (decision 0063): the method's symbol, the class's name for the panic when it raises, and
/// the errors it can raise.
pub(super) fn destructors<'db>(
    db: &'db dyn TypeDb,
    package: Package,
) -> impl Fn(&Nominal<'db>) -> Option<llvm_backend::Destructor<'db>> {
    move |nominal| {
        let (item, fallible) = checker::destructor(db, package, nominal)?;
        let errors = match &db.declaration(item).declaration {
            Declaration::Function(signature) if fallible => match signature.returns.kind(db) {
                TypeKind::Fallible { errors, .. } => errors.clone(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        };
        Some(llvm_backend::Destructor {
            symbol: symbol(db, package, item, &[]),
            class: names(db, package)(nominal).class,
            errors,
        })
    }
}

/// The names `print` writes for each struct or enum of `package`: the class's name, then its
/// fields' or members' names in declaration order.
pub(super) fn names<'db>(
    db: &'db dyn TypeDb,
    package: Package,
) -> impl Fn(&Nominal<'db>) -> llvm_backend::ClassNames {
    move |nominal| {
        let bare = Nominal {
            arguments: Vec::new(),
            ..nominal.clone()
        };
        let item = Item::new(db, package, nominal.file, nominal.name);
        let members = match &db.declaration(item).declaration {
            Declaration::Struct { fields, .. } => {
                fields.iter().map(|field| field.name.clone()).collect()
            }
            Declaration::Enum { variants } => variants.clone(),
            _ => Vec::new(),
        };
        llvm_backend::ClassNames {
            class: types::Type::new(db, TypeKind::Struct(bare))
                .display(db)
                .to_string(),
            members,
        }
    }
}
