# Baryon for Visual Studio Code

This extension teaches VS Code the Baryon language: files ending in `.byn` open as Baryon, with
syntax highlighting and Python-style editing (comments, brackets, indentation and folding).
Roadmap item 1.17; the reasons for its shape are in
[decision 0025](../../docs/decisions/0025-vscode-extension.md).

Live diagnostics from [`baryon lsp`](../../docs/reference/commands.md#baryon-lsp) are not wired
in yet. That is the second half of item 1.17.

## Install

The extension is not on the Visual Studio Marketplace yet. Install it from a `.vsix` file, a
packaged extension:

1. Package it. This needs [Node.js](https://nodejs.org/) 18 or later, and only on the machine
   that packages it:

   ```text
   cd editors/vscode
   npx @vscode/vsce package
   ```

   This writes `baryon-0.1.0.vsix` next to `package.json`. Git ignores it.
2. In VS Code, open the Extensions view, choose **Install from VSIX...** in its `...` menu, and
   pick the file. From a terminal, `code --install-extension baryon-0.1.0.vsix` does the same.
3. Open any `.byn` file. The language picker in the bottom right of the window shows **Baryon**.

If you mapped `.byn` to Python in `files.associations` before, remove that line, or it wins over
the extension.

## What gets highlighted

Every Baryon file is valid Python syntax ([decision 0004](../../docs/decisions/0004-python-syntax.md)),
so the grammar is two small files:

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
The grammar was checked by tokenizing every program in [`docs/examples/`](../../docs/examples/README.md)
with `vscode-textmate`, the library VS Code itself highlights with, against VS Code's own Python
grammar, and reading the scope of every Baryon name, including ones in comments, strings and
f-strings.
