//! Tests of bounds-check hoisting on hand-built loops: which checks are hoisted, what the block
//! before the loop compares, and which loops are left as they were.

use crate::hoist;
use mir::{
    BinaryOperator, BlockId, Body, Check, Constant, Local, LocalKind, Operand, Place, Projection,
    Rvalue, Statement, StatementKind, Terminator, TerminatorKind, UnaryOperator,
};
use source::{FileId, Span};
use types::{Float, Integer, Mode, Primitive, Type, TypeKind};

/// A database with no files: hoisting never reads a name.
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
        unreachable!("hoisting never reads a file")
    }

    fn resolve(&self, _: database::Package, _: database::File) -> &database::Resolution {
        unreachable!("hoisting never resolves names")
    }
}

/// The span every test statement carries.
fn span() -> Span {
    Span::new(FileId::from_index(0), 0, 1)
}

/// The blocks of a body under construction, before they are added to it: each one's
/// statements and terminator.
type Blocks<'db> = Vec<(Vec<Statement<'db>>, TerminatorKind<'db>)>;

/// A function `f(n: i64, k: i64, a: Array[f64, 1])` (locals 1 to 3) with the loop
/// `for i in range(0, n)`, whose counter is local 4. Its blocks are the entry, the header
/// (block 1), the blocks [`Builder::finish`] is given from block 2, the latch, then the exit.
struct Builder<'db> {
    /// The body.
    body: Body<'db>,
    /// `i64`.
    int: Type<'db>,
    /// `bool`.
    boolean: Type<'db>,
}

/// The parameter `n`, where the loop stops.
fn n() -> Local {
    Local::new(1)
}

/// The parameter `k`, an index the loop does not change.
fn k() -> Local {
    Local::new(2)
}

/// The parameter `a`, the array indexed.
fn a() -> Local {
    Local::new(3)
}

/// The loop's counter.
fn counter() -> Local {
    Local::new(4)
}

impl<'db> Builder<'db> {
    /// The body's parameters and counter.
    fn new(database: &'db Database) -> Self {
        let int = Type::new(
            database,
            TypeKind::Primitive(Primitive::Integer(Integer::I64)),
        );
        let boolean = Type::new(database, TypeKind::Primitive(Primitive::Bool));
        let float = Type::new(database, TypeKind::Primitive(Primitive::Float(Float::F64)));
        let vector = Type::new(
            database,
            TypeKind::Array {
                element: float,
                rank: 1,
            },
        );
        let mut body = Body::new(int, span());
        for (name, ty) in [("n", int), ("k", int), ("a", vector)] {
            let kind = LocalKind::Parameter {
                name: name.into(),
                mode: Mode::Borrow,
            };
            body.push_local(ty, kind, span());
        }
        body.push_local(int, LocalKind::Temporary, span());
        Self { body, int, boolean }
    }

    /// A new temporary of type `ty`.
    fn temporary(&mut self, ty: Type<'db>) -> Local {
        self.body.push_local(ty, LocalKind::Temporary, span())
    }

