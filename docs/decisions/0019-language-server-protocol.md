# 0019. The language server is `baryon lsp`, speaking the protocol over `serde_json`

Status: Accepted
Date: 2026-11-07
Roadmap: 1.16

## Context

Roadmap item 1.16 is the language server: diagnostics as the user types first, then go to
definition, hover, completion and fixes. The design plan says it is "the compiler's query engine
behind the Language Server Protocol", so it must hold the same `driver::Session` that `baryon check`
uses, and every feature becomes a thin translation of a query result.

Two choices are hard to reverse once the server grows, which is why they get a record
(`CONTRIBUTING.md` rule 7). Neither changes the language.

1. **Where does the server live?** It needs `driver::Session`, which wires every implementation
   crate into the database. `docs/architecture.md` lets an application depend on foundation,
   interface and implementation crates, but not on another application, so a separate
   `language_server` application crate could not use `driver` without breaking the rules or
   duplicating the wiring.
2. **What speaks the protocol?** The Language Server Protocol is JSON-RPC 2.0 over standard input
   and output, each message preceded by a `Content-Length` header. The options:
   - **`tower-lsp`**: an async framework. It brings `tokio`, `futures` and their trees, and
     answers requests concurrently, which the compiler's single-threaded session does not need.
   - **`lsp-server` and `lsp-types`**, the crates rust-analyzer is built on: a synchronous
     message loop and typed structs for every message in the specification. Two new direct
     dependencies, with `serde` derives, a URI crate and `crossbeam-channel` behind them, for
     a server that today reads five notifications and two requests.
   - **Write the framing and the few message shapes we use, over `serde_json`.** `serde_json`
     is already a workspace dependency (the benchmark harness and `xtask` use it). The framing
     is about 40 lines; each message is a `json!` literal checked by a test against its full
     text.

## Decision

- **The server is a subcommand of the `baryon` binary: `baryon lsp`.** Its code is a private module
  of the `driver` crate's binary (`crates/driver/src/lsp/`), next to `baryon check`, so both share
  one `Session` and one way of finding a package's files. This is the layout of `ruff server`
  and `deno lsp`: one executable to install, and an editor extension only has to run it.
- **The protocol is written by hand over `serde_json::Value`.** The server reads and writes
  JSON-RPC messages itself and builds each reply with `json!`. No LSP framework, no `serde`
  derive, no async runtime.
- **One thread, one message at a time.** Each change runs `Session::check`, which the query
  database keeps incremental, then publishes the diagnostics of every file whose diagnostics
  changed. Requests are not cancelled, because nothing yet takes long enough to need it.
- **Full text on every change, positions in UTF-16.** The server asks for whole-document
  synchronization and converts byte offsets to the UTF-16 positions every client supports.

## Consequences

- No new dependency, and the binary stays small. The server builds and tests in the time the
  driver already takes.
- The protocol's shapes are ours to get right. Each message the server sends is tested by its
  full JSON text, and the tests play whole sessions through the wire format, so a wrong field
  name fails a test rather than an editor.
- Each new feature (hover, go to definition, completion) adds its message shapes by hand. When
  the hand-written shapes grow past what is easy to review, roughly when completion and code
  actions land, adopting `lsp-types` for the types alone is the natural next step and needs
  only this record superseded; the session, the framing and the tests stay.
- A slow query will block the editor until it finishes. When one does (type-checking every file
  of a large package on each keystroke is the likely first), the fix is a reader thread and
  cancellation through salsa, not a framework.
- `baryon lsp` grows the `driver` crate's binary, not its library: the library API gains only
  `Session::file_id`. If a second program ever needs the server, it moves to its own crate
  together with the wiring it needs, which is a change to `docs/architecture.md` and its own
  record.
