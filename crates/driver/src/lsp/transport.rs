//! The wire format of the Language Server Protocol: JSON-RPC messages, each preceded by a
//! header that gives its length in bytes.
//!
//! ```text
//! Content-Length: 52\r\n
//! \r\n
//! {"jsonrpc":"2.0","method":"initialized","params":{}}
//! ```
//!
//! Only `Content-Length` matters. Other headers, such as `Content-Type`, are read and ignored,
//! as the specification allows.

use serde_json::Value;
use std::io::{self, BufRead, Write};

/// The largest message body read, in bytes. A whole source file travels in one message, so the
/// limit is generous; it exists so a corrupt header cannot make the server allocate gigabytes.
const LARGEST_BODY: usize = 256 * 1024 * 1024;

/// Reads the body of the next message, or `None` at the end of input.
///
/// # Errors
///
/// Fails when reading fails, when the header has no valid `Content-Length`, or when the input
/// ends in the middle of a message. The body is not parsed here, so a body that is not JSON is
/// still returned, and the caller answers it with a parse error.
pub(super) fn read(input: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut length = None;
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            return if length.is_none() {
                Ok(None)
            } else {
                Err(invalid("the input ended inside a message header"))
            };
        }
        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("Content-Length")
        {
            let value = value.trim().parse::<usize>();
            length = Some(value.map_err(|_| invalid("`Content-Length` is not a number"))?);
        }
    }
    let length = length.ok_or_else(|| invalid("a message header has no `Content-Length`"))?;
    if length > LARGEST_BODY {
        return Err(invalid("a message is larger than 256 MiB"));
    }
    let mut body = vec![0; length];
    input.read_exact(&mut body)?;
    Ok(Some(body))
}

/// Writes `message` with its header and flushes it, so the editor sees it at once.
///
/// # Errors
///
/// Fails when writing fails.
pub(super) fn write(output: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = message.to_string();
    write!(output, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    output.flush()
}

/// An error for input that breaks the wire format.
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_written_message_reads_back() {
        let mut wire = Vec::new();
        let message = json!({"jsonrpc": "2.0", "method": "initialized", "params": {}});
        write(&mut wire, &message).unwrap();
        assert_eq!(
            String::from_utf8(wire.clone()).unwrap(),
            "Content-Length: 52\r\n\r\n{\"jsonrpc\":\"2.0\",\"method\":\"initialized\",\"params\":{}}"
        );
        let mut input = wire.as_slice();
        let body = read(&mut input).unwrap().unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap(), message);
        assert!(read(&mut input).unwrap().is_none());
    }

    #[test]
    fn other_headers_are_ignored() {
        let mut input =
            "Content-Type: application/vscode-jsonrpc; charset=utf-8\r\ncontent-length: 2\r\n\r\n{}"
                .as_bytes();
        assert_eq!(read(&mut input).unwrap().unwrap(), b"{}");
    }

    #[test]
    fn a_header_without_a_length_is_an_error() {
        let mut input = "Content-Type: text\r\n\r\n{}".as_bytes();
        let error = read(&mut input).unwrap_err();
        assert_eq!(
            error.to_string(),
            "a message header has no `Content-Length`"
        );
    }

    #[test]
    fn input_that_ends_inside_a_body_is_an_error() {
        let mut input = "Content-Length: 10\r\n\r\n{}".as_bytes();
        assert_eq!(
            read(&mut input).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }
}