    /// The `i64` constant `value`.
    fn integer(&self, value: i128) -> Operand<'db> {
        Operand::Constant(Constant::integer(self.int, value))
    }

    /// The statements and terminator lowering writes to check that `index` lies in
    /// `0..length`, then go to block `target`.
    fn check(
        &mut self,
        index: Local,
        length: Operand<'db>,
        target: usize,
    ) -> (Vec<Statement<'db>>, TerminatorKind<'db>) {
        let [above, below, inside] = [(); 3].map(|()| self.temporary(self.boolean));
        let statements = vec![
            assign(
                above,
                Rvalue::Binary(BinaryOperator::GreaterEqual, copy(index), self.integer(0)),
            ),
            assign(
                below,
                Rvalue::Binary(BinaryOperator::Less, copy(index), length.clone()),
            ),
            assign(
                inside,
                Rvalue::Binary(BinaryOperator::BitAnd, copy(above), copy(below)),
            ),
        ];
        let terminator = TerminatorKind::Assert {
            condition: copy(inside),
            check: Check::IndexInBounds {
                dimension: 0,
                index: copy(index),
                length,
            },
            target: BlockId::new(target),
        };
        (statements, terminator)
    }

    /// The body with `blocks` between the header and the latch. The first is block 2, and the
    /// last goes to the latch, block `2 + blocks.len()`.
    fn finish(mut self, blocks: Blocks<'db>) -> Body<'db> {
        let latch = 2 + blocks.len();
        let test = self.temporary(self.boolean);
        let mut all: Blocks<'db> = vec![
            (
                vec![assign(counter(), Rvalue::Use(self.integer(0)))],
                TerminatorKind::Goto(BlockId::new(1)),
            ),
            (
                vec![assign(
                    test,
                    Rvalue::Binary(BinaryOperator::Less, copy(counter()), copy(n())),
                )],
                TerminatorKind::Branch {
                    condition: copy(test),
                    then: BlockId::new(2),
                    otherwise: BlockId::new(latch + 1),
                },
            ),
        ];
        all.extend(blocks);
        all.push((
            vec![assign(
                counter(),
                Rvalue::Binary(BinaryOperator::Add, copy(counter()), self.integer(1)),
            )],
            TerminatorKind::Goto(BlockId::new(1)),
        ));
        all.push((Vec::new(), TerminatorKind::Return));
        for (statements, kind) in all {
            let id = self.body.push_block(Terminator { kind, span: span() });
            self.body.blocks[id.index()].statements = statements;
        }
        self.body
    }
}

/// `local = rvalue`.
fn assign(local: Local, rvalue: Rvalue<'_>) -> Statement<'_> {
    Statement {
        kind: StatementKind::Assign(Place::from(local), rvalue),
        span: span(),
    }
}

/// `copy local`.
fn copy(local: Local) -> Operand<'static> {
    Operand::Copy(Place::from(local))
}

/// How many blocks of `body` end in an assert.
fn asserts(body: &Body<'_>) -> usize {
    body.blocks
        .iter()
        .filter(|block| matches!(block.terminator.kind, TerminatorKind::Assert { .. }))
        .count()
}

/// `a[i + 1]`, with the length read in the loop and the addition checked for overflow: the copy
/// keeps neither check, the loop as it was keeps both, and the block before them reads the
/// length again and compares `0 >= -1` and `n <= len(a) - 1`.
#[test]
fn hoists_a_shifted_counter_and_its_overflow_check() {
    let database = Database::default();
    let mut builder = Builder::new(&database);
    let pair = Type::new(
        &database,
        TypeKind::Tuple(vec![builder.int, builder.boolean]),
    );
    let [sum, flag, index, length] =
        [pair, builder.boolean, builder.int, builder.int].map(|ty| builder.temporary(ty));
    let (mut statements, check) = builder.check(index, copy(length), 4);
    statements.splice(
        0..0,
        [
            assign(
                sum,
                Rvalue::CheckedBinary(BinaryOperator::Add, copy(counter()), builder.integer(1)),
            ),
            assign(
                flag,
                Rvalue::Unary(
                    UnaryOperator::Not,
                    Operand::Copy(Place::from(sum).project(Projection::Field(1))),
                ),
            ),
        ],
    );
    let overflow = TerminatorKind::Assert {
        condition: copy(flag),
        check: Check::NoOverflow(BinaryOperator::Add),
        target: BlockId::new(3),
    };
    let after = vec![
        assign(
            index,
            Rvalue::Use(Operand::Copy(
                Place::from(sum).project(Projection::Field(0)),
            )),
        ),
        assign(length, Rvalue::Length(Place::from(a()), 0)),
    ];
    let first = statements.split_off(2);
    let mut second = after;
    second.extend(first);
    let mut body = builder.finish(vec![(statements, overflow), (second, check)]);
    hoist(&database, &mut body, &|_| {
        types::Declared::Struct(Vec::new())
    });

    assert!(body.validate(&database).is_empty());
    // Entry, header, two checking blocks, latch, exit; the copy of the four loop blocks; the
    // block before them.
    assert_eq!(body.blocks.len(), 6 + 4 + 1);
    assert_eq!(asserts(&body), 2);
    // The copies of the header and of the two checking blocks.
    for (block, target) in [(7, 8), (8, 9)] {
        assert_eq!(
            body.blocks[block].terminator.kind,
            TerminatorKind::Goto(BlockId::new(target))
        );
    }
    let text = body.display(&database).to_string();
    assert!(text.contains("_13 = len(_3, 0);"), "{text}");
    assert!(text.contains("GreaterEqual(copy _4, -1_i64)"), "{text}");
    assert!(text.contains("LessEqual(copy _1, copy "), "{text}");
}

