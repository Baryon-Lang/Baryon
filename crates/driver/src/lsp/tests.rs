//! Whole sessions with the server: an editor's messages in, the server's messages out, both
//! through the wire format, compared in full.

use super::*;

/// Frames each message as an editor would send it.
fn wire(messages: &[Value]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for message in messages {
        transport::write(&mut bytes, message).unwrap();
    }
    bytes
}

/// Runs a session over `messages` and returns the exit status and everything the server wrote.
fn session(messages: &[Value]) -> (u8, Vec<Value>) {
    let input = wire(messages);
    let mut output = Vec::new();
    let status = serve(input.as_slice(), &mut output).unwrap();
    let mut written = output.as_slice();
    let mut replies = Vec::new();
    while let Some(body) = transport::read(&mut written).unwrap() {
        replies.push(serde_json::from_slice(&body).unwrap());
    }
    (status, replies)
}

/// A request from the editor.
fn request(id: u32, method: &str, params: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

/// A notification from the editor.
fn notification(method: &str, params: &Value) -> Value {
    json!({"jsonrpc": "2.0", "method": method, "params": params})
}

/// `initialize` for a workspace at `root`, or for single files when `root` is `None`.
fn initialize(root: Option<&Path>) -> Value {
    let root = root.map_or(Value::Null, |root| Value::from(convert::uri(root)));
    request(
        1,
        "initialize",
        &json!({"processId": null, "rootUri": root}),
    )
}

/// The server's reply to [`initialize`].
fn initialized_reply() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "capabilities": {
                "positionEncoding": "utf-16",
                "textDocumentSync": {"openClose": true, "change": 1},
            },
            "serverInfo": {"name": "npl", "version": env!("CARGO_PKG_VERSION")},
        },
    })
}

/// The end of an orderly session: `shutdown`, its reply, and `exit`.
fn goodbye(id: u32) -> [Value; 2] {
    [
        request(id, "shutdown", &Value::Null),
        notification("exit", &Value::Null),
    ]
}

/// The reply to `shutdown` request `id`.
fn goodbye_reply(id: u32) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": null})
}

/// `didOpen` for `uri` with `text`.
fn open(uri: &str, text: &str) -> Value {
    notification(
        "textDocument/didOpen",
        &json!({"textDocument": {"uri": uri, "languageId": "npl", "version": 1, "text": text}}),
    )
}

/// `didChange` replacing the whole text of `uri`.
fn change(uri: &str, text: &str) -> Value {
    notification(
        "textDocument/didChange",
        &json!({"textDocument": {"uri": uri, "version": 2}, "contentChanges": [{"text": text}]}),
    )
}

/// `publishDiagnostics` for `uri`.
fn published(uri: &str, diagnostics: &Value) -> Value {
    notification(
        "textDocument/publishDiagnostics",
        &json!({"uri": uri, "diagnostics": diagnostics}),
    )
}

/// The diagnostic the parser gives for `x = ` at the end of line 0: it covers the line's end.
fn missing_expression(character: u32) -> Value {
    json!([{
        "range": {
            "start": {"line": 0, "character": character},
            "end": {"line": 1, "character": 0},
        },
        "severity": 1,
        "source": "npl",
        "message": "expected an expression\nan expression should be here",
    }])
}

#[test]
fn an_edit_publishes_new_diagnostics_and_a_fix_clears_them() {
    let uri = "file:///scratch/main.npl";
    let [shutdown, exit] = goodbye(2);
    let (status, replies) = session(&[
        initialize(None),
        notification("initialized", &json!({})),
        open(uri, "STEPS: Final = 100\n"),
        change(uri, "STEPS: Final = \n"),
        change(uri, "STEPS: Final = 10\n"),
        shutdown,
        exit,
    ]);
    assert_eq!(status, 0);
    assert_eq!(
        replies,
        [
            initialized_reply(),
            published(uri, &missing_expression(15)),
            published(uri, &json!([])),
            goodbye_reply(2),
        ]
    );
}

