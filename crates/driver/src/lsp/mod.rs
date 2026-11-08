//! `baryon lsp`: the language server, which gives an editor the compiler's diagnostics as the user
//! types.
//!
//! An editor starts `baryon lsp` and talks to it over standard input and output in the Language
//! Server Protocol (<https://microsoft.github.io/language-server-protocol/>), version 3.17.
//! The server holds one [`Session`], the same incremental database `baryon check` uses, so the
//! editor and the command line report exactly the same problems, and an edit reruns only the
//! queries that read the edited file. Decision 0019 records why the protocol is written here,
//! over `serde_json`, rather than taken from a framework, and why the server lives in the `baryon`
//! binary.
//!
//! # What it does today
//!
//! - **On `initialize`**, it loads every `.byn` file under the workspace root as one package, so
//!   imports between files resolve exactly as in `baryon check <root>`.
//! - **On `didOpen` and `didChange`**, the editor's text replaces the file's text. The whole
//!   text is sent on each change (full synchronization), which keeps the server simple; the
//!   query database, not the transfer, decides what is recomputed.
//! - **On `didClose`**, the file goes back to its text on disk.
//! - **After each of those**, it checks the package and publishes the diagnostics of every file
//!   whose diagnostics changed. A diagnostic with no location, such as a file name that is not a
//!   valid module name, is shown once as a message when it first appears.
//!
//! Go to definition, hover, completion and fixes are later roadmap items (1.16).
//!
//! # Example
//!
//! The editor's side of a session, as bytes on standard input:
//!
//! ```text
//! Content-Length: 89
//!
//! {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"rootUri":null}}
//! ```
//!
//! followed by `initialized`, `textDocument/didOpen` and so on. The module's tests play whole
//! sessions this way.

mod convert;
mod transport;

use crate::package;
use database::File;
use diagnostics::{Diagnostic, Severity};
use driver::Session;
use serde_json::{Value, json};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

/// JSON-RPC error: the message is not valid JSON.
const PARSE_ERROR: i64 = -32700;
/// JSON-RPC error: the request is not valid in the server's current state.
const INVALID_REQUEST: i64 = -32600;
/// JSON-RPC error: the server does not support the requested method.
const METHOD_NOT_FOUND: i64 = -32601;
/// LSP error: a request arrived before `initialize`.
const SERVER_NOT_INITIALIZED: i64 = -32002;

/// Runs the server until the editor sends `exit` or closes the input, reading messages from
/// `input` and writing replies and notifications to `output`. Returns the exit status: 0 after
/// an orderly `shutdown` and `exit`, 1 otherwise, as the protocol asks.
///
/// # Errors
///
/// Fails when reading or writing fails, or when the input breaks the wire format so badly that
/// the next message cannot be found. A message that is framed correctly but is not valid JSON
/// gets an error reply instead, and the server carries on.
pub(crate) fn serve(mut input: impl BufRead, mut output: impl Write) -> io::Result<u8> {
    let mut server = Server::new();
    while let Some(body) = transport::read(&mut input)? {
        let exit = server.receive(&body);
        for message in server.outbox.drain(..) {
            transport::write(&mut output, &message)?;
        }
        if let Some(status) = exit {
            return Ok(status);
        }
    }
    Ok(1)
}

/// Where the server is in the protocol's lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// Waiting for `initialize`. Every other request is refused.
    Waiting,
    /// Initialized: documents are synchronized and diagnostics published.
    Running,
    /// `shutdown` was received. Only `exit` is still meaningful.
    ShutDown,
}

/// One file in the session, as the editor knows it.
#[derive(Debug)]
struct Document {
    /// The file in the session.
    file: File,
    /// Where the file lives on disk, or `None` for a buffer the editor has not saved anywhere.
    path: Option<PathBuf>,
    /// The URI diagnostics are published under: the editor's own once it has opened the file,
    /// so it recognizes them, and one built from `path` before that.
    uri: String,
    /// The diagnostics last published for the file, so unchanged ones are not sent again.
    published: Vec<Value>,
}

/// The server's state between messages.
#[derive(Debug)]
struct Server {
    /// Where the server is in the protocol's lifecycle.
    phase: Phase,
    /// The package root, from the editor's workspace, or `None` when it opened single files.
    root: Option<PathBuf>,
    /// The compiler's incremental database, with every document's text.
    session: Session,
    /// Every file in `session`. A file's index here is the index of its [`source::FileId`],
    /// because both are assigned in the order files are added.
    documents: Vec<Document>,
    /// The rendered diagnostics without a location that were last shown.
    unlocated: Vec<String>,
    /// Messages waiting to be written to the editor, in order.
    outbox: Vec<Value>,
}

