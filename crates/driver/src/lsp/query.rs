//! The requests about a place in a document: hover, go to definition and completion, each a
//! translation of the [`Session`](driver::Session) query of the same name into the protocol;
//! and the one about a whole document, its inlay hints.

use super::{Server, convert};
use serde_json::{Value, json};
use source::Sources;

/// JSON-RPC error: the request's parameters are not what the method needs.
const INVALID_PARAMS: i64 = -32602;

impl Server {
    /// Answers a question about a place in a document: hover, go to definition or completion.
    /// A document the server does not know gets an empty answer, as one with nothing at the
    /// place would.
    pub(super) fn query(&self, method: &str, params: &Value) -> Result<Value, (i64, String)> {
        let malformed = || {
            (
                INVALID_PARAMS,
                format!("`{method}` needs a document and a position"),
            )
        };
        let uri = params["textDocument"]["uri"]
            .as_str()
            .ok_or_else(malformed)?;
        let line = params["position"]["line"].as_u64().ok_or_else(malformed)?;
        let character = params["position"]["character"]
            .as_u64()
            .ok_or_else(malformed)?;
        let Some(index) = self.find(uri) else {
            return Ok(Value::Null);
        };
        let file = self.documents[index].file;
        let source = self.session.file(self.session.file_id(file));
        let offset = convert::offset(source, line, character);
        Ok(match method {
            "textDocument/hover" => self
                .session
                .hover(file, offset)
                .map_or(Value::Null, |hover| {
                    json!({
                        "contents": {"kind": "markdown", "value": convert::markdown(&hover)},
                        "range": convert::range(&self.session, hover.span),
                    })
                }),
            "textDocument/definition" => {
                self.session
                    .definition(file, offset)
                    .map_or(Value::Null, |span| {
                        json!({
                            "uri": self.documents[span.file().index() as usize].uri,
                            "range": convert::range(&self.session, span),
                        })
                    })
            }
            _ => Value::Array(
                self.session
                    .complete(file, offset)
                    .iter()
                    .map(convert::completion)
                    .collect(),
            ),
        })
    }

    /// Answers `textDocument/inlayHint`: the inferred type of each variable in the document
    /// ([`Session::inlay_hints`](driver::Session::inlay_hints)). Every hint of the document is
    /// sent whatever range the editor asks for, since a document's hints cost one walk of its
    /// functions and the editor keeps those it shows. A document the server does not know has
    /// none.
    pub(super) fn inlay_hints(&self, params: &Value) -> Result<Value, (i64, String)> {
        let uri = params["textDocument"]["uri"].as_str().ok_or_else(|| {
            (
                INVALID_PARAMS,
                "`textDocument/inlayHint` needs a document".to_owned(),
            )
        })?;
        let Some(index) = self.find(uri) else {
            return Ok(Value::Array(Vec::new()));
        };
        let file = self.documents[index].file;
        let source = self.session.file(self.session.file_id(file));
        Ok(Value::Array(
            self.session
                .inlay_hints(file)
                .iter()
                .map(|hint| convert::inlay_hint(source, hint))
                .collect(),
        ))
    }
}
