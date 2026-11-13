//! The `h5by` module (decision 0052): `h5by.File(path)` and a subscript of a dataset, each a
//! call of a [`Builtin::ArrayFile`] that can fail. Everything else a program asks of a `File`,
//! a `Group` or a `Dataset` is a call of a library function the checker recorded where the
//! program wrote it, lowered as any call is.
//!
//! - `h5by.File(path)` writes the file's description lines straight into the `File`'s one
//!   field, a `list[str]`.
//! - `dset[...]` or `dset[0, ::2]` is three calls, since deflate is decoded in Baryon, by the
//!   standard library's `zlib` (decision 0049), not by the runtime: the runtime lists the
//!   deflated chunks the subscript touches, the library's `_inflate` reads and inflates them,
//!   and the runtime reads the values, taking the inflated chunks back. Each passes the path and
//!   the name the `Dataset` holds, its fields 5 and 0, and the four `int`s of each item of the
//!   subscript; the element type and rank are the destination's. A dataset with no deflated
//!   chunk lists none, and `_inflate` then reads nothing.
//!
//! Each call's error edge goes where any failing call's does (the `errors` module).

use crate::builder::Builder;
use mir::{Argument, ArrayFile, Builtin, Callee, Place, Projection};
use syntax::{NodeId, Role, SyntaxKind};
use types::{Item, Nominal, Type, TypeKind};

/// The position of a `Dataset`'s `name` field in `<std>/h5by.byn`.
const NAME: u32 = 0;

/// The position of a `Dataset`'s `_path` field in `<std>/h5by.byn`.
const PATH: u32 = 5;

impl<'db> Builder<'db> {
    /// The call `node` of the `h5by` built-in `name` into `place`; `arguments` are as the
    /// checker recorded them.
    pub(crate) fn h5by_call(
        &mut self,
        place: Place,
        name: &str,
        arguments: &[Option<NodeId>],
        node: NodeId,
    ) {
        match (name, arguments) {
            ("h5by.File", &[Some(path)]) => {
                let path = self.text_argument(path);
                // The `File`'s one field, its description lines.
                let lines = place.project(Projection::Field(0));
                let open = Builtin::ArrayFile(ArrayFile::Hdf5Open);
                self.raising_call(open, vec![path], lines, node);
            }
            ("h5by.read", [Some(dataset), items @ ..]) => {
                self.h5by_read(place, *dataset, items, node);
            }
            _ => self.unsupported(node, "these arguments of an `h5by` call"),
        }
    }

    /// `dset[items]`, the subscript `node` of the `Dataset` `dataset`, into `place`.
    fn h5by_read(&mut self, place: Place, dataset: NodeId, items: &[Option<NodeId>], node: NodeId) {
        let dataset = self.borrow(dataset);
        let path = dataset.clone().project(Projection::Field(PATH));
        let name = dataset.project(Projection::Field(NAME));
        let methods = (ArrayFile::Hdf5Chunks, ArrayFile::Hdf5Read);
        self.inflated_read(place, methods, (path, name), items, node);
    }

    /// The read `node` of the values `items` pick of the HDF5 dataset or `NetCDF` variable
    /// `name` in the file at `path`, both borrowed `str`s, into `place`: the deflated chunks
    /// listed by `methods.0`, inflated by `<std>/h5by.byn`'s `_inflate`, then the values read
    /// by `methods.1`, which takes the inflated chunks back. A `NetCDF` read shares the `h5by`
    /// library's inflater, since a `NetCDF-4` file is an HDF5 file (decision 0052).
    pub(crate) fn inflated_read(
        &mut self,
        place: Place,
        methods: (ArrayFile, ArrayFile),
        (path, name): (Place, Place),
        items: &[Option<NodeId>],
        node: NodeId,
    ) {
        let (Some(inflate), Some(inflated)) = (
            self.h5by_function(if items.is_empty() {
                "_inflate"
            } else {
                "_inflate_picked"
            }),
            self.h5by_class("_Inflated"),
        ) else {
            return self.unsupported(node, "a compressed read without the standard library");
        };
        let words = self.subscript_words(items);
        let text = Type::new(self.db, TypeKind::Str);
        let lines = Type::new(self.db, TypeKind::List(text));
        let chunks = self.temporary(lines, node);
        self.drop_later(chunks);
        let mut given = vec![
            Argument::Borrow(path.clone()),
            Argument::Borrow(name.clone()),
        ];
        given.extend(words.iter().cloned());
        self.raising_call(
            Builtin::ArrayFile(methods.0),
            given,
            Place::from(chunks),
            node,
        );
        let bytes = self.temporary(inflated, node);
        self.drop_later(bytes);
        let callee = Callee::Function {
            item: inflate,
            type_arguments: Vec::new(),
        };
        let given = vec![
            Argument::Borrow(path.clone()),
            Argument::Borrow(Place::from(chunks)),
        ];
        self.raising_callee(callee, given, Place::from(bytes), node);
        let mut given = vec![
            Argument::Borrow(path),
            Argument::Borrow(name),
            Argument::Borrow(Place::from(bytes).project(Projection::Field(0))),
            Argument::Borrow(Place::from(bytes).project(Projection::Field(1))),
        ];
        given.extend(words);
        self.raising_call(Builtin::ArrayFile(methods.1), given, place, node);
    }

    /// The function `name` that `<std>/h5by.byn` defines at its top level.
    fn h5by_function(&self, name: &str) -> Option<Item<'db>> {
        let (file, function) = self.h5by_definition(SyntaxKind::FunctionDefinition, name)?;
        Some(Item::new(self.db, self.package, file, function))
    }

    /// The type of the class `name` that `<std>/h5by.byn` defines at its top level.
    fn h5by_class(&self, name: &str) -> Option<Type<'db>> {
        let (file, class) = self.h5by_definition(SyntaxKind::ClassDefinition, name)?;
        Some(Type::new(
            self.db,
            TypeKind::Struct(Nominal {
                file,
                name: class,
                arguments: Vec::new(),
            }),
        ))
    }

    /// The file `<std>/h5by.byn` and the name node of its top-level definition of `kind`
    /// named `name`. Costs one pass over the file's top-level statements.
    fn h5by_definition(&self, kind: SyntaxKind, name: &str) -> Option<(database::File, NodeId)> {
        let file = database::modules(self.db, self.package).library("h5by")?;
        let tree = self.db.parse(file).tree();
        let text = file.source(self.db).text();
        let found = tree
            .children_with(tree.root(), Role::Body)
            .filter(|&statement| tree.kind(statement) == kind)
            .filter_map(|statement| tree.child(statement, Role::Name))
            .find(|&node| tree.text(node, text) == name)?;
        Some((file, found))
    }
}