/// `a[k]`, `k` a parameter: the block before the loop checks `k >= 0` and `k < len(a)`, so a
/// loop that runs no pass still runs the copy and never reads `a[k]`.
#[test]
fn hoists_an_index_the_loop_does_not_change() {
    let database = Database::default();
    let mut builder = Builder::new(&database);
    let length = builder.temporary(builder.int);
    let (mut statements, check) = builder.check(k(), copy(length), 3);
    statements.insert(0, assign(length, Rvalue::Length(Place::from(a()), 0)));
    let mut body = builder.finish(vec![(statements, check)]);
    hoist(&database, &mut body, &|_| {
        types::Declared::Struct(Vec::new())
    });

    assert!(body.validate(&database).is_empty());
    assert_eq!(body.blocks.len(), 5 + 3 + 1);
    let text = body.display(&database).to_string();
    assert!(text.contains("GreaterEqual(copy _2, 0_i64)"), "{text}");
    assert!(text.contains("Less(copy _2, copy "), "{text}");
}

/// Checks the loop cannot make before it starts are left where they are, and a loop with only
/// those is not copied: an index scaled by 2, an array the loop replaces, a counter the body
/// also writes, and a negative constant index, which always fails.
#[test]
fn leaves_checks_it_cannot_hoist() {
    let database = Database::default();
    let cases: [for<'a, 'db> fn(&'a mut Builder<'db>) -> Blocks<'db>; 4] = [
        |builder| {
            let index = builder.temporary(builder.int);
            let (mut statements, check) = builder.check(index, builder.integer(8), 3);
            statements.insert(
                0,
                assign(
                    index,
                    Rvalue::Binary(
                        BinaryOperator::Multiply,
                        copy(counter()),
                        builder.integer(2),
                    ),
                ),
            );
            vec![(statements, check)]
        },
        |builder| {
            let length = builder.temporary(builder.int);
            let (mut statements, check) = builder.check(counter(), copy(length), 3);
            statements.insert(0, assign(length, Rvalue::Length(Place::from(a()), 0)));
            statements.push(Statement {
                kind: StatementKind::Drop(Place::from(a())),
                span: span(),
            });
            vec![(statements, check)]
        },
        |builder| {
            let (mut statements, check) = builder.check(counter(), builder.integer(8), 3);
            statements.push(assign(counter(), Rvalue::Use(builder.integer(0))));
            vec![(statements, check)]
        },
        |builder| {
            let index = builder.temporary(builder.int);
            let (mut statements, check) = builder.check(index, builder.integer(8), 3);
            statements.insert(0, assign(index, Rvalue::Use(builder.integer(-1))));
            vec![(statements, check)]
        },
    ];
    for (case, blocks) in cases.into_iter().enumerate() {
        let mut builder = Builder::new(&database);
        let blocks = blocks(&mut builder);
        let mut body = builder.finish(blocks);
        let before = body.blocks.len();
        hoist(&database, &mut body, &|_| {
            types::Declared::Struct(Vec::new())
        });
        assert_eq!(body.blocks.len(), before, "case {case}");
    }
}

mod unit;
