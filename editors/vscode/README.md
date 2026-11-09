# Baryon for Visual Studio Code

This extension teaches VS Code the Baryon language: files ending in `.byn` open as Baryon, with
syntax highlighting, Python-style editing (comments, brackets, indentation and folding), and the
errors and warnings of `baryon check` underlined as you type, from
[`baryon lsp`](../../docs/reference/commands.md#baryon-lsp). Roadmap item 1.17; the reasons for
its shape are in [decision 0025](../../docs/decisions/0025-vscode-extension.md).

## Install

[Coding Baryon in VS Code](../../docs/guide/vscode.md) is the step-by-step setup for testers,
from building `baryon` to running a first program; this section is the short version.

The extension is not on the Visual Studio Marketplace yet. Install it from a `.vsix` file, a
packaged extension:

1. Package it. This needs [Node.js](https://nodejs.org/) 18 or later, and only on the machine
   that packages it:

   ```text
   cd editors/vscode
   npm ci
   npx @vscode/vsce package
   ```

   `npm ci` installs the language server client the extension runs on, at the exact versions in
   `package-lock.json`, into `node_modules/`. The second command writes `baryon-0.2.0.vsix` next
   to `package.json`, with those packages inside it (about 470 KB). Git ignores both.
2. In VS Code, open the Extensions view, choose **Install from VSIX...** in its `...` menu, and
   pick the file. From a terminal, `code --install-extension baryon-0.2.0.vsix` does the same.
3. Install `baryon` so VS Code can run it: build it as
   [The `baryon` command](../../docs/reference/commands.md) says, and put it on your `PATH` or set
   `baryon.path` (below).
4. Open the folder of a Baryon package, the folder holding its `.byn` files, with **File > Open
   Folder**, and open any `.byn` file. The language picker in the bottom right of the window shows
   **Baryon**, and errors are underlined and listed in the Problems view.

If you mapped `.byn` to Python in `files.associations` before, remove that line, or it wins over
the extension.

## What gets highlighted

Every Baryon file is valid Python syntax
([decision 0004](../../docs/decisions/0004-python-syntax.md)), so the grammar is two small files:

| File | What it does |
| --- | --- |
| [`syntaxes/baryon.tmLanguage.json`](syntaxes/baryon.tmLanguage.json) | The `source.baryon` grammar. It includes VS Code's built-in Python grammar, `source.python`, whole: keywords, strings, f-strings, numbers, comments, decorators and the rest |
| [`syntaxes/baryon-builtins.tmLanguage.json`](syntaxes/baryon-builtins.tmLanguage.json) | An injection grammar: rules VS Code applies inside `source.baryon` at every depth, before the Python rules. It marks the names Baryon adds to Python |

The injection colors these names with the scopes a color theme already styles, so every theme
works without changes:

| Names | Scope | Defined in |
| --- | --- | --- |
| `i8` to `i64`, `u8` to `u64`, `f32`, `f64`, `complex64`, `complex128` | `support.type.baryon` | [Types](../../docs/reference/types.md#numbers) |
| `Array`, `Fixed` | `support.type.baryon` | [Arrays](../../docs/reference/arrays.md) |
| `Struct`, `Enum`, `Copy`, `Final` | `support.type.baryon` | [Types](../../docs/reference/types.md) |
| `InOut`, `Owned` | `support.type.baryon` | [Values and references](../../docs/reference/values-and-references.md) |
| `Raises` | `support.type.baryon` | [Errors](../../docs/reference/errors.md) |
| `Number`, `Float`, `VectorSpace` | `support.type.baryon` | [Traits](../../docs/reference/types.md#traits) |
| `prange(...)`, `simd(...)` | `support.function.builtin.baryon` | [Parallel loops](../../docs/reference/parallel-loops.md) |

A name is matched only as a whole word and never after a dot, so `point.f64` and `my_f64` stay
plain. Strings, comments and f-strings are left alone, so `"f64"` stays a string; the one cost is
that a name inside an f-string's `{...}` is not colored either. When the reference adds a type
or a built-in function, add it to the matching rule in the injection grammar and to this table.

## Diagnostics

[`extension.js`](extension.js) is the extension's only code. The first time a `.byn` file opens,
it runs `baryon lsp` and connects it to VS Code through Microsoft's `vscode-languageclient`, the
client most VS Code language extensions use. From then on the server checks the package on every
change and VS Code shows what it finds. As the
[`baryon lsp`](../../docs/reference/commands.md#baryon-lsp) reference says, the server reads the
first folder of the window as one package.

| Setting | Default | What it does |
| --- | --- | --- |
| `baryon.path` | `baryon` | The program run as `baryon lsp`. A bare name is looked up on `PATH`; give a full path, such as `/Users/you/Baryon/target/release/baryon`, if `baryon` is not on it. Reload the window after changing it |

If the server cannot start, VS Code shows one error naming `baryon.path`, and highlighting keeps
working. If the server crashes, the client restarts it, up to four times in three minutes.

### Dependencies

The client is the extension's only JavaScript dependency. Writing the protocol's client side
ourselves would duplicate a maintained library: `vscode-languageclient` also handles document
synchronization, restarts and the Problems view. `package-lock.json` pins every package:

| Package | Version | License |
| --- | --- | --- |
| `vscode-languageclient` | 9.0.1 | MIT |
| `vscode-languageserver-protocol`, `vscode-languageserver-types` | 3.17.5 | MIT |
| `vscode-jsonrpc` | 8.2.0 | MIT |
| `minimatch` | 5.1.9 | ISC |
| `semver` | 7.8.6 | ISC |
| `brace-expansion`, `balanced-match` | 2.1.7, 1.0.2 | MIT |

ISC is a permissive license equivalent to MIT. Version 9 is used rather than 10 or 11 because
those depend on `minimatch` 10, which has a published denial-of-service advisory
([GHSA-g3hv-75wg-m249](https://github.com/advisories/GHSA-g3hv-75wg-m249)); `npm audit
--omit=dev` reports no advisory for this set. `cargo xtask check-dependencies` covers only Rust
crates, so run `npm audit --omit=dev` here after any change to `package-lock.json`. Version 9
also keeps the oldest supported VS Code at 1.82, from August 2023.

## Editing behavior

[`language-configuration.json`](language-configuration.json) follows VS Code's Python
configuration, trimmed to what Baryon accepts:

- `#` comments a line; triple quotes are the block comment, as in Python docstrings.
- Brackets and quotes close themselves, including `f"` and `f'`. The byte, raw and unicode string
  prefixes are left out because Baryon has no such literals.
- Pressing Enter after a line that ends in `:` and starts with `def`, `class`, `for`, `if`,
  `elif`, `else`, `while`, `try`, `except`, `finally`, `with`, `match` or `case` indents the next
  line.
- Blocks fold by indentation.

## How it was checked

There is no automated test yet; idea I.24 in the [roadmap](../../ROADMAP.md) is a check in CI.

The client was checked by driving `baryon lsp` with `vscode-jsonrpc` and
`vscode-languageserver-protocol`, the packages it sends every message through: `initialize`, then
`didOpen` of a file that reads an undefined name, which came back as one error diagnostic on the
right line and column, then `shutdown` and `exit`, which ended the server with status 0.

The grammar was checked by tokenizing every program in
[`docs/examples/`](../../docs/examples/README.md) with `vscode-textmate`, the library VS Code
itself highlights with, against VS Code's own Python grammar, and reading the scope of every
Baryon name, including ones in comments, strings and f-strings.
