//! Dictionaries: the empty `dict`, finding, inserting, removing, stepping through entries,
//! `len`, the missing-key panic and the drop, against a stand-in for the runtime's table.

use super::{Database, assign, build, end, primitive, run, span};
use mir::{
    Argument, BasicBlock, BinaryOperator, BlockId, Body, Builtin, Callee, Check, Constant,
    ConstantValue, Local, LocalKind, Operand, Place, Projection, Rvalue, Statement, StatementKind,
    TerminatorKind,
};
use types::{Integer, Primitive, Type, TypeKind};

/// The runtime's `dict` functions in C, as a list searched in order rather than a hash table:
/// enough to check that compiled code passes the right arguments and reaches each entry at
/// `entries + position * entry_size + 8`. Keys here are integers, compared byte for byte.
const DICT_RUNTIME: &str = r#"
    #include <stdint.h>
    #include <stdio.h>
    #include <stdlib.h>
    #include <string.h>
    typedef struct { int64_t entry_size, key_size, part_count; const int64_t *parts; } Layout;
    typedef struct { char *entries; int64_t live, used, capacity; } Table;
    static char *at(Table *t, const Layout *l, int64_t p) { return t->entries + p * l->entry_size; }
    int64_t baryon_rt_dict_find(Table *t, const Layout *l, const void *key) {
        if (!t) return -1;
        for (int64_t p = 0; p < t->used; p++) {
            char *e = at(t, l, p);
            if (*(uint64_t *)e != UINT64_MAX && !memcmp(e + 8, key, l->key_size)) return p;
        }
        return -1;
    }
    int64_t baryon_rt_dict_insert(Table **slot, const Layout *l, const void *key, const void *loc) {
        (void)loc;
        Table *t = *slot;
        if (!t) { t = calloc(1, sizeof *t); *slot = t; }
        if (t->used == t->capacity) {
            t->capacity = t->capacity ? 2 * t->capacity : 4;
            t->entries = realloc(t->entries, t->capacity * l->entry_size);
        }
        char *e = at(t, l, t->used);
        *(uint64_t *)e = 0;
        memcpy(e + 8, key, l->key_size);
        t->live++;
        return t->used++;
    }
    void baryon_rt_dict_remove(Table *t, const Layout *l, int64_t p) {
        *(uint64_t *)at(t, l, p) = UINT64_MAX;
        t->live--;
    }
    int64_t baryon_rt_dict_next(Table *t, const Layout *l, int64_t p) {
        if (!t) return -1;
        for (; p < t->used; p++) if (*(uint64_t *)at(t, l, p) != UINT64_MAX) return p;
        return -1;
    }
    int64_t baryon_rt_dict_length(Table *t) { return t ? t->live : 0; }
    void baryon_rt_dict_free(Table *t) {
        if (!t) { printf("freed an empty dict\n"); return; }
        printf("freed a dict of %lld\n", (long long)t->live);
        free(t->entries);
        free(t);
    }
    void baryon_rt_panic_key(const char *key, int64_t length, const void *loc) {
        (void)loc;
        printf("panic: key %.*s is not in the dict\n", (int)length, key);
        exit(101);
    }
"#;

/// Builds a body block by block: each call goes to the next block.
struct Blocks<'db> {
    /// The body built.
    body: Body<'db>,
    /// The statements of the block being written.
    statements: Vec<Statement<'db>>,
}

impl<'db> Blocks<'db> {
    /// An empty body returning `returns`.
    fn new(returns: Type<'db>) -> Self {
        Self {
            body: Body::new(returns, span()),
            statements: Vec::new(),
        }
    }

    /// A new temporary of type `ty`.
    fn local(&mut self, ty: Type<'db>) -> Local {
        self.body.push_local(ty, LocalKind::Temporary, span())
    }

    /// Adds a statement to the current block.
    fn statement(&mut self, statement: Statement<'db>) {
        self.statements.push(statement);
    }

    /// Ends the current block with `terminator`, given the next block's id.
    fn end(&mut self, terminator: impl FnOnce(BlockId) -> TerminatorKind<'db>) {
        let next = BlockId::new(self.body.blocks.len() + 1);
        let block = self.body.push_block(end(terminator(next)));
        self.body.blocks[block.index()].statements = std::mem::take(&mut self.statements);
    }

