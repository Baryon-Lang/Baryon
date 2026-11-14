# 0057. The VS Code extension stays a `.vsix` file for now

Status: Accepted
Date: 2026-11-13
Roadmap: D.16

## Context

The VS Code extension ([decision 0025](0025-vscode-extension.md)) is installed from a `.vsix` file
attached to each release. Publishing it on the Visual Studio Marketplace under a `baryon-lang`
publisher would make installing it one click, but needs a publisher account and a token in CI,
and a Marketplace version could advertise features ahead of the latest released compiler.

## Decision

The extension is not published on the Marketplace yet. It stays a `.vsix` file attached to each
release, and the question is revisited closer to 1.0. Collins chose this on 2026-11-13.

## Consequences

- Installing takes one extra step, `code --install-extension baryon-<version>.vsix`, which
  [Coding Baryon in VS Code](../guide/vscode.md) shows.
- No publisher account or CI secret is needed, and the extension a user installs always matches a
  released compiler.
