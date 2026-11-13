# 0055. Baryon is licensed Apache-2.0 OR MIT

Status: Accepted
Date: 2026-11-13
Roadmap: D.8

## Context

Baryon was licensed under the Apache License 2.0 alone. Most of the Rust ecosystem, Rust itself
included, is licensed under Apache-2.0 OR MIT: a user picks either. Apache-2.0 has a patent grant
that MIT lacks, but it is incompatible with GPLv2-only projects, which can use MIT code. The only
contributors so far are Collins and Claude, so relicensing is one pull request now and much harder
once outside contributions arrive. Every dependency is already checked to be usable under both
licenses ([`deny.toml`](../../deny.toml)).

## Decision

Baryon is licensed under either of the Apache License 2.0 ([`LICENSE`](../../LICENSE)) or the MIT
License ([`LICENSE-MIT`](../../LICENSE-MIT)), at the user's option. `Cargo.toml` declares
`license = "Apache-2.0 OR MIT"` for every crate. A contribution is licensed the same way, under
both, unless its author says otherwise. Collins chose this on 2026-11-13.

## Consequences

- GPLv2-only projects can reuse Baryon code, and Baryon matches the licensing Rust users expect.
- Third-party content and dependencies must stay compatible with both licenses, which
  `deny.toml` and [`THIRD-PARTY-NOTICES.md`](../../THIRD-PARTY-NOTICES.md) already require.
- Release archives and the VS Code extension carry both license files.