impl Server {
    /// A server waiting for `initialize`.
    fn new() -> Self {
        Self {
            phase: Phase::Waiting,
            root: None,
            session: Session::new(),
            documents: Vec::new(),
            unlocated: Vec::new(),
            outbox: Vec::new(),
        }
    }

    /// Handles one message body, queueing any replies in [`outbox`](Self::outbox). Returns the
    /// exit status once the editor has sent `exit`.
    fn receive(&mut self, body: &[u8]) -> Option<u8> {
        let Ok(message) = serde_json::from_slice::<Value>(body) else {
            self.respond(
                &Value::Null,
                Err((PARSE_ERROR, "the message is not valid JSON".to_owned())),
            );
            return None;
        };
        let params = message.get("params").unwrap_or(&Value::Null);
        match (
            message.get("method").and_then(Value::as_str),
            message.get("id"),
        ) {
            (Some(method), Some(id)) => {
                let result = self.request(method, params);
                self.respond(id, result);
            }
            (Some(method), None) => return self.notification(method, params),
            // A reply to a request; the server sends none, so there is nothing to match it to.
            (None, _) => {}
        }
        None
    }

    /// Answers one request.
    fn request(&mut self, method: &str, params: &Value) -> Result<Value, (i64, String)> {
        match (self.phase, method) {
            (Phase::Waiting, "initialize") => {
                self.phase = Phase::Running;
                Ok(self.initialize(params))
            }
            (Phase::Waiting, _) => Err((
                SERVER_NOT_INITIALIZED,
                "the server is not initialized yet".to_owned(),
            )),
            (Phase::Running, "shutdown") => {
                self.phase = Phase::ShutDown;
                Ok(Value::Null)
            }
            (Phase::Running, "initialize") => Err((
                INVALID_REQUEST,
                "the server is already initialized".to_owned(),
            )),
            (Phase::Running, _) => Err((METHOD_NOT_FOUND, format!("`{method}` is not supported"))),
            (Phase::ShutDown, _) => {
                Err((INVALID_REQUEST, "the server is shutting down".to_owned()))
            }
        }
    }

    /// Handles one notification. Returns the exit status for `exit`.
    fn notification(&mut self, method: &str, params: &Value) -> Option<u8> {
        if method == "exit" {
            return Some(u8::from(self.phase != Phase::ShutDown));
        }
        if self.phase != Phase::Running {
            return None;
        }
        let uri = params["textDocument"]["uri"].as_str();
        let handled = match method {
            "initialized" => Some(()),
            "textDocument/didOpen" => uri
                .zip(params["textDocument"]["text"].as_str())
                .map(|(uri, text)| self.open(uri, text)),
            "textDocument/didChange" => uri
                .zip(
                    params["contentChanges"]
                        .as_array()
                        .and_then(|changes| changes.last())
                        .and_then(|change| change["text"].as_str()),
                )
                .map(|(uri, text)| self.change(uri, text)),
            "textDocument/didClose" => uri.map(|uri| self.close(uri)),
            // Notifications the server does not use, such as `didSave`, may be ignored.
            _ => return None,
        };
        if handled.is_none() {
            self.show(
                Severity::Warning,
                &format!("baryon: ignored a malformed `{method}`"),
            );
        }
        self.publish();
        None
    }

    /// Loads the workspace's package and returns the server's capabilities.
    fn initialize(&mut self, params: &Value) -> Value {
        let root_uri = params["rootUri"]
            .as_str()
            .or_else(|| params["workspaceFolders"][0]["uri"].as_str());
        self.root = root_uri.and_then(convert::path);
        if let Some(root) = self.root.clone() {
            self.load(&root);
        }
        json!({
            "capabilities": {
                "positionEncoding": "utf-16",
                "textDocumentSync": {"openClose": true, "change": 1},
            },
            "serverInfo": {"name": "baryon", "version": env!("CARGO_PKG_VERSION")},
        })
    }

    /// Adds every `.byn` file under `root` to the session. A file that cannot be read is
    /// reported and left out, so one bad file does not stop the editor working on the rest.
    fn load(&mut self, root: &Path) {
        let paths = match package::files(root) {
            Ok(paths) => paths,
            Err(error) => {
                self.show(
                    Severity::Error,
                    &format!("baryon: {}: {error}", root.display()),
                );
                return;
            }
        };
        for path in paths {
            match fs::read_to_string(&path) {
                Ok(text) => self.add(convert::uri(&path), Some(path), text),
                Err(error) => {
                    self.show(
                        Severity::Error,
                        &format!("baryon: {}: {error}", path.display()),
                    );
                }
            }
        }
    }

