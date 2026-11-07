//! Unit tests of the driver: which queries a session runs, and what the type queries return.

use super::*;
use database::Definition;
use std::sync::{Arc, Mutex};
use syntax::Role;
use types::{Declaration, Mode, Value};

/// A session that records the name of every query it runs, in order.
fn logged() -> (Session, Arc<Mutex<Vec<String>>>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&log);
    let storage = salsa::Storage::new(Some(Box::new(move |event: salsa::Event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind {
            sink.lock().unwrap().push(format!("{database_key:?}"));
        }
    })));
    (Session::over(Database { storage }), log)
}

/// Takes the queries run since the last call, keeping only the parse queries.
fn parses(log: &Mutex<Vec<String>>) -> usize {
    let mut log = log.lock().unwrap();
    let count = log.iter().filter(|key| key.starts_with("parse(")).count();
    log.clear();
    count
}

#[test]
fn an_edit_reparses_only_the_edited_file() {
    let (mut session, log) = logged();
    let first = session.add_file("first.byn", "X: Final = 1\n").unwrap();
    session.add_file("second.byn", "Y: Final = 2\n").unwrap();
    assert!(session.check().is_empty());
    assert_eq!(parses(&log), 2);

    session.set_text(first, "X: Final = 3\n").unwrap();
    assert!(session.check().is_empty());
    assert_eq!(parses(&log), 1);

    assert!(session.check().is_empty());
    assert_eq!(parses(&log), 0);
}

#[test]
fn diagnostics_render_from_the_database() {
    let mut session = Session::new();
    session.add_file("physics/bad-name.byn", "").unwrap();
    session.add_file("main.byn", "x = \n").unwrap();
    let rendered: Vec<String> = session
        .check()
        .iter()
        .map(|diagnostic| diagnostics::render(diagnostic, &session).to_string())
        .collect();
    assert_eq!(
        rendered,
        [
            "error: `physics/bad-name.byn` cannot be imported: `bad-name` is not a valid \
             module name\n = note: module names follow Python's rules for names: letters, \
             digits and underscores, not starting with a digit",
            "error: expected an expression\n --> main.byn:1:5\n  |\n1 | x = \n  |     ^ an expression should be here",
        ]
    );
}

#[test]
fn files_get_ids_in_the_order_they_are_added() {
    let mut session = Session::new();
    let first = session.add_file("a.byn", "").unwrap();
    let second = session.add_file("b.byn", "").unwrap();
    assert_eq!(first.id(&session.database), FileId::from_index(0));
    assert_eq!(second.id(&session.database), FileId::from_index(1));
    assert_eq!(session.file(second.id(&session.database)).name(), "b.byn");
}

#[test]
fn every_use_of_a_name_points_to_its_definition() {
    let mut session = Session::new();
    let nbody = session
        .add_file("nbody.byn", "def step(dt: f64) -> f64:\n    return dt\n")
        .unwrap();
    let main = session
        .add_file(
            "main.byn",
            "from nbody import step\n\n\ndef main():\n    x = 1.0\n    x = step(x)\n",
        )
        .unwrap();
    let database = &session.database;
    let resolution = database.resolve(session.package, main);
    let tree = database.parse(main).tree();
    let text = main.source(database).text();
    let definition_of = |wanted: &str, occurrence: usize| {
        let node = tree
            .nodes()
            .iter()
            .enumerate()
            .filter(|(_, node)| {
                node.kind() == syntax::SyntaxKind::Name
                    && &text[node.start() as usize..node.end() as usize] == wanted
            })
            .nth(occurrence)
            .map(|(index, _)| index)
            .unwrap();
        resolution
            .references()
            .iter()
            .find(|(id, _)| id.index() == node)
            .map(|(_, definition)| definition.clone())
            .unwrap()
    };
    let Definition::Local(first) = definition_of("x", 0) else {
        panic!("`x` is not a local");
    };
    for occurrence in 1..3 {
        assert_eq!(definition_of("x", occurrence), Definition::Local(first));
    }
    let Definition::Item(file, node) = definition_of("step", 1) else {
        panic!("`step` is not an item");
    };
    assert_eq!(file, nbody);
    let nbody_tree = database.parse(nbody).tree();
    assert_eq!(nbody_tree.text(node, nbody.source(database).text()), "step");
}

