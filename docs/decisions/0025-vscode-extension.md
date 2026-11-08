# 0025. The VS Code extension lives in `editors/vscode/` and highlights with Python's grammar

Status: Accepted
Date: 2026-11-08
Roadmap: 1.17

## Context

The Phase 1 gate says a program "edits in VS Code with live diagnostics", and Collins asked for
the easiest way to support the language in an editor, with syntax highlighting a must. Roadmap
item 1.17 is the VS Code extension. Three choices in it are hard to reverse or reach past one
crate, so they get a record (`CONTRIBUTING.md` rule 7). None of them changes the language.

1. **Which editor?** A Baryon IDE of our own would be months of work that VS Code, the most used
   editor among the scientists we write for, already does. Any editor with a Language Server
   Protocol client gets diagnostics from `baryon lsp` today
   ([decision 0019](0019-language-server-protocol.md)); what one extension adds is the file
   association, highlighting and a client that starts the server.
2. **Where does it live?** The extension is not Rust and not documentation, so no current
   top-level directory fits, and the [README](../../README.md#repository-layout) says a new one
   needs a record.
3. **How does it highlight?** VS Code highlights with TextMate grammars: JSON files of regular
   expressions that give each piece of text a scope, such as `keyword.control`, which the color
   theme then styles. The options:
   - **A full Baryon grammar of our own.** Python's grammar is about 4,000 lines, and ours would
     track it forever for no difference a reader could see, because every Baryon file is valid
     Python syntax ([decision 0004](0004-python-syntax.md)). Item 1.3, a Tree-sitter grammar,
     was dropped for the same reason.
   - **Map `.byn` to Python.** No code at all, but the editor then calls the file Python, offers
     Python tools for it, and colors Baryon's own types (`f64`, `Array`, `InOut`) as plain names.
   - **A `source.baryon` grammar that includes VS Code's built-in `source.python`, plus a small
     injection grammar for the names Baryon adds.** An injection grammar is a set of rules VS Code
     applies inside another grammar at every nesting depth; ours is about 20 lines.

## Decision

- **VS Code first, through an extension, not an IDE of our own.** Other editors use `baryon lsp`
  directly and highlight `.byn` as Python until someone asks for more.
- **Editor integrations live in a new top-level directory, `editors/`, one subdirectory per
  editor.** The first is `editors/vscode/`. It is not a Cargo crate and the compiler never
  depends on it; it talks to the compiler only by running `baryon lsp`, the same interface any
  editor uses. This is the layout of rust-analyzer (`editors/code/`) and Ruff.
- **Highlighting is Python's grammar plus an injection.** `source.baryon` includes
  `source.python` whole, and `baryon.builtins.injection` marks Baryon's number types, array and
  struct types, parameter modes, `Raises`, the traits, `prange` and `simd` with scopes every color
  theme already styles. It skips strings, comments and f-strings.
- **The extension is packaged as a `.vsix` file and installed from it.** Publishing to the
  Visual Studio Marketplace waits for Collins, because it puts the name and the `baryon-lang`
  publisher in public.
- **The language server client, the second half of 1.17, is plain JavaScript on Microsoft's
  `vscode-languageclient` package**, the client nearly every VS Code language extension uses.
  Writing the protocol's client side ourselves would duplicate a maintained library for no gain,
  and plain JavaScript needs no compile step. Its pull request justifies the dependency as
  rule 6 asks.

## Consequences

- Highlighting tracks Python's grammar as VS Code updates it, at no cost to us. If Baryon ever
  leaves Python syntax, which decision 0004 rules out, the grammar has to be written.
- The highlighter does not know types. `f64` used as a variable name is still colored as a type,
  and a user's own struct is colored only where Python's grammar colors class names. Semantic
  tokens from `baryon lsp`, which color each name by what the checker resolved it to, are the
  fix, and need only a server change (idea I.25).
- A new name in the reference means a line in the injection grammar; its README says so. Nothing
  checks that the two agree.
- The extension's JavaScript dependencies sit outside `cargo xtask check-dependencies`, which
  covers only Rust crates. There are none until the client lands, and its pull request says how
  they are checked.
- `editors/` is part of the fixed layout from now on, and the README lists it.