    /// Adds a file to the session under the name `baryon check` would give it: its path relative
    /// to the root, or its file name when it lies outside the root.
    fn add(&mut self, uri: String, path: Option<PathBuf>, text: impl Into<String>) {
        let name = match (&path, &self.root) {
            (Some(path), Some(root)) if path.starts_with(root) => package::module_path(root, path),
            (Some(path), _) => path
                .file_name()
                .map_or_else(|| uri.clone(), |name| name.to_string_lossy().into_owned()),
            (None, _) => uri.clone(),
        };
        match self.session.add_file(name, text) {
            Ok(file) => self.documents.push(Document {
                file,
                path,
                uri,
                published: Vec::new(),
            }),
            Err(error) => self.show(Severity::Error, &format!("baryon: {error}")),
        }
    }

    /// The index of the document the editor calls `uri`, matching by path so that two
    /// spellings of one file's URI find the same document.
    fn find(&self, uri: &str) -> Option<usize> {
        let path = convert::path(uri);
        self.documents
            .iter()
            .position(|document| document.uri == uri || (path.is_some() && document.path == path))
    }

    /// The editor opened `uri` with `text`.
    fn open(&mut self, uri: &str, text: &str) {
        let Some(index) = self.find(uri) else {
            self.add(uri.to_owned(), convert::path(uri), text);
            return;
        };
        if self.documents[index].uri != uri {
            // Clear what was published under the old spelling, and publish again under the
            // editor's.
            let old = std::mem::replace(&mut self.documents[index].uri, uri.to_owned());
            if !std::mem::take(&mut self.documents[index].published).is_empty() {
                self.publish_to(&old, &[]);
            }
        }
        self.set_text(index, text);
    }

    /// The editor changed `uri` to `text`.
    fn change(&mut self, uri: &str, text: &str) {
        if let Some(index) = self.find(uri) {
            self.set_text(index, text);
        }
    }

    /// The editor closed `uri`: the file goes back to its text on disk, or to empty text when
    /// it has none, so a closed unsaved buffer reports nothing.
    fn close(&mut self, uri: &str) {
        if let Some(index) = self.find(uri) {
            let text = self.documents[index]
                .path
                .as_ref()
                .and_then(|path| fs::read_to_string(path).ok())
                .unwrap_or_default();
            self.set_text(index, &text);
        }
    }

    /// Replaces the text of document `index`.
    fn set_text(&mut self, index: usize, text: &str) {
        if let Err(error) = self.session.set_text(self.documents[index].file, text) {
            self.show(Severity::Error, &format!("baryon: {error}"));
        }
    }

    /// Checks the package and publishes every file's diagnostics that changed since they were
    /// last published, and shows any new diagnostic that has no location.
    fn publish(&mut self) {
        let mut located = vec![Vec::new(); self.documents.len()];
        let mut unlocated = Vec::new();
        let problems = self.session.check();
        for problem in &problems {
            let Some(primary) = problem.primary().map(diagnostics::Label::span) else {
                unlocated.push(problem);
                continue;
            };
            let uri_of =
                |span: source::Span| self.documents[span.file().index() as usize].uri.clone();
            located[primary.file().index() as usize].push(convert::diagnostic(
                problem,
                primary,
                &self.session,
                uri_of,
            ));
        }
        for (index, diagnostics) in located.into_iter().enumerate() {
            if self.documents[index].published != diagnostics {
                let uri = self.documents[index].uri.clone();
                self.publish_to(&uri, &diagnostics);
                self.documents[index].published = diagnostics;
            }
        }
        self.show_unlocated(&unlocated);
    }

    /// Shows each diagnostic without a location that was not shown last time.
    fn show_unlocated(&mut self, problems: &[&Diagnostic]) {
        let rendered: Vec<String> = problems
            .iter()
            .map(|problem| diagnostics::render(problem, &self.session).to_string())
            .collect();
        for (problem, text) in problems.iter().zip(&rendered) {
            if !self.unlocated.contains(text) {
                self.show(problem.severity(), text);
            }
        }
        self.unlocated = rendered;
    }

    /// Queues `textDocument/publishDiagnostics` for `uri`.
    fn publish_to(&mut self, uri: &str, diagnostics: &[Value]) {
        self.notify(
            "textDocument/publishDiagnostics",
            &json!({"uri": uri, "diagnostics": diagnostics}),
        );
    }

    /// Queues `window/showMessage`, which editors show as a notification.
    fn show(&mut self, severity: Severity, message: &str) {
        let kind = match severity {
            Severity::Error => 1,
            Severity::Warning => 2,
            Severity::Note => 3,
        };
        self.notify(
            "window/showMessage",
            &json!({"type": kind, "message": message}),
        );
    }

    /// Queues a notification.
    fn notify(&mut self, method: &str, params: &Value) {
        self.outbox
            .push(json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    /// Queues the reply to request `id`.
    fn respond(&mut self, id: &Value, result: Result<Value, (i64, String)>) {
        self.outbox.push(match result {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err((code, message)) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": code, "message": message},
            }),
        });
    }
}

#[cfg(test)]
mod tests;