#[test]
fn the_workspace_is_loaded_so_imports_resolve() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/package");
    let main = convert::uri(&root.join("main.npl"));
    let [shutdown, exit] = goodbye(2);
    let (status, replies) = session(&[
        initialize(Some(&root)),
        notification("initialized", &json!({})),
        // The import of `energy` from the other file resolves, so this is clean. Misspelling
        // it is an error, and closing the file goes back to the clean text on disk.
        change(&main, "from physics.nbody import energ\n"),
        notification(
            "textDocument/didClose",
            &json!({"textDocument": {"uri": main}}),
        ),
        shutdown,
        exit,
    ]);
    assert_eq!(status, 0);
    assert_eq!(replies.len(), 4, "{replies:#?}");
    assert_eq!(replies[0], initialized_reply());
    let diagnostics = replies[1]["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(replies[1]["params"]["uri"], main);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
    assert_eq!(
        diagnostics[0]["range"]["start"],
        json!({"line": 0, "character": 26})
    );
    assert_eq!(replies[2], published(&main, &json!([])));
    assert_eq!(replies[3], goodbye_reply(2));
}

#[test]
fn diagnostics_without_a_location_are_shown_once() {
    let uri = "file:///scratch/bad-name.npl";
    let [shutdown, exit] = goodbye(2);
    let (_, replies) = session(&[
        initialize(None),
        open(uri, ""),
        change(uri, "X: Final = 1\n"),
        shutdown,
        exit,
    ]);
    assert_eq!(
        replies[1..3],
        [
            notification(
                "window/showMessage",
                &json!({
                    "type": 1,
                    "message": "error: `bad-name.npl` cannot be imported: `bad-name` is not a \
                                valid module name\n = note: module names follow Python's rules \
                                for names: letters, digits and underscores, not starting with a \
                                digit",
                }),
            ),
            goodbye_reply(2),
        ]
    );
}

#[test]
fn requests_follow_the_lifecycle() {
    let (status, replies) = session(&[
        request(1, "textDocument/hover", &json!({})),
        request(2, "initialize", &json!({"rootUri": null})),
        request(3, "initialize", &json!({"rootUri": null})),
        request(4, "textDocument/hover", &json!({})),
        request(5, "shutdown", &Value::Null),
        request(6, "shutdown", &Value::Null),
        notification("exit", &Value::Null),
    ]);
    assert_eq!(status, 0);
    let errors: Vec<(&Value, &Value)> = replies
        .iter()
        .filter_map(|reply| Some((&reply["id"], reply.get("error")?)))
        .collect();
    assert_eq!(
        errors,
        [
            (
                &json!(1),
                &json!({"code": -32002, "message": "the server is not initialized yet"})
            ),
            (
                &json!(3),
                &json!({"code": -32600, "message": "the server is already initialized"})
            ),
            (
                &json!(4),
                &json!({"code": -32601, "message": "`textDocument/hover` is not supported"})
            ),
            (
                &json!(6),
                &json!({"code": -32600, "message": "the server is shutting down"})
            ),
        ]
    );
}

#[test]
fn exit_without_shutdown_or_end_of_input_fails() {
    assert_eq!(session(&[notification("exit", &Value::Null)]).0, 1);
    assert_eq!(session(&[initialize(None)]).0, 1);
}

#[test]
fn a_body_that_is_not_json_gets_a_parse_error() {
    let mut input = b"Content-Length: 3\r\n\r\n{x}".to_vec();
    input.extend(wire(&[notification("exit", &Value::Null)]));
    let mut output = Vec::new();
    assert_eq!(serve(input.as_slice(), &mut output).unwrap(), 1);
    let body = transport::read(&mut output.as_slice()).unwrap().unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": {"code": -32700, "message": "the message is not valid JSON"},
        })
    );
}

#[test]
fn a_malformed_notification_is_reported() {
    let (_, replies) = session(&[
        initialize(None),
        notification("textDocument/didOpen", &json!({"textDocument": {}})),
    ]);
    assert_eq!(
        replies[1],
        notification(
            "window/showMessage",
            &json!({"type": 2, "message": "npl: ignored a malformed `textDocument/didOpen`"}),
        )
    );
}