    /// Calls `builtin` with `arguments` into `destination`.
    fn call(&mut self, builtin: Builtin, arguments: Vec<Argument<'db>>, destination: Local) {
        self.end(|target| TerminatorKind::Call {
            callee: Callee::Builtin(builtin),
            arguments,
            destination: destination.into(),
            target,
            error: None,
        });
    }

    /// Ends the body with a return.
    fn finish(mut self) -> Body<'db> {
        let statements = std::mem::take(&mut self.statements);
        self.body.blocks.push(BasicBlock {
            statements,
            terminator: end(TerminatorKind::Return),
        });
        self.body
    }
}

/// The value of the entry at the position in `position` of the `dict` in `dict`.
fn value(dict: Local, position: Local) -> Place {
    Place::from(dict)
        .project(Projection::Index(vec![position]))
        .project(Projection::Field(1))
}

/// `d = {}; d[3] = 2; d[7] = 4; x = d.pop(3)`, then `len(d) * 100 + next * 10 + x`, where
/// `next` is the first position holding a key, and `d` dropped: every operation reaches the
/// table and its entries where the runtime put them.
#[test]
fn entries_are_inserted_found_removed_and_freed() {
    let database = Database::default();
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let dict = Type::new(
        &database,
        TypeKind::Dict {
            key: i64,
            value: i64,
        },
    );
    let number = |value: i128| Operand::Constant(Constant::integer(i64, value));
    let mut blocks = Blocks::new(i64);
    let d = blocks.local(dict);
    let key = blocks.local(i64);
    let position = blocks.local(i64);
    let removed = blocks.local(i64);
    let next = blocks.local(i64);
    let length = blocks.local(i64);
    let scratch = blocks.local(i64);
    blocks.statement(assign(d, Rvalue::Aggregate(dict, Vec::new())));
    for (k, v) in [(3, 2), (7, 4)] {
        blocks.statement(assign(key, Rvalue::Use(number(k))));
        blocks.call(
            Builtin::DictInsert,
            vec![
                Argument::InOut(d.into()),
                Argument::Owned(Operand::Move(key.into())),
            ],
            position,
        );
        blocks.statement(assign(value(d, position), Rvalue::Use(number(v))));
    }
    blocks.statement(assign(key, Rvalue::Use(number(3))));
    blocks.call(
        Builtin::DictFind,
        vec![Argument::Borrow(d.into()), Argument::Borrow(key.into())],
        position,
    );
    blocks.call(
        Builtin::DictRemove,
        vec![
            Argument::InOut(d.into()),
            Argument::Owned(Operand::Copy(position.into())),
        ],
        removed,
    );
    blocks.call(
        Builtin::DictNext,
        vec![Argument::Borrow(d.into()), Argument::Owned(number(0))],
        next,
    );
    let copy = |local: Local| Operand::Copy(local.into());
    blocks.statement(assign(length, Rvalue::Length(d.into(), 0)));
    blocks.statement(assign(
        scratch,
        Rvalue::Binary(BinaryOperator::Multiply, copy(length), number(100)),
    ));
    blocks.statement(assign(
        next,
        Rvalue::Binary(BinaryOperator::Multiply, copy(next), number(10)),
    ));
    blocks.statement(assign(
        scratch,
        Rvalue::Binary(BinaryOperator::Add, copy(scratch), copy(next)),
    ));
    blocks.statement(assign(
        Local::RETURN,
        Rvalue::Binary(BinaryOperator::Add, copy(scratch), copy(removed)),
    ));
    blocks.statement(Statement {
        kind: StatementKind::Drop(d.into()),
        span: span(),
    });
    let text = super::module(&database, &[("tally", blocks.finish())]);
    assert!(
        text.contains("@baryon.dict.parts.0 = private unnamed_addr constant [1 x { i64, i64 }]")
    );
    assert!(text.contains("{ i64 24, i64 8, i64 1, ptr @baryon.dict.parts.0 }"));
    let main = format!(
        "{DICT_RUNTIME}
        int64_t tally(void);
        int main(void) {{
            printf(\"%lld\\n\", (long long)tally());
            return 0;
        }}"
    );
    let executable = build("dict-tally", &text, &main);
    assert_eq!(
        run(&executable, &[]).as_deref(),
        Ok("freed a dict of 1\n112\n")
    );
}

/// `{}[5]`: a key that is not there panics, naming the key as the check's text gives it.
#[test]
fn a_missing_key_panics_naming_it() {
    let database = Database::default();
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let bool = primitive(&database, Primitive::Bool);
    let str = Type::new(&database, TypeKind::Str);
    let dict = Type::new(
        &database,
        TypeKind::Dict {
            key: i64,
            value: i64,
        },
    );
    let mut blocks = Blocks::new(i64);
    let d = blocks.local(dict);
    let key = blocks.local(i64);
    let position = blocks.local(i64);
    let found = blocks.local(bool);
    blocks.statement(assign(d, Rvalue::Aggregate(dict, Vec::new())));
    blocks.statement(assign(
        key,
        Rvalue::Use(Operand::Constant(Constant::integer(i64, 5))),
    ));
    blocks.call(
        Builtin::DictFind,
        vec![Argument::Borrow(d.into()), Argument::Borrow(key.into())],
        position,
    );
    blocks.statement(assign(
        found,
        Rvalue::Binary(
            BinaryOperator::GreaterEqual,
            Operand::Copy(position.into()),
            Operand::Constant(Constant::integer(i64, 0)),
        ),
    ));
    let text_of_key = Operand::Constant(Constant {
        ty: str,
        value: ConstantValue::Str("5".to_owned()),
    });
    blocks.end(|target| TerminatorKind::Assert {
        condition: Operand::Copy(found.into()),
        check: Check::KeyPresent { key: text_of_key },
        target,
    });
    blocks.statement(assign(
        Local::RETURN,
        Rvalue::Use(Operand::Copy(value(d, position))),
    ));
    let text = super::module(&database, &[("missing", blocks.finish())]);
    let main = format!(
        "{DICT_RUNTIME}
        int64_t missing(void);
        int main(void) {{
            printf(\"%lld\\n\", (long long)missing());
            return 0;
        }}"
    );
    let executable = build("dict-missing", &text, &main);
    assert_eq!(
        run(&executable, &[]),
        Err("panic: key 5 is not in the dict\n".to_owned())
    );
}

/// `d = {1: [1, 2], 2: [3]}` then `d` dropped: dropping a `dict` whose values own memory frees
/// each value, in insertion order, before the table.
#[test]
fn dropping_a_dict_drops_its_values() {
    let database = Database::default();
    let i64 = primitive(&database, Primitive::Integer(Integer::I64));
    let list = Type::new(&database, TypeKind::List(i64));
    let dict = Type::new(
        &database,
        TypeKind::Dict {
            key: i64,
            value: list,
        },
    );
    let none = Type::new(&database, TypeKind::None);
    let number = |value: i128| Operand::Constant(Constant::integer(i64, value));
    let mut blocks = Blocks::new(none);
    let d = blocks.local(dict);
    let key = blocks.local(i64);
    let position = blocks.local(i64);
    blocks.statement(assign(d, Rvalue::Aggregate(dict, Vec::new())));
    for (k, items) in [(1, vec![number(1), number(2)]), (2, vec![number(3)])] {
        blocks.statement(assign(key, Rvalue::Use(number(k))));
        blocks.call(
            Builtin::DictInsert,
            vec![
                Argument::InOut(d.into()),
                Argument::Owned(Operand::Move(key.into())),
            ],
            position,
        );
        blocks.statement(assign(value(d, position), Rvalue::Aggregate(list, items)));
    }
    blocks.statement(Statement {
        kind: StatementKind::Drop(d.into()),
        span: span(),
    });
    let text = super::module(&database, &[("nest", blocks.finish())]);
    let main = format!(
        "{DICT_RUNTIME}
        void nest(void);
        int main(void) {{
            nest();
            return 0;
        }}"
    );
    let executable = build("dict-drop", &text, &main);
    assert_eq!(
        run(&executable, &[]).as_deref(),
        Ok("freed 16 bytes aligned to 8\nfreed 8 bytes aligned to 8\nfreed a dict of 2\n")
    );
}