#[test]
fn declarations_carry_types_and_values() {
    let mut session = Session::new();
    let program = "LATTICE: Final = 8\nN: Final = LATTICE**3\nBOX: Final = LATTICE * 1.5\n\
                   Vec3 = Fixed[f64, (3,)]\n\n\n\
                   def step(atoms: InOut[Array[Vec3, 1]], dt: f64) -> f64:\n    return dt\n";
    let main = session.add_file("main.byn", program).unwrap();
    let database = &session.database;
    let tree = database.parse(main).tree();
    let declaration = |index: usize| {
        let statement = tree.children(tree.root()).nth(index).unwrap();
        let name = tree
            .child(statement, Role::Target)
            .or_else(|| tree.child(statement, Role::Name))
            .unwrap();
        let item = Item::new(database, session.package, main, name);
        &database.declaration(item).declaration
    };
    let show = |ty: types::Type<'_>| ty.display(database).to_string();

    let Declaration::Constant { ty, value } = declaration(1) else {
        panic!("`N` is not a constant");
    };
    assert_eq!(
        (show(*ty), *value),
        ("i64".to_owned(), Some(Value::Integer(512)))
    );
    let Declaration::Constant { ty, value } = declaration(2) else {
        panic!("`BOX` is not a constant");
    };
    let twelve = Some(Value::Float(12.0_f64.to_bits()));
    assert_eq!((show(*ty), *value), ("f64".to_owned(), twelve));
    let Declaration::Alias(vec3) = declaration(3) else {
        panic!("`Vec3` is not an alias");
    };
    assert_eq!(show(*vec3), "Fixed[f64, (3,)]");
    let Declaration::Function(signature) = declaration(4) else {
        panic!("`step` is not a function");
    };
    let parameters: Vec<_> = signature
        .parameters
        .iter()
        .map(|parameter| (parameter.name.as_str(), show(parameter.ty), parameter.mode))
        .collect();
    assert_eq!(
        parameters,
        [
            (
                "atoms",
                "Array[Fixed[f64, (3,)], 1]".to_owned(),
                Mode::InOut
            ),
            ("dt", "f64".to_owned(), Mode::Borrow),
        ]
    );
    assert_eq!(show(signature.returns), "f64");
}

/// The body query records, for one function, the types lowering reads: a literal typed by
/// its context, a conversion, a method on a built-in type, a fallible call and its errors,
/// and the enum member each case tests.
#[test]
fn typed_bodies_record_what_lowering_reads() {
    let mut session = Session::new();
    let program = "class Bad(Exception):\n    pass\n\n\n\
                   class Side(Enum):\n    LEFT = 1\n    RIGHT = 2\n\n\n\
                   def solve(x: f64, scale: f64 = 1.0) -> Raises[f64, Bad]:\n    return x * scale\n\n\n\
                   def run(x: f32, side: Side, values: InOut[list[f64]]) -> Raises[f64, Bad]:\n\
                   \x20   y = x * 2\n\
                   \x20   z = f64(y) + solve(1.0)\n\
                   \x20   values.append(z)\n\
                   \x20   match side:\n\
                   \x20       case Side.LEFT:\n\
                   \x20           return z\n\
                   \x20       case Side.RIGHT:\n\
                   \x20           return -z\n";
    let main = session.add_file("main.byn", program).unwrap();
    let database = &session.database;
    let tree = database.parse(main).tree();
    let text = main.source(database).text();
    let run = tree.children(tree.root()).nth(3).unwrap();
    let name = tree.child(run, Role::Name).unwrap();
    let body = database.body(Item::new(database, session.package, main, name));
    let show = |ty: types::Type<'_>| ty.display(database).to_string();
    let node = |key: &types::Key| tree.text(key.0, text);

    assert!(body.diagnostics.is_empty(), "{:?}", body.diagnostics);
    let literal = body
        .expressions
        .keys()
        .find(|key| tree.kind(key.0) == syntax::SyntaxKind::Integer)
        .copied()
        .unwrap();
    assert_eq!(show(body.expressions[&literal]), "f32");
    let calls: Vec<String> = body
        .calls
        .iter()
        .map(|(key, call)| {
            let arguments: Vec<&str> = call
                .arguments
                .iter()
                .map(|argument| argument.as_ref().map_or("default", node))
                .collect();
            let callee = match &call.callee {
                types::Callee::Function(_) => "function".to_owned(),
                types::Callee::Builtin(name) => name.clone(),
                types::Callee::Conversion(ty) => format!("conversion to {}", show(*ty)),
                other => format!("{other:?}"),
            };
            format!("{}: {callee} {arguments:?}", node(key))
        })
        .collect();
    assert_eq!(
        calls,
        [
            r#"f64(y): conversion to f64 ["y"]"#,
            r#"solve(1.0): function ["1.0", "default"]"#,
            r#"values.append(z): list.append ["values", "z"]"#,
        ]
    );
    let errors: Vec<String> = body
        .errors
        .iter()
        .map(|(key, errors)| {
            let shown: Vec<String> = errors.iter().map(|&ty| show(ty)).collect();
            format!("{}: {shown:?}", node(key))
        })
        .collect();
    assert_eq!(errors, [r#"solve(1.0): ["Bad"]"#]);
    let patterns: Vec<(&str, u32)> = body
        .patterns
        .iter()
        .map(|(key, &position)| (node(key), position))
        .collect();
    assert_eq!(patterns, [("Side.LEFT", 0), ("Side.RIGHT", 1)]);
    let z = body
        .expressions
        .iter()
        .find(|(key, _)| node(key) == "z")
        .map(|(_, &ty)| show(ty));
    assert_eq!(z.as_deref(), Some("f64"));
}
