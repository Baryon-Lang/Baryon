# 0070. Baryon's logo is the Kanizsa B in brass on forest green

Status: Accepted
Date: 2026-11-21
Roadmap: 1.17.3

## Context

Baryon had no logo. The VS Code extension gave `.byn` files no icon, so editor tabs showed a blank
page beside them, and the extension had no icon of its own. The chief designer asked for a logo and
a file icon on 2026-11-21, and chose among four rounds of drawn concepts. The mark must read at 16
pixels, the size of a file icon on an editor tab, on light and dark themes.

## Decision

Baryon's official logo is the Kanizsa B in the "brass on forest" colours: a brass letter B on a
dark forest-green tile with a double brass frame, with three round cuts in the B, one per quark,
whose missing wedges imply a triangle that is not drawn
([`docs/brand/README.md`](../brand/README.md) describes it). The chief designer chose it on
2026-11-21 ("3 is it. Brass on Forest") and asked for it in the VS Code extension and as the
official logo in the GitHub repository.

- [`docs/brand/baryon-logo.svg`](../brand/baryon-logo.svg) holds it.
- The VS Code extension uses it as the `baryon` language's file icon and as its own icon
  ([`editors/vscode/README.md`](../../editors/vscode/README.md#icons)).
- The top of the README shows it.

## Consequences

- `.byn` files and the extension are recognisable in VS Code, except under a full file icon theme
  that does not know `.byn`, which draws its own icon instead; the extension's README and the VS
  Code guide say so.
- The organization's avatar and the repository's social preview are GitHub settings, changed by
  hand on the website, not by a pull request.
- No trademark or similarity search has been done on the mark. A conflict found later would mean
  a new record and a new mark.
