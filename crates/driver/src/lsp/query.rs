//! The requests about a place in a document: hover, go to definition and completion, each a
//! translation of the [`Session`](driver::Session) query of the same name into the protocol.

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
}
