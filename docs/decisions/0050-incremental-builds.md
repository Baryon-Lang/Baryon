# 0050. Incremental builds: keep the compiler running rather than cache object files on disk

Status: Accepted
Date: 2026-11-12, accepted 2026-11-13
Roadmap: 2.2

## Context

The design plan sets a target of "incremental one-function edit under 200 ms": change one
function, rebuild, and wait less than a fifth of a second. Roadmap 2.2 planned to reach it with
"incremental code generation and object patching".

After roadmap 2.5 to 2.7 and the first passes of 2.3, a debug build of the 4,706-line program of
roadmap 2.0 takes about 0.6 s on 4 cores:

| Stage | Time | What it is |
| --- | --- | --- |
| `compile` | 0.26 s | `baryon` itself: parse, check, lower, IR passes, LLVM text |
| `machine code` | 0.28 s | `clang -O0`, one module per core, all at once (2.6) |
| `link` | 0.07 s | `clang` with `lld` (2.3) |

Each `baryon build` starts from nothing, so a one-line edit pays all three. There are two ways to
pay less.

**A. Cache object files on disk.** Each module's object file is kept, named by a hash of the
module's text, and reused when the text is unchanged. Functions must then go to modules by a
hash of their name rather than by size, so an edit leaves the other modules' text alone. This
spares only `clang`, and only when modules are small: with one module per core, the one edited
module still takes as long as all four together, since they run at once. Measured with 16
modules instead of 4, a full build's machine code took 0.42 s instead of 0.28 s, and an edit
would recompile one sixteenth, about 0.1 s. The edit build then takes about 0.26 + 0.1 + 0.07 =
0.43 s, against 0.6 s today, and every first build is 0.14 s slower. The front end's 0.26 s is
untouched, so 200 ms is out of reach. It also adds a side effect `baryon build` does not have
today: a directory under the user's cache directory (`$XDG_CACHE_HOME/baryon` or
`~/.cache/baryon` on Linux, `~/Library/Caches/baryon` on macOS) that grows with every edit, so
it needs a size limit and a way to clear it.

**B. Keep the compiler running.** The front end is already a `salsa` query database
([0016](0016-salsa-query-engine.md)), and `baryon lsp` keeps one alive while the editor is open.
A long-running build, `baryon watch`, would keep the database and the last build's per-function
LLVM text and object files in memory, rebuild when a file is saved, and redo only what the edit
reaches: re-check the edited function, re-lower it and the instances it changes, rewrite its LLVM
text, recompile only the module holding it, and link. This is how the design plan describes the
mechanism ("query invalidation re-checks and re-generates only the edited function"), and it is
the only option that also spares the front end. Nothing is written outside the build's own
scratch directory, so there is nothing to size or clear. The cost is a new command, and the
lowering and IR passes must become per-function queries so `salsa` can tell what an edit
reaches.

## Decision

Option B, which Collins chose on 2026-11-13 over option A and over doing neither. Roadmap 2.2 becomes "`baryon watch`: rebuild on save, redoing only what the
edit reaches", in two steps:

1. `baryon watch [--release] <directory>` rebuilds on each save in one process, reusing the
   checked front end that `salsa` already caches. It writes the executable as `build` does, and
   runs it with `--run`. This step alone spares parsing and checking of unchanged files.
2. Lowering, the IR passes and the backend become per-function queries, and modules keep their
   object files in memory between rebuilds, assigned by a hash of each function's symbol, so an
   edit recompiles one module.

`baryon build` keeps working as today: from nothing, with no cache.

## Consequences

- No new files on the user's disk, and `baryon build` stays reproducible from its sources alone.
- The 200 ms target is measured on `baryon watch`, not on `baryon build`. A cold `build` keeps
  its own target, 100,000 lines per second, which roadmap 2.3 works toward.
- `baryon watch` is a new command, so it is documented in the command reference and the guide.
- If option A is chosen instead, this record is rewritten to say where the cache lives, its size
  limit (proposed: 1 GB, oldest entries removed first) and `baryon clean` to clear it.
