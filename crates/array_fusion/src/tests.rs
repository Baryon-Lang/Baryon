//! Tests of array fusion, compared by the text form of the fused body.

use crate::fuse;
use mir::{
    BinaryOperator, Body, Elementwise, Local, LocalKind, Operand, Place, Rvalue, Statement,
    StatementKind, Terminator, TerminatorKind,
};
use source::{FileId, Span};
use types::{Float, Integer, Mode, Primitive, Type, TypeKind};

/// A database with no files: fusion never reads a name.
#[salsa::db]
#[derive(Clone, Default)]
struct Database {
    /// Salsa's inputs and caches.
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for Database {}

#[salsa::db]
impl database::Db for Database {
    fn parse(&self, _: database::File) -> &database::ParsedFile {
        unreachable!("fusion never reads a file")
    }

    fn resolve(&self, _: database::Package, _: database::File) -> &database::Resolution {
        unreachable!("fusion never resolves names")
    }
}

/// The span every test statement carries; the text form does not show spans.
fn span() -> Span {
    Span::new(FileId::from_index(0), 0, 1)
}

/// A body under construction: `Array[element, 1]` parameters `a` and `b` (locals 1 and 2), the
/// variables `t` and `u` (locals 3 and 4) and unnamed temporaries 5 and 6, all of that type,
/// returning one.
struct Builder<'db> {
    /// The database types are interned in.
    database: &'db Database,
    /// The body.
    body: Body<'db>,
}

impl<'db> Builder<'db> {
    /// The body's locals for arrays of `element`.
    fn new(database: &'db Database, element: Primitive) -> Self {
        let element = Type::new(database, TypeKind::Primitive(element));
        let vector = Type::new(database, TypeKind::Array { element, rank: 1 });
        let mut body = Body::new(vector, span());
        for name in ["a", "b"] {
            let kind = LocalKind::Parameter {
                name: name.into(),
                mode: Mode::Borrow,
            };
            body.push_local(vector, kind, span());
        }
        for name in ["t", "u"] {
            let kind = LocalKind::Variable { name: name.into() };
            body.push_local(vector, kind, span());
        }
        body.push_local(vector, LocalKind::Temporary, span());
        body.push_local(vector, LocalKind::Temporary, span());
        Self { database, body }
    }

    /// Adds a block holding `statements` that returns, or jumps to the next block when `goto`.
    fn block(mut self, statements: Vec<Statement<'db>>, goto: bool) -> Self {
        let next = mir::BlockId::new(self.body.blocks.len() + 1);
        let kind = if goto {
            TerminatorKind::Goto(next)
        } else {
            TerminatorKind::Return
        };
        let block = self.body.push_block(Terminator { kind, span: span() });
        self.body.blocks[block.index()].statements = statements;
        self
    }

    /// The body fused, checked valid, as text.
    fn fused(mut self) -> String {
        fuse(self.database, &mut self.body);
        assert_eq!(self.body.validate(self.database), Vec::new());
        self.body.display(self.database).to_string()
    }
}

/// Local `index`.
fn local(index: usize) -> Local {
    Local::new(index)
}

/// `left op right` over two locals.
fn binary<'db>(operator: BinaryOperator, left: usize, right: usize) -> Elementwise<'db> {
    let leaf = |index| Box::new(Elementwise::Leaf(Operand::Copy(local(index).into())));
    Elementwise::Binary(operator, leaf(left), leaf(right))
}

/// `destination = elementwise tree`.
fn compute(destination: usize, tree: Elementwise<'_>) -> Statement<'_> {
    statement(StatementKind::Assign(
        local(destination).into(),
        Rvalue::Elementwise(Box::new(tree)),
    ))
}

/// `destination = move source`, as lowering stores a temporary in a variable.
fn store<'db>(destination: usize, source: usize) -> Statement<'db> {
    statement(StatementKind::Assign(
        local(destination).into(),
        Rvalue::Use(Operand::Move(local(source).into())),
    ))
}

/// `drop(local)`.
fn drop<'db>(index: usize) -> Statement<'db> {
    statement(StatementKind::Drop(Place::from(local(index))))
}

/// A statement at the test span.
fn statement(kind: StatementKind<'_>) -> Statement<'_> {
    Statement { kind, span: span() }
}

/// The statements of `t = a * b`, then `return t + b`, as lowering writes them.
fn steps<'db>() -> Vec<Statement<'db>> {
    vec![
        compute(5, binary(BinaryOperator::Multiply, 1, 2)),
        drop(3),
        store(3, 5),
        compute(0, binary(BinaryOperator::Add, 3, 2)),
        drop(3),
    ]
}

/// `t = a * b` through a temporary, then `return t + b`: the computation moves into the
/// return, and the store of `t` goes; its drops stay, for drop elaboration to remove.
#[test]
fn a_temporary_read_once_is_fused_into_its_reader() {
    let database = Database::default();
    let text = Builder::new(&database, Primitive::Float(Float::F64))
        .block(steps(), false)
        .fused();
    assert!(
        text.ends_with(
            "    bb0: {
        drop(_3);
        _0 = elementwise Add(fused(Multiply(copy _1, copy _2)), copy _2);
        drop(_3);
        return;
    }
}
"
        ),
        "{text}"
    );
}

/// `t = a * b`, `u = t + a`, `return u * b`: both fuse, the first inside the second.
#[test]
fn chains_fuse_into_one_expression() {
    let database = Database::default();
    let text = Builder::new(&database, Primitive::Complex(types::Complex::C128))
        .block(
            vec![
                compute(3, binary(BinaryOperator::Multiply, 1, 2)),
                compute(4, binary(BinaryOperator::Add, 3, 1)),
                compute(0, binary(BinaryOperator::Multiply, 4, 2)),
            ],
            false,
        )
        .fused();
    assert!(
        text.contains(
            "        _0 = elementwise Multiply(fused(Add(fused(Multiply(copy _1, copy _2)), \
             copy _1)), copy _2);\n        return;"
        ),
        "{text}"
    );
}

/// Each case keeps its statements as they are: `t` read twice; `a`, which `t` reads, dropped
/// between; integer elements, whose overflow checks would change order; `t` assigned in one
/// block and read in the next.
#[test]
fn temporaries_that_cannot_be_fused_stay() {
    let database = Database::default();
    let float = Primitive::Float(Float::F64);
    let unchanged = |builder: Builder<'_>| {
        let before = builder.body.display(builder.database).to_string();
        assert_eq!(builder.fused(), before);
    };

    let mut read_twice = steps();
    read_twice.insert(3, compute(6, binary(BinaryOperator::Add, 3, 3)));
    unchanged(Builder::new(&database, float).block(read_twice, false));

    let mut changed_between = steps();
    changed_between.insert(3, drop(1));
    unchanged(Builder::new(&database, float).block(changed_between, false));

    let integer = Primitive::Integer(Integer::I64);
    unchanged(Builder::new(&database, integer).block(steps(), false));

    let mut first = steps();
    let second = first.split_off(3);
    unchanged(
        Builder::new(&database, float)
            .block(first, true)
            .block(second, false),
    );
}
