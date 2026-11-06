//! End-to-end tests of the compiler's front end: the parser, then the syntax check.
//!
//! This crate holds tests and no library code. Each front-end stage is an implementation crate,
//! and an implementation crate may not depend on another, not even for its tests
//! (`docs/architecture.md`). So a test that needs real source text parsed by `parser` and then
//! checked by `syntax_check` lives here, in an application crate, the one kind allowed to wire
//! implementations together.
//!
//! The tests are in `tests/`:
//!
//! - `tests/corpus.rs` runs every program in `tests/corpus/` through both stages and compares
//!   the rendered diagnostics with the `.errors` file beside it, and checks that every example
//!   program in `docs/examples/` passes both stages with no diagnostics.
//!
//! Run them with `cargo test --package front_end_tests`.
